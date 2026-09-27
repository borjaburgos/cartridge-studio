use crate::{rom, usb::Bus, Error, Result};
use serde_json::{json, Value};
use std::time::Duration;
pub struct Reader {
    pub bus: Bus,
    pub mapper: Option<String>,
    pub audio: bool,
    pub flash_verified: bool,
    pub helper_base: Option<u32>,
    pub helper_code: Vec<u8>,
    pub mailbox: Option<Vec<u8>>,
    closed: bool,
}
impl Reader {
    pub fn new(bus: Bus) -> Self {
        Self {
            bus,
            mapper: None,
            audio: false,
            flash_verified: false,
            helper_base: None,
            helper_code: vec![],
            mailbox: None,
            closed: false,
        }
    }
    pub fn initialize(&mut self) -> Result<()> {
        self.bus.cancel.check()?;
        self.bus.cmd(2, 0, 0, 0)?;
        self.bus.cmd(2, 5, 0, 0)?;
        self.bus.cmd(2, 9, 0, 0)?;
        std::thread::sleep(Duration::from_millis(100));
        Ok(())
    }
    pub fn read_byte(&mut self, a: u16) -> Result<u8> {
        self.bus.byte(12, 0, a)
    }
    pub fn header_bytes(&mut self) -> Result<Vec<u8>> {
        let mut v = vec![0; 0x100];
        for a in 0x100..0x150 {
            self.bus.cancel.check()?;
            v.push(self.read_byte(a)?);
        }
        Ok(v)
    }
    pub fn bank_write(&mut self, a: u16, v: u8) -> Result<()> {
        if self.mapper.is_none() || ![0x2000, 0x2100, 0x3000, 0x4000, 0x6000].contains(&a) {
            return Err(Error::check("Disallowed bank register write."));
        }
        self.bus.cmd(12, 1, a, v)
    }
    pub fn select_bank(&mut self, bank: usize) -> Result<u16> {
        let mapper = self.mapper.clone();
        select_bank(mapper.as_deref(), bank, &mut |address, value| {
            self.bank_write(address, value)
        })
    }
    pub fn read_bank(&mut self, bank: usize) -> Result<Vec<u8>> {
        self.bus.cancel.check()?;
        let a = self.select_bank(bank)?;
        self.bus.block(0x26dd, a, 16384)
    }
    pub fn detect(&mut self) -> Result<Value> {
        detect(self)
    }
    pub fn identity(&self) -> Value {
        let mut v = self.bus.identity();
        v["voltage"] = json!(5);
        v
    }
    pub fn close(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        // Run every cleanup step even when another step fails or cancellation is set.
        let mut first = None;
        macro_rules! clean {
            ($e:expr) => {
                if let Err(e) = $e {
                    if first.is_none() {
                        first = Some(e);
                    }
                }
            };
        }
        if self.audio {
            clean!(self.bus.cmd(1, 5, 15, 0));
            clean!(self.bus.cmd(1, 8, 0, 0));
            clean!(self.command(&[(0, 0xf0)]));
            clean!(self.bus.cmd(1, 1, 15, 0));
        }
        if self.mapper.is_some() {
            clean!(self.select_bank(0));
            if self.mapper.as_deref() == Some("MBC1") {
                clean!(self.bank_write(0x6000, 0));
            }
        }
        clean!(self.bus.stop());
        clean!(self.bus.cmd(2, 0, 0, 0));
        match first {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            eprintln!("Reader cleanup: {e}");
        }
    }
}

