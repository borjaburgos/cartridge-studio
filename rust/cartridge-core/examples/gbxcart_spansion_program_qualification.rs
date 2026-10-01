//! Explicit destructive development qualification followed by exact restoration.
//! Does not bypass production source validation or expose a worker/API override.
use cartridge_core::{
    gb::{FlashWriter, RomReader},
    gbxcart::Reader,
    operations::{self, Journal},
    rom,
    storage::{self, Cancel},
    Error, Result,
};
use serde_json::{json, Value};
use std::path::Path;

// A byte-for-byte restoration of an independently verified full-capacity backup
// can have an invalid game header/checksum. This wrapper exists only in this
// explicit hardware example; normal GUI/TUI/CLI writes validate compatible ROMs.
struct Restore {
    reader: Reader,
    pinned: String,
}
impl RomReader for Restore {
    fn flash_capacity(&self) -> usize {
        rom::SPANSION_CAPACITY
    }
    fn flash_profile(&self) -> &'static str {
        rom::SPANSION_PROFILE
    }
    fn initialize(&mut self) -> Result<()> {
        self.reader.initialize()
    }
    fn header_bytes(&mut self) -> Result<Vec<u8>> {
        self.reader.header_bytes()
    }
    fn set_mapper(&mut self, m: String) {
        self.reader.set_mapper(m);
    }
    fn read_bank(&mut self, b: usize) -> Result<Vec<u8>> {
        self.reader.read_bank(b)
    }
    fn identity(&self) -> Value {
        self.reader.identity()
    }
    fn close(&mut self) -> Result<()> {
        self.reader.close()
    }
    fn identify_flash(&mut self) -> Result<Value> {
        self.reader.identify_flash()
    }
    fn enable_audio(&mut self) -> Result<()> {
        self.reader.enable_audio()
    }
}
impl FlashWriter for Restore {
    fn validate_program_source(&self, data: &[u8]) -> Result<Value> {
        if data.len() != rom::SPANSION_CAPACITY || rom::sha(data) != self.pinned {
            return Err(Error::check(
                "Restoration must exactly match the pinned, independently verified full backup.",
            ));
        }
        Ok(json!({"raw_restoration":true,"sha256":self.pinned,"bytes":data.len()}))
    }
    fn check_cancel(&self) -> Result<()> {
        self.reader.check_cancel()
    }
    fn prepare_program(&mut self) -> Result<()> {
        self.reader.prepare_program()
    }
    fn erase(&mut self) -> Result<()> {
        self.reader.erase()
    }
    fn program_bank(&mut self, b: usize, d: &[u8]) -> Result<()> {
        self.reader.program_bank(b, d)
    }
}
fn pattern() -> Vec<u8> {
    let mut bytes = vec![255; rom::SPANSION_CAPACITY];
    for bank in 0..256 {
        // Unique data in both halves and at both ends of every physical bank.
        for offset in [0, 0x2000, 0x3f00] {
            for n in 0..256 {
                bytes[bank * 16384 + offset + n] = (n as u8).wrapping_add(bank as u8);
            }
        }
    }
    bytes[0x100..0x104].copy_from_slice(&[0, 0xc3, 0x50, 1]);
    bytes[0x104..0x134].copy_from_slice(&rom::LOGO);
    bytes[0x134..0x150].fill(0);
    bytes[0x134..0x13d].copy_from_slice(b"FLASH QA ");
    bytes[0x147] = 0x19;
    bytes[0x148] = 7;
    bytes[0x14d] = bytes[0x134..0x14d]
        .iter()
        .fold(0u8, |a, b| a.wrapping_sub(*b).wrapping_sub(1));
    let sum = bytes.iter().fold(0u16, |a, b| a.wrapping_add(*b as u16));
    bytes[0x14e..0x150].copy_from_slice(&sum.to_be_bytes());
    bytes
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 6
        || !["--confirm-erase-program-restore", "--restore-only"].contains(&args[1].as_str())
    {
        return Err(Error::check("Usage: gbxcart_spansion_program_qualification --confirm-erase-program-restore|--restore-only PORT VERIFIED_BACKUP_DIRECTORY EXPECTED_SHA256 NEW_DIRECTORY"));
    }
    let baseline = Path::new(&args[3]);
    let report: Value = serde_json::from_slice(&std::fs::read(baseline.join("report.json"))?)?;
    let original = storage::read_rom(&baseline.join("before.read1.bin"))?;
    if report["status"] != "complete"
        || report["entire_rom_unchanged"] != true
        || original.len() != rom::SPANSION_CAPACITY
        || rom::sha(&original) != args[4]
        || original != std::fs::read(baseline.join("before.read2.bin"))?
        || original != std::fs::read(baseline.join("after-identification.bin"))?
    {
        return Err(Error::check("Three retained reads and the completed read-only report must match the pinned original backup."));
    }
    let directory = Path::new(&args[5]);
    std::fs::create_dir(directory)?;
    storage::write_new(&directory.join("original-full-capacity.bin"), &original)?;
    let mut progress = |s: String| println!("{s}");
    if args[1] != "--restore-only" {
        let source = pattern();
        rom::validate_gb_flash_profile(&source, rom::SPANSION_PROFILE)?;
        let mut j = Journal::new(&directory.join("pattern"), "gameboy", "write-qualification")?;
        let reader = Reader::open(&args[2], Cancel::default())?.with_spansion_profile()?;
        operations::gb_write_with(Box::new(reader), &mut j, Some(&source), &mut progress)?;
    }
    let mut j = Journal::new(
        &directory.join("restoration"),
        "gameboy",
        "restore-original",
    )?;
    let reader = Reader::open(&args[2], Cancel::default())?.with_spansion_profile()?;
    let r = Restore {
        reader,
        pinned: args[4].clone(),
    };
    let report = operations::gb_write_with(Box::new(r), &mut j, Some(&original), &mut progress)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{}", serde_json::to_string_pretty(&e).unwrap());
        std::process::exit(e.exit_code);
    }
}
