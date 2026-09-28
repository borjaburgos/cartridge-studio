//! Durable backup / erase / program / verify transactions. All hardware is injected.
use crate::{
    famicom, gb,
    rom::{self, Board, NesRom, CAPACITY},
    storage::{self, Cancel},
    usb::Bus,
    Error, Result,
};
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub struct Journal {
    pub directory: PathBuf,
    pub report: Value,
}
impl Journal {
    pub fn new(directory: &Path, platform: &str, operation: &str) -> Result<Self> {
        fs::create_dir(directory)?;
        let j = Self {
            directory: directory.into(),
            report: json!({"operation":operation,"platform":platform,"status":"preflight","started_utc":storage::stamp(),"directory":directory,"backend":"rust","backend_version":crate::VERSION}),
        };
        j.save()?;
        Ok(j)
    }
    pub fn save(&self) -> Result<()> {
        storage::save_report(&self.directory.join("report.json"), &self.report)
    }
    pub fn stage(&mut self, s: &str) -> Result<()> {
        self.report["status"] = json!(s);
        self.save()
    }
    pub fn put(&self, name: &str, data: &[u8]) -> Result<PathBuf> {
        let p = self.directory.join(name);
        storage::write_new(&p, data)?;
        Ok(p)
    }
    pub fn finish(&mut self, r: Result<()>) -> Result<Value> {
        match r {
            Ok(()) => {
                self.stage("complete")?;
                Ok(self.report.clone())
            }
            Err(mut e) => {
                e.details["report"] = json!(self.directory.join("report.json"));
                e.details["backup_directory"] = json!(self.directory);
                self.report["failed_during"] = self.report["status"].clone();
                self.report["status"] = json!(if e.code == "INTERRUPTED" {
                    "interrupted"
                } else {
                    "failed"
                });
                self.report["error"] = json!(e);
                let _ = self.save();
                Err(e)
            }
        }
    }
}
fn file(path: &Path) -> Result<File> {
    Ok(OpenOptions::new().write(true).create_new(true).open(path)?)
}
fn mismatch(code: &str, message: impl Into<String>) -> Error {
    Error::new(code,message,"Retain the raw reads, original backup and saved source. Unplug USB, clean and reseat the cartridge, then retry into a new folder. Restore the saved source if erasing or programming had begun.").exit(5)
}
fn same(a: &[u8], b: &[u8], code: &str, message: &str) -> Result<()> {
    if a == b {
        Ok(())
    } else {
        Err(mismatch(code, message))
    }
}
fn blank(data: &[u8]) -> bool {
    data.iter().all(|&b| b == 255)
}
fn gb_dump(
    r: &mut dyn gb::RomReader,
    j: &Journal,
    name: &str,
    banks: usize,
    progress: &mut dyn FnMut(String),
) -> Result<Vec<u8>> {
    let mut f = file(&j.directory.join(name))?;
    let mut data = Vec::with_capacity(banks * 16384);
    for n in 0..banks {
        let bytes = r.read_bank(n)?;
        f.write_all(&bytes)?;
        data.extend(bytes);
        progress(format!("{name}: {}/{} KiB", (n + 1) * 16, banks * 16));
    }
    f.sync_all()?;
    storage::sync_parent(&j.directory.join(name))?;
    Ok(data)
}
fn nes_dump(
    r: &mut famicom::Reader,
    j: &Journal,
    prefix: &str,
    progress: &mut dyn FnMut(String),
) -> Result<(Vec<u8>, Vec<u8>)> {
    let path = j.directory.join(format!("{prefix}.prg.bin"));
    let mut f = file(&path)?;
    let banks = r.board.prg_capacity / 16384;
    let mut data = Vec::with_capacity(r.board.prg_capacity);
    for n in 0..banks {
        let bytes = r.read_bank(n)?;
        f.write_all(&bytes)?;
        data.extend(bytes);
        if (n + 1) % 8 == 0 || n + 1 == banks {
            progress(format!("{prefix}: {}/{} KiB", (n + 1) * 16, banks * 16));
        }
    }
    f.sync_all()?;
    storage::sync_parent(&path)?;
    let chr = if r.board.chr_capacity > 0 {
        let d = r.block(false, 0, r.board.chr_capacity)?;
        j.put(&format!("{prefix}.chr.bin"), &d)?;
        d
    } else {
        vec![]
    };
    Ok((data, chr))
}
pub(crate) fn cleanup<T>(result: Result<T>, close: Result<()>) -> Result<T> {
    match (result, close) {
        (Ok(v), Ok(())) => Ok(v),
        (Err(mut e), Err(c)) => {
            e.details["cleanup_error"] = json!(c);
            Err(e)
        }
        (Err(e), _) => Err(e),
        (_, Err(e)) => Err(Error::new(
            "READER_CLEANUP_FAILED",
            "Cartridge access finished, but the reader could not return to its idle state.",
            "Keep the output and report. Reconnect USB before another operation; leave BL alone.",
        )
        .details(json!({"reason":e}))),
    }
}

