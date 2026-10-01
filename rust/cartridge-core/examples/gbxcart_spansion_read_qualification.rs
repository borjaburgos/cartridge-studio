//! Explicit physical-board investigation; never run by ordinary automated tests.
use cartridge_core::{
    gb::RomReader,
    gbxcart::Reader,
    operations::Journal,
    rom,
    storage::{self, Cancel},
    Error, Result,
};
use serde_json::json;
use std::path::Path;
const CAPACITY: usize = 4 * 1024 * 1024;
fn dump(reader: &mut Reader, journal: &Journal, name: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(CAPACITY);
    for bank in 0..256 {
        bytes.extend(reader.read_bank(bank)?);
        if bank % 16 == 15 {
            println!("{name}: {}/4096 KiB", bytes.len() / 1024);
        }
    }
    if bytes.len() != CAPACITY {
        return Err(Error::check("Incomplete capacity read."));
    }
    journal.put(name, &bytes)?;
    Ok(bytes)
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 || args[1] != "--confirmed-s29gl032m-mbc5" {
        return Err(Error::check("Usage: gbxcart_spansion_read_qualification --confirmed-s29gl032m-mbc5 PORT EXISTING_GAME_DUMP NEW_DIRECTORY"));
    }
    let expected = storage::read_rom(Path::new(&args[3]))?;
    if expected.len() < 0x150 || expected.len() > CAPACITY {
        return Err(Error::check(
            "Existing game must contain a header and fit within this diagnostic's capacity.",
        ));
    }
    let path = Path::new(&args[4]);
    storage::prepare_directory(path)?;
    let mut j = Journal::new(path, "gameboy", "spansion-read-only-qualification")?;
    let mut reader = Reader::open(&args[2], Cancel::default())?;
    let result = (|| {
        j.report["physical_board_basis"] = json!(
            "User photos: S29GL032M90TAIR4 flash, MBC5-labelled controller, battery-backed SRAM"
        );
        j.report["expected_game_sha256"] = json!(rom::sha(&expected));
        j.stage("reading-first-capacity-backup")?;
        reader.initialize_spansion_mbc5_readonly()?;
        let header = reader.header_bytes()?;
        if header != expected[..0x150] || reader.header_bytes()? != header {
            return Err(Error::new("CARTRIDGE_NOT_READY",
                "The cartridge header does not match the retained game at 3.3 V.",
                "Unplug USB, firmly reseat the photographed board, reconnect, and retry. Do not infer a blank chip or change the board's voltage jumpers from this result."));
        }
        j.report["device"] = reader.identity();
        let first = dump(&mut reader, &j, "before.read1.bin")?;
        if !first.starts_with(&expected) {
            return Err(Error::check("The read does not match the previously retained game. Keep both; stop before chip queries."));
        }
        j.stage("reading-second-capacity-backup")?;
        reader.initialize_spansion_mbc5_readonly()?;
        let header = reader.header_bytes()?;
        if header != expected[..0x150] || reader.header_bytes()? != header {
            return Err(Error::new("CARTRIDGE_NOT_READY",
                "The cartridge header does not match the retained game at 3.3 V.",
                "Unplug USB, firmly reseat the photographed board, reconnect, and retry. Do not infer a blank chip or change the board's voltage jumpers from this result."));
        }
        let second = dump(&mut reader, &j, "before.read2.bin")?;
        if first != second {
            return Err(Error::check(
                "Capacity backups disagree. Stop before chip queries; reconnect and reseat.",
            ));
        }
        j.report["backup_bytes"] = json!(first.len());
        j.report["identical_backups"] = json!(true);
        j.report["sha256"] = json!(rom::sha(&first));
        j.stage("identifying-flash-without-erase-or-program")?;
        let detection = reader.inspect_spansion_mbc5_readonly()?;
        println!("{}", serde_json::to_string_pretty(&detection)?);
        j.report["detection"] = detection;
        j.stage("checking-entire-rom-after-identification")?;
        let after = dump(&mut reader, &j, "after-identification.bin")?;
        j.report["entire_rom_unchanged"] = json!(after == first);
        if after != first {
            return Err(Error::check("ROM changed after identification. Keep all reads; unplug USB before further investigation."));
        }
        j.report["programming_qualified"] = json!(false);
        Ok(())
    })();
    let close = reader.close();
    let result = match (result, close) {
        (Err(mut e), Err(close)) => {
            e.details["cleanup_error"] = json!(close);
            Err(e)
        }
        (Err(e), _) | (_, Err(e)) => Err(e),
        (Ok(()), Ok(())) => Ok(()),
    };
    let report = j.finish(result)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{}", serde_json::to_string_pretty(&e).unwrap());
        std::process::exit(e.exit_code);
    }
}
