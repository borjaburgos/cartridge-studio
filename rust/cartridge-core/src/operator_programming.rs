//! Operator-specific transaction policy. Hardware activation is separately gated.
//!
//! Unlike bank-addressable writers, the Operator owns erase + program as one
//! operation. An ACK never counts as a blank check or a ROM readback.
use crate::{
    operations::{cleanup, Journal},
    rom::{self, CAPACITY},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const POLICY: &str = "operator-combined-verified-v1";

pub fn unavailable() -> Error {
    Error::new(
        "OPERATOR_PROGRAMMING_NOT_QUALIFIED",
        "GB Operator programming is awaiting hardware qualification.",
        "Use a qualified Ferrante profile with GBxCart RW or INLretro for now. The Operator needs a confirmed physical flash identity and validated programming responses before writing can be enabled; changing the ROM profile cannot bypass this.",
    )
}

/// Evidence from a qualified hardware adapter, never inferred from a ROM header.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Qualification {
    pub device_serial: String,
    pub firmware: String,
    pub profile: String,
    pub capacity: usize,
    pub manufacturer_id: u8,
    pub device_id: u8,
}
impl Qualification {
    fn validate(&self) -> Result<()> {
        if self.device_serial.is_empty()
            || self.firmware.is_empty()
            || self.profile != rom::GB_PROFILE
            || self.capacity != CAPACITY
            || self.manufacturer_id != 0xbf
            || self.device_id != 0xb7
        {
            return Err(unavailable().details(json!({"qualification":self})));
        }
        Ok(())
    }
}

/// Implement only after the firmware/profile pair has been physically qualified.
/// There is deliberately no USB implementation until that requirement is met.
pub trait Programmer {
    /// Must freshly inspect the physical board and validate firmware applicability.
    fn qualify(&mut self) -> Result<Qualification>;
    /// Must issue a fresh complete hardware read, regardless of header ROM size.
    fn read_full(&mut self, capacity: usize) -> Result<Vec<u8>>;
    /// Owns erase, programming, validated device replies and bounded timeouts.
    /// Must not report completion on an unrecognized reply. Returns transport
    /// evidence, not verification evidence. Must honor cancellation while active.
    fn erase_and_program(
        &mut self,
        target: &[u8],
        progress: &mut dyn FnMut(String),
    ) -> Result<Value>;
    fn check_cancel(&self) -> Result<()>;
    fn close(&mut self) -> Result<()>;
}

fn failure(code: &str, message: &str) -> Error {
    Error::new(code, message,
        "Keep the operation folder with its source, backups and report. If programming started, reconnect USB before recovery and restore a retained image using a qualified writer. Do not treat this operation as verified.")
}

fn read_pass(
    r: &mut dyn Programmer,
    j: &Journal,
    name: &str,
    progress: &mut dyn FnMut(String),
) -> Result<Vec<u8>> {
    r.check_cancel()?;
    progress(format!("{name}: reading the complete 512 KiB flash…"));
    let data = r.read_full(CAPACITY)?;
    // Retain even a short response for diagnosis; never accept it as a backup.
    j.put(name, &data)?;
    if data.len() != CAPACITY {
        return Err(failure(
            "OPERATOR_FULL_READ_LENGTH",
            "The Operator did not return the complete flash capacity.",
        )
        .details(json!({"expected":CAPACITY,"received":data.len(),"file":name})));
    }
    Ok(data)
}

/// A pinned, verified ROM write. Wipe requires its own hardware qualification.
/// `expected_sha256` comes from the reviewed source, not a subsequent file read.
pub fn write_with(
    mut r: Box<dyn Programmer>,
    j: &mut Journal,
    source: &[u8],
    expected_sha256: &str,
    progress: &mut dyn FnMut(String),
) -> Result<Value> {
    let result = (|| {
        j.report["programming_policy"] = json!(POLICY);
        j.report["verification"] = json!({
            "backup_passes":0,"final_passes":0,
            "host_blank_check":"unavailable_during_combined_operation",
            "immediate_bank_readback":"unavailable_during_combined_operation",
            "transport_ack_is_verification":false
        });
        j.stage("validating_source")?;
        let digest = rom::sha(source);
        if digest != expected_sha256 {
            return Err(failure(
                "SOURCE_CHANGED",
                "The ROM no longer matches the source reviewed for this write.",
            ));
        }
        j.report["source_info"] = rom::validate_gb_flash(source)?;
        j.report["source"] = json!(j.put("source.gb", source)?);
        j.report["source_sha256"] = json!(digest);
        let mut target = source.to_vec();
        target.resize(CAPACITY, 0xff);
        j.report["target_sha256"] = json!(rom::sha(&target));
        j.stage("qualifying")?;
        r.check_cancel()?;
        let qualification = r.qualify()?;
        qualification.validate()?;
        j.report["qualification"] = json!(qualification);
        j.stage("backing_up")?;
        let first = read_pass(r.as_mut(), j, "before.read1.bin", progress)?;
        j.report["verification"]["backup_passes"] = json!(1);
        j.save()?;
        let second = read_pass(r.as_mut(), j, "before.read2.bin", progress)?;
        j.report["verification"]["backup_passes"] = json!(2);
        if first != second {
            return Err(failure(
                "BACKUP_MISMATCH",
                "The two full-chip backups differ. Erase/write was not started.",
            ));
        }
        j.report["backup_sha256"] = json!(rom::sha(&first));
        j.report["backup_bytes"] = json!(CAPACITY);
        j.report["verification"]["identical_backups"] = json!(true);
        j.stage("backed_up")?;
        let recheck = r.qualify()?;
        recheck.validate()?;
        if qualification != recheck {
            return Err(failure(
                "CARTRIDGE_CHANGED",
                "Physical identification changed after backup. Erase/write was not started.",
            ));
        }
        r.check_cancel()?;
        // The durable stage precedes the first possibly destructive USB command.
        // A failure from this point on must be presented as potentially destructive.
        j.stage("erasing_and_programming")?;
        progress("Operator firmware is erasing and programming. Verification follows two fresh full reads.".into());
        j.report["transport_evidence"] = r.erase_and_program(&target, progress)?;
        j.stage("verifying")?;
        for n in 1..=2 {
            let actual = read_pass(r.as_mut(), j, &format!("after.read{n}.bin"), progress)?;
            if actual != target {
                return Err(failure("FINAL_VERIFY_FAILED", "A full-chip readback differs from the target. The write is not verified.")
                    .details(json!({"pass":n,"actual_sha256":rom::sha(&actual),"expected_sha256":rom::sha(&target)})));
            }
            j.report["verification"]["final_passes"] = json!(n);
            j.save()?;
            if n == 2 {
                let readback = &actual[..source.len()];
                j.report["output"] = json!(j.put("readback.gb", readback)?);
                j.report["readback_sha256"] = json!(rom::sha(readback));
                j.report["full_chip_sha256"] = json!(rom::sha(&actual));
            }
        }
        j.report["identical_final_reads"] = json!(true);
        j.report["unused_flash_blank"] = json!(true);
        Ok(())
    })();
    let close = r.close();
    j.finish(cleanup(result, close))
}