#[allow(clippy::too_many_arguments)] // Explicit transaction inputs, independently checked by the service.
pub fn gb_read(
    bus: Bus,
    j: &mut Journal,
    action: &str,
    full: bool,
    source: Option<&[u8]>,
    passes: usize,
    strict: bool,
    progress: &mut dyn FnMut(String),
) -> Result<Value> {
    gb_read_with(
        Box::new(gb::Reader::new(bus)),
        j,
        action,
        full,
        source,
        passes,
        strict,
        progress,
    )
}
#[allow(clippy::too_many_arguments)]
pub fn gb_read_with(
    mut r: Box<dyn gb::RomReader>,
    j: &mut Journal,
    action: &str,
    full: bool,
    source: Option<&[u8]>,
    passes: usize,
    strict: bool,
    progress: &mut dyn FnMut(String),
) -> Result<Value> {
    let result = (|| {
        j.report["device"] = r.identity();
        let banks = if full {
            j.report["flash"] = r.identify_flash()?;
            32
        } else {
            r.initialize()?;
            let info = rom::gb_header(&r.header_bytes()?)?;
            rom::validate_gb(&info)?;
            r.set_mapper(info["mapper"].as_str().unwrap().to_owned());
            info["rom_banks"].as_u64().unwrap() as usize
        };
        let passes = if action == "read" { passes } else { 2 };
        j.stage("reading")?;
        let mut first = Vec::new();
        for n in 1..=passes {
            r.initialize()?;
            if full {
                r.enable_audio()?;
            }
            let actual = gb_dump(r.as_mut(), j, &format!("read{n}.bin"), banks, progress)?;
            if n > 1 {
                same(
                    &first,
                    &actual,
                    "BACKUP_MISMATCH",
                    "The two cartridge reads differ.",
                )?;
            }
            first = actual;
        }
        j.report["read_passes"] = json!(passes);
        j.report["identical_reads"] = if passes == 2 {
            json!(true)
        } else {
            Value::Null
        };
        j.report["hashes"] = rom::hashes(&first);
        let output = if action == "verify" {
            let expected = source.ok_or_else(|| Error::check("Verification requires a source."))?;
            let mut target = expected.to_vec();
            if full {
                if target.len() > CAPACITY {
                    return Err(Error::check(
                        "The source exceeds the physical flash capacity.",
                    ));
                }
                target.resize(CAPACITY, 255);
            }
            same(
                &first,
                &target,
                "CARTRIDGE_MISMATCH",
                "The cartridge differs from the loaded ROM.",
            )?;
            Some(expected.to_vec())
        } else if full {
            match rom::gb_header(&first).and_then(|i| {
                rom::validate_gb(&i)?;
                let size = i["rom_bytes"].as_u64().unwrap() as usize;
                if size > first.len() {
                    return Err(Error::check("Declared ROM exceeds flash capacity."));
                }
                Ok(size)
            }) {
                Ok(n) => Some(first[..n].to_vec()),
                Err(_) => {
                    j.report["warning"]=json!("The complete flash backup is preserved, but its header is blank or invalid. It is not a playable ROM.");
                    None
                }
            }
        } else {
            Some(first)
        };
        if let Some(data) = output {
            let checks = rom::gb_checks(&data)?;
            rom::validate_gb(&checks)?;
            if checks["size_valid"] != true {
                return Err(mismatch(
                    "GB_SIZE_FAILED",
                    "The read does not match its declared ROM size.",
                ));
            }
            let suffix = if checks["cgb_flag"].as_u64().unwrap() & 128 != 0 {
                "gbc"
            } else {
                "gb"
            };
            let path = j.put(
                &format!(
                    "{}.{suffix}",
                    if action == "verify" {
                        "readback"
                    } else {
                        "cartridge"
                    }
                ),
                &data,
            )?;
            j.report["output"] = json!(path);
            j.report["cartridge"] = checks.clone();
            if checks["global_checksum_valid"] != true {
                j.report["warning"]=json!("The bytes were preserved exactly, but the ROM's built-in global checksum is incorrect.");
                if strict && action == "read" {
                    return Err(Error::new("GB_CHECKSUM_FAILED",j.report["warning"].as_str().unwrap(),"Keep the dump and compare a known checksum. For an intentional prototype or modified ROM, turn off strict global checksum validation and read again.").details(json!({"output":path})));
                }
            }
        }
        Ok(())
    })();
    // Initialization can discover firmware and cartridge transport details.
    j.report["device"] = r.identity();
    let close = r.close();
    drop(r);
    j.finish(cleanup(result, close))
}