/// Cartridge-level reading interface. Reader protocols stay behind this boundary.
pub trait RomReader {
    fn initialize(&mut self) -> Result<()>;
    fn header_bytes(&mut self) -> Result<Vec<u8>>;
    fn set_mapper(&mut self, mapper: String);
    fn read_bank(&mut self, bank: usize) -> Result<Vec<u8>>;
    fn identity(&self) -> Value;
    fn close(&mut self) -> Result<()>;
    fn identify_flash(&mut self) -> Result<Value> {
        Err(crate::readers::unsupported("full flash access"))
    }
    fn enable_audio(&mut self) -> Result<()> {
        Err(crate::readers::unsupported("AUDIO flash wiring"))
    }
}
/// Board-specific programming beneath the shared durable transaction.
pub trait FlashWriter: RomReader {
    fn check_cancel(&self) -> Result<()>;
    fn prepare_program(&mut self) -> Result<()>;
    fn erase(&mut self) -> Result<()>;
    fn program_bank(&mut self, bank: usize, data: &[u8]) -> Result<()>;
}
impl FlashWriter for Reader {
    fn check_cancel(&self) -> Result<()> {
        self.bus.cancel.check()
    }
    fn prepare_program(&mut self) -> Result<()> {
        self.prepare_helper()
    }
    fn erase(&mut self) -> Result<()> {
        self.erase()
    }
    fn program_bank(&mut self, bank: usize, data: &[u8]) -> Result<()> {
        if bank >= 32 || data.len() != 16384 {
            return Err(Error::check("Invalid flash bank."));
        }
        self.load_helper()?;
        for (index, chunk) in data.chunks(116).enumerate() {
            if !chunk.iter().all(|&b| b == 255) {
                self.program_chunk(bank, index * 116, chunk)?;
            }
        }
        Ok(())
    }
}
impl RomReader for Reader {
    fn initialize(&mut self) -> Result<()> {
        self.initialize()
    }
    fn header_bytes(&mut self) -> Result<Vec<u8>> {
        self.header_bytes()
    }
    fn set_mapper(&mut self, mapper: String) {
        self.mapper = Some(mapper);
    }
    fn read_bank(&mut self, bank: usize) -> Result<Vec<u8>> {
        self.read_bank(bank)
    }
    fn identity(&self) -> Value {
        self.identity()
    }
    fn close(&mut self) -> Result<()> {
        self.close()
    }
    fn identify_flash(&mut self) -> Result<Value> {
        self.identify_flash()
    }
    fn enable_audio(&mut self) -> Result<()> {
        self.enable_audio()
    }
}
pub fn select_bank(
    mapper: Option<&str>,
    bank: usize,
    write: &mut dyn FnMut(u16, u8) -> Result<()>,
) -> Result<u16> {
    let max = match mapper {
        Some("ROM") => 2,
        Some("MBC1" | "MBC3") => 128,
        Some("MBC2") => 16,
        Some("MBC5") => 512,
        _ => return Err(Error::check("Read requires a validated mapper.")),
    };
    if bank >= max {
        return Err(Error::check("Bank exceeds the mapper's address range."));
    }
    match mapper.unwrap() {
        "ROM" => return Ok((bank * 0x4000) as u16),
        "MBC1" => {
            write(0x6000, u8::from(bank.is_multiple_of(32)))?;
            write(0x4000, (bank >> 5) as u8)?;
            write(0x2000, (bank as u8 & 31).max(1))?;
            return Ok(if bank.is_multiple_of(32) { 0 } else { 0x4000 });
        }
        "MBC5" => {
            write(0x3000, (bank >> 8) as u8)?;
            write(0x2000, bank as u8)?;
        }
        m => {
            let a = if m == "MBC2" { 0x2100 } else { 0x2000 };
            write(a, bank.max(1) as u8)?;
        }
    }
    Ok(if bank == 0 { 0 } else { 0x4000 })
}
pub fn detect(reader: &mut dyn RomReader) -> Result<Value> {
    reader.initialize()?;
    let raw = reader.header_bytes()?;
    if raw != reader.header_bytes()? {
        return Err(Error::new(
            "CARTRIDGE_UNSTABLE",
            "The Game Boy header changed between reads.",
            "Unplug USB, clean and reseat the cartridge, then detect again.",
        )
        .exit(3));
    }
    let mut i = rom::gb_header(&raw)?;
    rom::validate_gb(&i)?;
    let ram = if i["mapper"] == "MBC2" {
        Some(256)
    } else {
        match i["ram_size_code"].as_u64() {
            Some(0) => Some(0),
            Some(1) => Some(2048),
            Some(2) => Some(8192),
            Some(3) => Some(32768),
            Some(4) => Some(131072),
            Some(5) => Some(65536),
            _ => None,
        }
    };
    i["ram_bytes"] = json!(ram);
    let name = format!(
        "{} · {} · {} KiB ROM",
        if i["cgb_flag"].as_u64().unwrap() & 128 != 0 {
            "Game Boy Color"
        } else {
            "Game Boy"
        },
        i["mapper"].as_str().unwrap(),
        i["rom_bytes"].as_u64().unwrap() / 1024
    );
    Ok(
        json!({"device":reader.identity(),"cartridge":i,"profile":"auto","detection":{"status":"identified","platform":"gameboy","profile":"auto","name":name,"summary":name,"basis":"cartridge-header","writable":false,"evidence":["Two identical headers.","Boot logo and header checksum match."],"limitation":"The header describes the ROM mapper and size, not the PCB or flash wiring. Automatic mode supports reading and verification only."},"message":format!("Detected {name} · {}. Read/verify only; flash wiring is not identifiable from the header.",i["title"].as_str().unwrap_or("Untitled"))}),
    )
}
