//! Explicit read-only hardware investigation; never run by routine tests.
use cartridge_core::{
    gb::RomReader,
    operator::Reader,
    rom,
    storage::{self, Cancel},
    Error, Result,
};
use std::path::Path;
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 || args[1] != "--confirmed-ferrante-512" {
        return Err(Error::check(
            "Usage: operator_read_qualification --confirmed-ferrante-512 NEW_DIRECTORY",
        ));
    }
    let directory = Path::new(&args[2]);
    storage::prepare_directory(directory)?;
    std::fs::create_dir(directory)?;
    let mut reader = Reader::open(Cancel::default())?;
    let detection = reader.detect_flashcart()?;
    println!("{}", serde_json::to_string_pretty(&detection)?);
    let first = reader.read_confirmed_gb_capacity(rom::CAPACITY)?;
    storage::write_new(&directory.join("read1.bin"), &first)?;
    let second = reader.read_confirmed_gb_capacity(rom::CAPACITY)?;
    storage::write_new(&directory.join("read2.bin"), &second)?;
    let report = serde_json::json!({"detection":detection,"identity":reader.identity(),
        "bytes":first.len(),"sha256":rom::sha(&first),"second_sha256":rom::sha(&second),"match":first==second});
    storage::write_new(
        &directory.join("report.json"),
        &serde_json::to_vec_pretty(&report)?,
    )?;
    reader.close()?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if first != second {
        return Err(Error::check(
            "Independent Operator reads disagree; retain both files.",
        ));
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{}", serde_json::to_string_pretty(&e).unwrap());
        std::process::exit(e.exit_code);
    }
}
