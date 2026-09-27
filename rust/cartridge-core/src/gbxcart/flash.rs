//! SST39SF040 / MBC5 / AUDIO (Ferrante 512). No generic flash command API.
use super::*;
use crate::{gb::RomReader, operations::cleanup, rom};

const UNLOCK: [(u16, u8); 2] = [(0x5555, 0xaa), (0x2aaa, 0x55)];

impl Reader {
    fn flash_hardware(&self) -> Result<()> {
        if self.identity["pcb"] != 6 || self.identity["firmware"] != "L14" {
            return Err(Error::new("GBXCART_FLASH_FIRMWARE_UNQUALIFIED",
                "This GBxCart hardware/firmware has not been qualified for Ferrante 512 writing.",
                "Use a GBxCart RW v1.4a/b/c (PCB 6) with L14 for this flash profile. Automatic ROM reading remains available. No firmware has been changed."));
        }
        Ok(())
    }
    pub(super) fn configure_audio(&mut self) -> Result<()> {
        self.flash_hardware()?;
        if self.mode != 1 || !self.flash_session {
            return Err(Error::check(
                "Select and identify the AUDIO/MBC5 flash board first.",
            ));
        }
        self.variable(1, 4, 2)?; // FLASH_WE_PIN_AUDIO; normal B2 banking still uses WR.
        for key in [5, 7, 10] {
            self.variable(1, key, 0)?;
        }
        Ok(())
    }
    fn flash_sequence(&mut self, sequence: &[(u16, u8)]) -> Result<()> {
        if self.mode != 1 || !self.flash_session {
            return Err(Error::check("Flash bus is not initialized."));
        }
        let mut cmd = vec![0xd4, 1, sequence.len() as u8];
        for &(address, value) in sequence {
            cmd.extend((address as u32).to_be_bytes());
            cmd.extend((value as u16).to_be_bytes());
        }
        self.ack(&cmd, true)
    }
    fn reset_flash(&mut self) -> Result<()> {
        // An ID query must exit software-ID mode even after cancellation.
        self.ack(&[0xd1, 0, 0, 0, 0, 0xf0], false)
    }
    pub(super) fn identify_sst(&mut self) -> Result<Value> {
        self.flash_hardware()?;
        RomReader::initialize(self)?;
        self.mapper = Some("MBC5".into());
        self.bank_write(0x3000, 0)?;
        self.bank_write(0x2000, 1)?;
        let before = self.read_range(0, 0x150)?;
        let header = rom::gb_header(&before)?;
        if header["logo_valid"] == true
            && (header["cartridge_type"] != 0x19 || header["ram_size_code"] != 0)
        {
            return Err(Error::new("ROM_INCOMPATIBLE", "The connected cartridge header does not match the Ferrante 512 / AUDIO-MBC5 profile.", "Choose Automatic for reading, or select the exact physical flash board. Nothing has been erased."));
        }
        self.flash_session = true;
        self.configure_audio()?;
        self.reset_flash()?;
        let result = (|| {
            self.flash_sequence(&[UNLOCK[0], UNLOCK[1], (0x5555, 0x90)])?;
            std::thread::sleep(Duration::from_millis(1));
            self.read_range(0, 2)
        })();
        let reset = self.reset_flash();
        let id = cleanup(result, reset)?;
        std::thread::sleep(Duration::from_millis(1));
        let after = self.read_range(0, 0x150)?;
        if before != after {
            return Err(Error::new("FLASH_ID_CHANGED_ROM", "Cartridge bytes changed after flash identification.", "Unplug USB and check the physical cartridge profile and seating before retrying. Erase/program was not started."));
        }
        if id != [0xbf, 0xb7] || id == before[..2] {
            return Err(Error::new("FLASH_NOT_IDENTIFIED", format!("Expected a responding SST39SF040 flash chip (BF B7); received {}.", hex::encode(&id)), "Check that this is the Ferrante 512 / SST39SF040 AUDIO/MBC5 board. Retail cartridges and unknown flash boards cannot be written. Nothing has been erased.").details(json!({"received":hex::encode(id)})).exit(3));
        }
        self.flash_verified = true;
        Ok(
            json!({"profile":rom::GB_PROFILE,"flash_id":"bfb7","capacity":rom::CAPACITY,"header":header,"header_unchanged_after_id":true}),
        )
    }
}

impl gb::FlashWriter for Reader {
    fn check_cancel(&self) -> Result<()> {
        self.cancel.check()
    }
    fn prepare_program(&mut self) -> Result<()> {
        if !self.flash_verified || self.mode != 1 {
            return Err(Error::check(
                "Identify the flash chip before configuring programming.",
            ));
        }
        // A7 loads six address/data entries. AMD byte programming uses three
        // unlock entries followed by the firmware's current address/data.
        let mut command = vec![0xa7, 1, 1, 2]; // AMD, unbuffered, AUDIO
        for (address, value) in [UNLOCK[0], UNLOCK[1], (0x5555, 0xa0), (0, 0), (0, 0), (0, 0)] {
            command.extend((address as u32).to_be_bytes());
            command.extend((value as u16).to_be_bytes());
        }
        self.ack(&command, true)?;
        self.variable(1, 6, 1)?; // Unlock commands must address physical bank 1.
        self.ack(&[0xb8, 0], true)?; // Firmware's default MBC5 bank switch.
        self.variable(2, 1, 0)?; // No flash write buffer.
        self.variable(2, 5, 0x80)?;
        self.variable(2, 6, 0x80)?;
        self.program_ready = true;
        Ok(())
    }
    fn erase(&mut self) -> Result<()> {
        self.cancel.check()?;
        if !self.flash_verified || self.mode != 1 {
            return Err(Error::check(
                "Erase requires a verified AUDIO/MBC5 flash chip.",
            ));
        }
        self.bank_write(0x3000, 0)?;
        self.bank_write(0x2000, 1)?;
        self.flash_sequence(&[
            UNLOCK[0],
            UNLOCK[1],
            (0x5555, 0x80),
            UNLOCK[0],
            UNLOCK[1],
            (0x5555, 0x10),
        ])?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            self.cancel.check()?;
            if self.read_range(0, 1)? == [255] {
                self.erased = true;
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(Error::new("ERASE_TIMEOUT", "The flash chip did not finish erasing.", "Retain both original backups and the source. Reconnect the reader and restore the saved source before using the cartridge.").exit(5));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn program_bank(&mut self, bank: usize, data: &[u8]) -> Result<()> {
        if self.mode != 1
            || !self.flash_verified
            || !self.program_ready
            || !self.erased
            || bank >= 32
            || data.len() != 16384
        {
            return Err(Error::check(
                "Programming requires a verified, erased chip and one valid 16 KiB bank.",
            ));
        }
        self.variable(2, 2, bank as u32)?;
        self.bank_write(0x3000, 0)?;
        self.bank_write(0x2000, bank as u8)?;
        let base = if bank == 0 { 0 } else { 0x4000 };
        for (index, chunk) in data.chunks(256).enumerate() {
            self.cancel.check()?;
            if chunk.iter().all(|&b| b == 255) {
                continue;
            }
            self.variable(4, 0, base + (index * 256) as u32)?;
            self.variable(2, 0, chunk.len() as u32)?;
            let mut command = vec![0xd3];
            command.extend(chunk);
            // Never retry an ambiguous erase/program response.
            self.ack(&command, true).map_err(|mut e| {
                e.details["bank"] = json!(bank);
                e.details["offset"] = json!(index * 256);
                e
            })?;
        }
        Ok(())
    }
}
