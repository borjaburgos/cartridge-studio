//! Explicit development-only hardware qualification; never run by routine tests.
use cartridge_core::{
    operations::Journal,
    operator, operator_programming,
    storage::{self, Cancel},
    Result,
};
use std::path::Path;
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 || args[1] != "--confirm-ferrante-512" {
        return Err(cartridge_core::Error::check("Usage: operator_ferrante_qualification --confirm-ferrante-512 SOURCE.gb EXPECTED_SHA256 NEW_BACKUP_DIRECTORY"));
    }
    let source = storage::read_rom(Path::new(&args[2]))?;
    if cartridge_core::rom::sha(&source) != args[3] {
        return Err(cartridge_core::Error::check(
            "Source SHA-256 differs from the reviewed release.",
        ));
    }
    cartridge_core::rom::validate_gb_flash(&source)?;
    let directory = Path::new(&args[4]);
    storage::prepare_directory(directory)?;
    let mut journal = Journal::new(directory, "gameboy", "write")?;
    let reader = operator::Reader::open(Cancel::default())?;
    let report = operator_programming::write_with(
        Box::new(reader.confirmed_ferrante512()),
        &mut journal,
        &source,
        &args[3],
        &mut |s| println!("{s}"),
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{}", serde_json::to_string_pretty(&e).unwrap());
        std::process::exit(e.exit_code);
    }
}