pub fn gb_write(
    bus: Bus,
    j: &mut Journal,
    source: Option<&[u8]>,
    progress: &mut dyn FnMut(String),
) -> Result<Value> {
    gb_write_with(Box::new(gb::Reader::new(bus)), j, source, progress)
}
pub fn gb_write_with(
    mut r: Box<dyn gb::FlashWriter>,
    j: &mut Journal,
    source: Option<&[u8]>,
    progress: &mut dyn FnMut(String),
) -> Result<Value> {
    if let Some(data) = source {
        j.put("source.gb", data)?;
        j.report["source"] = json!(j.directory.join("source.gb"));
        j.report["source_info"] = rom::validate_gb_flash(data)?;
        j.save()?;
    }
    let result = (|| {
        j.report["device"] = r.identity();
        j.report["flash"] = r.identify_flash()?;
        j.stage("backing_up")?;
        let first = gb_dump(r.as_mut(), j, "before.read1.bin", 32, progress)?;
        r.initialize()?;
        r.enable_audio()?;
        let second = gb_dump(r.as_mut(), j, "before.read2.bin", 32, progress)?;
        same(
            &first,
            &second,
            "BACKUP_MISMATCH",
            "The two full-chip reads differ. Erase/write was not started.",
        )?;
        j.report["backup_sha256"] = json!(rom::sha(&first));
        j.report["backup_bytes"] = json!(CAPACITY);
        j.stage("backed_up")?;
        // Repeat identity after backup, then prove the helper before the first destructive command.
        let verified = r.identify_flash()?;
        if verified != j.report["flash"] {
            return Err(mismatch(
                "CARTRIDGE_CHANGED",
                "Cartridge identification changed during the backup.",
            ));
        }
        if source.is_some() {
            r.prepare_program()?;
        }
        r.check_cancel()?;
        j.stage("erasing")?;
        progress("Erasing the complete flash chip…".into());
        r.erase()?;
        let erased = gb_dump(r.as_mut(), j, "erased.bin", 32, progress)?;
        if !blank(&erased) {
            return Err(mismatch(
                "BLANK_CHECK_FAILED",
                "Some flash bytes are still programmed after erase. Programming was stopped.",
            ));
        }
        j.report["blank_verified_bytes"] = json!(erased.len());
        j.stage("erased")?;
        if let Some(data) = source {
            let mut target = data.to_vec();
            target.resize(CAPACITY, 255);
            j.stage("programming")?;
            for (bank, expected) in data.as_chunks::<16384>().0.iter().enumerate() {
                r.program_bank(bank, expected)?;
                let actual = r.read_bank(bank)?;
                if actual != expected {
                    j.put(&format!("failed-bank-{bank:02}.bin"), &actual)?;
                    return Err(mismatch(
                        "BANK_VERIFY_FAILED",
                        format!("Bank {bank} did not match after programming."),
                    ));
                }
                j.report["banks_verified"] = json!(bank + 1);
                j.save()?;
                progress(format!(
                    "Written and verified: {}/{} KiB",
                    (bank + 1) * 16,
                    data.len() / 1024
                ));
            }
            j.stage("verifying")?;
            let mut actual = Vec::new();
            for n in 1..=2 {
                r.initialize()?;
                r.enable_audio()?;
                actual = gb_dump(r.as_mut(), j, &format!("after.read{n}.bin"), 32, progress)?;
                same(&actual,&target,"FINAL_VERIFY_FAILED","Complete readback differed from the target. Do not treat this write as successful.")?;
            }
            let readback = &actual[..data.len()];
            j.report["output"] = json!(j.put("readback.gb", readback)?);
            j.report["identical_final_reads"] = json!(true);
            j.report["full_chip_sha256"] = json!(rom::sha(&actual));
            j.report["readback_sha256"] = json!(rom::sha(readback));
            j.report["unused_flash_blank"] = json!(blank(&actual[data.len()..]));
        } else {
            // Wipe also gets an independent, power-cycled final read.
            r.initialize()?;
            r.enable_audio()?;
            let second = gb_dump(r.as_mut(), j, "erased.read2.bin", 32, progress)?;
            same(
                &erased,
                &second,
                "BLANK_CHECK_FAILED",
                "The second full-chip blank check failed.",
            )?;
            j.report["identical_final_reads"] = json!(true);
        }
        Ok(())
    })();
    let close = r.close();
    drop(r);
    j.finish(cleanup(result, close))
}

