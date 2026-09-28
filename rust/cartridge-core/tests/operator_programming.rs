use cartridge_core::{
    operations::Journal,
    operator_programming::{self, Programmer, Qualification, POLICY},
    rom::{self, CAPACITY, GB_PROFILE, LOGO},
    storage, Error, Result,
};
use serde_json::{json, Value};
use std::{cell::RefCell, fs, path::PathBuf, rc::Rc};

#[derive(Default)]
struct State {
    reads: usize,
    qualifications: usize,
    writes: usize,
    closed: bool,
    fault: &'static str,
    target: Vec<u8>,
}
struct Fake(Rc<RefCell<State>>);
impl Programmer for Fake {
    fn qualify(&mut self) -> Result<Qualification> {
        let mut s = self.0.borrow_mut();
        s.qualifications += 1;
        if s.fault == "unqualified" {
            return Err(operator_programming::unavailable());
        }
        Ok(Qualification {
            device_serial: if s.fault == "changed" && s.qualifications == 2 {
                "other"
            } else {
                "synthetic"
            }
            .into(),
            firmware: "synthetic-qualified-firmware".into(),
            profile: GB_PROFILE.into(),
            capacity: CAPACITY,
            identification: "electronic-flash-id".into(),
            manufacturer_id: Some(0xbf),
            device_id: Some(if s.fault == "wrong-chip" { 0xb6 } else { 0xb7 }),
        })
    }
    fn read_full(&mut self, size: usize) -> Result<Vec<u8>> {
        assert_eq!(size, CAPACITY);
        let mut s = self.0.borrow_mut();
        s.reads += 1;
        let mut data = if s.writes == 0 {
            vec![0x5a; size]
        } else {
            s.target.clone()
        };
        if (s.fault == "backup" && s.reads == 2)
            || (s.fault == "final1" && s.reads == 3)
            || (s.fault == "final2" && s.reads == 4)
        {
            // Corrupt the unused tail: verification must cover physical capacity.
            data[CAPACITY - 1] ^= 1;
        }
        if s.fault == "short" {
            data.pop();
        }
        Ok(data)
    }
    fn erase_and_program(&mut self, target: &[u8], _: &mut dyn FnMut(String)) -> Result<Value> {
        let mut s = self.0.borrow_mut();
        assert_eq!(s.reads, 2);
        assert_eq!(s.qualifications, 2);
        assert_eq!(target.len(), CAPACITY);
        s.writes += 1;
        s.target = target.to_vec();
        match s.fault {
            "disconnect" => Err(Error::new(
                "READER_DISCONNECTED",
                "Disconnected",
                "Reconnect",
            )),
            "timeout" => Err(Error::new("OPERATOR_USB_TIMEOUT", "Timed out", "Reconnect")),
            "reply" => Err(Error::new(
                "OPERATOR_UNEXPECTED_REPLY",
                "Unknown reply",
                "Keep report",
            )),
            "cancel-active" => Err(Error::interrupted()),
            _ => Ok(json!({"synthetic_transport_complete":true})),
        }
    }
    fn check_cancel(&self) -> Result<()> {
        let s = self.0.borrow();
        if s.fault == "cancel" || (s.fault == "cancel-after-backup" && s.reads == 2) {
            Err(Error::interrupted())
        } else {
            Ok(())
        }
    }
    fn close(&mut self) -> Result<()> {
        let mut s = self.0.borrow_mut();
        s.closed = true;
        if ["close", "disconnect"].contains(&s.fault) {
            Err(Error::new("CLOSE_FAILED", "Close failed", "Reconnect"))
        } else {
            Ok(())
        }
    }
}
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
struct Folder(PathBuf);
impl Folder {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tmp/operator-transaction-tests")
            .join(format!(
                "{}-{}-{}",
                std::process::id(),
                storage::stamp(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        Self(path)
    }
}
impl Drop for Folder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn source() -> Vec<u8> {
    let mut d = vec![0u8; 32 * 1024];
    d[0x104..0x134].copy_from_slice(&LOGO);
    d[0x147] = 0x19;
    d[0x14d] = d[0x134..0x14d]
        .iter()
        .fold(0u8, |sum, b| sum.wrapping_sub(*b).wrapping_sub(1));
    let sum = d
        .iter()
        .enumerate()
        .filter(|(i, _)| ![0x14e, 0x14f].contains(i))
        .fold(0u16, |s, (_, b)| s.wrapping_add(*b as u16));
    d[0x14e..0x150].copy_from_slice(&sum.to_be_bytes());
    d
}
fn run(fault: &'static str) -> (Folder, Rc<RefCell<State>>, Journal, Result<Value>) {
    let dir = Folder::new();
    let state = Rc::new(RefCell::new(State {
        fault,
        ..Default::default()
    }));
    let mut journal = Journal::new(&dir.0, "gameboy", "write").unwrap();
    let mut data = source();
    let digest = rom::sha(&data);
    if fault == "source" {
        data[0] ^= 1;
    }
    let result = operator_programming::write_with(
        Box::new(Fake(state.clone())),
        &mut journal,
        &data,
        &digest,
        &mut |_| {},
    );
    assert!(state.borrow().closed);
    (dir, state, journal, result)
}
#[test]
fn complete_write_retains_two_backups_and_two_fresh_full_readbacks() {
    let (dir, state, journal, result) = run("");
    result.unwrap();
    assert_eq!(state.borrow().reads, 4);
    assert_eq!(state.borrow().writes, 1);
    let r = journal.report;
    assert_eq!(r["status"], "complete");
    assert_eq!(r["programming_policy"], POLICY);
    assert_eq!(r["verification"]["backup_passes"], 2);
    assert_eq!(r["verification"]["final_passes"], 2);
    assert_eq!(r["verification"]["transport_ack_is_verification"], false);
    assert_eq!(
        r["verification"]["host_blank_check"],
        "unavailable_during_combined_operation"
    );
    assert!(r.get("blank_verified_bytes").is_none());
    assert!(r.get("banks_verified").is_none());
    assert_eq!(r["readback_sha256"], rom::sha(&source()));
    assert_eq!(fs::read(dir.0.join("source.gb")).unwrap(), source());
    assert_eq!(fs::read(dir.0.join("readback.gb")).unwrap(), source());
    for name in [
        "before.read1.bin",
        "before.read2.bin",
        "after.read1.bin",
        "after.read2.bin",
    ] {
        assert_eq!(
            fs::metadata(dir.0.join(name)).unwrap().len(),
            CAPACITY as u64
        );
    }
}
#[test]
fn preflight_failures_never_erase_or_program() {
    for (fault, code) in [
        ("source", "SOURCE_CHANGED"),
        ("unqualified", "OPERATOR_PROGRAMMING_NOT_QUALIFIED"),
        ("wrong-chip", "OPERATOR_PROGRAMMING_NOT_QUALIFIED"),
        ("backup", "BACKUP_MISMATCH"),
        ("changed", "CARTRIDGE_CHANGED"),
        ("short", "OPERATOR_FULL_READ_LENGTH"),
        ("cancel", "INTERRUPTED"),
        ("cancel-after-backup", "INTERRUPTED"),
    ] {
        let (_dir, state, journal, result) = run(fault);
        assert_eq!(result.unwrap_err().code, code, "{fault}");
        assert_eq!(state.borrow().writes, 0, "{fault}");
        assert_ne!(journal.report["status"], "complete");
    }
}
#[test]
fn bad_final_reads_retain_evidence_and_never_report_success() {
    for (fault, pass) in [("final1", 1), ("final2", 2)] {
        let (dir, _, journal, result) = run(fault);
        let error = result.unwrap_err();
        assert_eq!(error.code, "FINAL_VERIFY_FAILED");
        assert_eq!(error.details["pass"], pass);
        assert_eq!(journal.report["failed_during"], "verifying");
        assert_eq!(journal.report["verification"]["final_passes"], pass - 1);
        assert_eq!(
            fs::metadata(dir.0.join(format!("after.read{pass}.bin")))
                .unwrap()
                .len(),
            CAPACITY as u64
        );
        assert!(!dir.0.join("readback.gb").exists());
    }
}
#[test]
fn transport_failures_preserve_backups_and_destructive_stage() {
    for (fault, code) in [
        ("disconnect", "READER_DISCONNECTED"),
        ("timeout", "OPERATOR_USB_TIMEOUT"),
        ("reply", "OPERATOR_UNEXPECTED_REPLY"),
        ("cancel-active", "INTERRUPTED"),
    ] {
        let (dir, state, journal, result) = run(fault);
        let error = result.unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(state.borrow().reads, 2);
        assert_eq!(journal.report["failed_during"], "erasing_and_programming");
        assert!(dir.0.join("source.gb").exists());
        assert!(dir.0.join("before.read2.bin").exists());
        if fault == "disconnect" {
            assert_eq!(error.details["cleanup_error"]["error"], "CLOSE_FAILED");
        }
    }
}
#[test]
fn cleanup_failure_preserves_verified_output_but_does_not_report_complete() {
    let (dir, _, journal, result) = run("close");
    assert_eq!(result.unwrap_err().code, "READER_CLEANUP_FAILED");
    assert_eq!(journal.report["status"], "failed");
    assert!(dir.0.join("readback.gb").exists());
}