#[allow(clippy::too_many_arguments)] // Explicit transaction inputs, independently checked by the service.
pub fn nes_operation(
    bus: Bus,
    j: &mut Journal,
    action: &str,
    board: Board,
    automatic: bool,
    source: Option<&NesRom>,
    passes: usize,
    progress: &mut dyn FnMut(String),
) -> Result<Value> {
    if let Some(rom) = source {
        j.put("source.nes", &rom.raw)?;
        j.report["source"] = rom.summary();
        j.save()?;
    }
    let mut r = famicom::Reader::new(bus, board);
    let result = (|| {
        if automatic {
            j.report["detection"] = r.detect(progress)?["detection"].clone();
        }
        j.report["board"] = json!(board);
        let hardware = r.inspect()?;
        let mirror = hardware["mirroring"].as_str().unwrap().to_owned();
        j.report["hardware"] = hardware.clone();
        if let Some(rom) = source {
            j.report["compatibility"] =
                rom::compatibility(rom, board, Some(&mirror), action == "write")?;
        }
        j.stage("backing_up")?;
        let first = nes_dump(&mut r, j, "before.read1", progress)?;
        let passes = if action == "read" { passes } else { 2 };
        if passes == 2 {
            r.initialize()?;
            let second = nes_dump(&mut r, j, "before.read2", progress)?;
            if first != second {
                return Err(mismatch(
                    "BACKUP_MISMATCH",
                    "The two complete cartridge reads differ. Erase/write was not started.",
                ));
            }
        }
        j.report["backup"] = json!({"identical_reads":if passes==2{json!(true)}else{Value::Null},"read_passes":passes,"prg_sha256":rom::sha(&first.0),"chr_sha256":rom::sha(&first.1),"prg_bytes":first.0.len(),"chr_bytes":first.1.len(),"blank_prg":blank(&first.0)});
        let backup = rom::nes_build(&first.0, &first.1, board, &mirror)?;
        j.put("before.nes", &backup)?;
        j.stage("backed_up")?;
        if action == "read" || action == "backup" {
            j.report["output"] = json!(j.put("cartridge.nes", &backup)?);
            j.report["rom_sha256"] = json!(rom::sha(&backup));
            j.report["bootable_not_guaranteed"] = json!(true);
            if blank(&first.0) {
                j.report["warning"]=json!("The flash is blank. The backup is complete, but it is not a playable game. Write a compatible ROM.");
            } else if first
                .0
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .len()
                <= 2
            {
                j.report["warning"]=json!("The read contains a repeated or constant pattern. Keep the backup, check seating, and verify in an emulator before treating it as playable.");
            }
            return Ok(());
        }
        let target = source.map(|s| s.prg.repeat(board.prg_capacity / s.prg.len()));
        let actual = if action == "verify" {
            let rom = source.unwrap();
            same(
                &first.0,
                target.as_ref().unwrap(),
                "FINAL_VERIFY_FAILED",
                "The cartridge reads agree but differ from the supplied ROM.",
            )?;
            same(
                &first.1,
                &rom.chr,
                "FINAL_VERIFY_FAILED",
                "Graphics ROM differs from the supplied ROM.",
            )?;
            first.0
        } else {
            let recheck = r.inspect()?;
            if recheck != hardware {
                return Err(mismatch(
                    "CARTRIDGE_CHANGED",
                    "Cartridge identification changed during the operation.",
                ));
            }
            if let Some(rom) = source {
                rom::compatibility(rom, board, Some(&mirror), true)?;
            }
            r.bus.cancel.check()?;
            j.stage("erasing")?;
            progress("Erasing the complete flash chip…".into());
            r.erase()?;
            let erased = nes_dump(&mut r, j, "erased", progress)?;
            if !blank(&erased.0) {
                return Err(mismatch(
                    "BLANK_CHECK_FAILED",
                    "Some flash bytes are still programmed after erase. Programming was stopped.",
                ));
            }
            j.report["blank_verified_bytes"] = json!(erased.0.len());
            if action == "wipe" {
                return Ok(());
            }
            let target = target.as_ref().unwrap();
            j.put("target-prg.bin", target)?;
            j.report["target_prg_sha256"] = json!(rom::sha(target));
            j.stage("programming")?;
            for (bank, expected) in target.as_chunks::<16384>().0.iter().enumerate() {
                if !blank(expected) {
                    r.program_bank(bank, expected)?;
                }
                let actual = r.read_bank(bank)?;
                if actual != expected {
                    j.put(&format!("failed-bank-{bank:02}.bin"), &actual)?;
                    return Err(mismatch(
                        "BANK_VERIFY_FAILED",
                        format!("Program bank {bank} did not match the source after writing."),
                    ));
                }
                j.report["banks_verified"] = json!(bank + 1);
                j.save()?;
                progress(format!(
                    "Written and verified: {}/{} KiB",
                    (bank + 1) * 16,
                    board.prg_capacity / 1024
                ));
            }
            j.stage("verifying")?;
            let mut actual = Vec::new();
            for n in 1..=2 {
                r.initialize()?;
                actual = nes_dump(&mut r, j, &format!("after.read{n}"), progress)?.0;
                same(&actual,target,"FINAL_VERIFY_FAILED","Complete verification differed from the target. Do not treat this write as successful.")?;
            }
            actual
        };
        let rom = source.unwrap();
        let mut readback = rom.raw[..16].to_vec();
        readback.extend(&actual[..rom.prg.len()]);
        if action == "verify" {
            readback.extend(&first.1);
        }
        same(
            &readback,
            &rom.raw,
            "READBACK_ROM_MISMATCH",
            "The reconstructed ROM differs from the source.",
        )?;
        j.report["output"] = json!(j.put("readback.nes", &readback)?);
        j.report["identical_final_reads"] = json!(true);
        j.report["full_prg_sha256"] = json!(rom::sha(&actual));
        j.report["readback_sha256"] = json!(rom::sha(&readback));
        Ok(())
    })();
    let close = r.close();
    drop(r);
    j.finish(cleanup(result, close))
}

pub fn diagnostic(error: &mut Error, root: &Path) {
    let directory = root.join("diagnostics");
    if fs::create_dir_all(&directory).is_ok() {
        let p = directory.join(format!("{}-{}.json", storage::stamp(), std::process::id()));
        if storage::write_new(&p, &serde_json::to_vec_pretty(error).unwrap_or_default()).is_ok() {
            error.details["diagnostic_log"] = json!(p);
        }
    }
}
pub fn open(cancel: Cancel) -> Result<Bus> {
    Bus::open(cancel)
}
