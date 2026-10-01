//! Exact S29GL032M R4 profile, normal data wiring, WR/MBC5 at 3.3 V.
//! Physically verified with a full-capacity pattern and two power-cycled readbacks.
use super::*;
use crate::rom;
const UNLOCK: [(u16, u8); 2] = [(0xaaa, 0xaa), (0x555, 0x55)];

impl Reader {
    /// Explicit physical board selection; the ROM header cannot establish wiring.
    pub fn with_spansion_profile(mut self) -> Result<Self> {
        self.flash_hardware()?;
        self.spansion_profile = true;
        self.flash_verified = false;
        self.program_ready = false;
        self.erased = false;
        Ok(self)
    }
    pub(super) fn configure_spansion(&mut self) -> Result<()> {
        self.flash_hardware()?;
        if !self.spansion_profile || self.mode != 1 || self.identity["diagnostic_voltage"] != 3.3 {
            return Err(Error::check(
                "Initialize the confirmed 3.3 V Spansion board first.",
            ));
        }
        self.flash_session = true;
        self.variable(1, 4, 1)?; // WR, never AUDIO
        for key in [5, 7, 10] {
            self.variable(1, key, 0)?;
        }
        Ok(())
    }
    pub(super) fn identify_spansion(&mut self) -> Result<Value> {
        self.flash_verified = false;
        self.initialize_spansion_mbc5_readonly()?;
        self.configure_spansion()?;
        let header = rom::gb_header(&self.read_range(0, 0x150)?)?;
        let query = self.inspect_spansion_mbc5_readonly()?;
        validate_identity(&query)?;
        // Program bank zero through the switchable window to avoid WR accesses
        // to the MBC5 ROM-bank registers at 0x2000–0x3FFF. Prove that alias first.
        self.bank_write(0x2000, 0)?;
        let fixed = self.read_range(0, 16384)?;
        if fixed != self.read_range(0x4000, 16384)? {
            return Err(Error::new("FLASH_BANK_MAPPING_MISMATCH", "MBC5 bank zero does not map as expected.",
                "Check the physical board model. This profile cannot safely program this cartridge; nothing has been erased."));
        }
        self.bank_write(0x2000, 1)?;
        self.flash_verified = true;
        Ok(
            json!({"profile":rom::SPANSION_PROFILE,"flash_id":"017e1a00","capacity":rom::SPANSION_CAPACITY,
            "voltage":3.3,"write_pin":"WR","header":header,"header_unchanged_after_id":true,
            "cfi":query["cfi"],"bank_zero_alias_verified":true}),
        )
    }
    fn require_spansion(&self) -> Result<()> {
        self.flash_hardware()?;
        if !self.spansion_profile
            || !self.flash_verified
            || self.mode != 1
            || self.identity["diagnostic_voltage"] != 3.3
        {
            return Err(Error::check(
                "Programming requires an identified S29GL032M R4 at 3.3 V.",
            ));
        }
        Ok(())
    }
    pub(super) fn prepare_spansion(&mut self) -> Result<()> {
        self.require_spansion()?;
        let mut command = vec![0xa7, 1, 1, 1]; // AMD, unbuffered byte programming, WR
        for (address, value) in [UNLOCK[0], UNLOCK[1], (0xaaa, 0xa0), (0, 0), (0, 0), (0, 0)] {
            command.extend((address as u32).to_be_bytes());
            command.extend((value as u16).to_be_bytes());
        }
        self.ack(&command, true)?;
        // Unlock addresses are in fixed bank zero; do not switch banks between
        // unlocking and writing the selected 0x4000 window.
        self.variable(1, 6, 0)?;
        self.ack(&[0xb8, 0], true)?;
        self.variable(2, 1, 0)?;
        self.variable(2, 5, 0x80)?;
        self.variable(2, 6, 0x80)?;
        self.program_ready = true;
        Ok(())
    }
    pub(super) fn erase_spansion(&mut self) -> Result<()> {
        self.require_spansion()?;
        self.cancel.check()?;
        self.erased = false;
        self.bank_write(0x3000, 0)?;
        self.bank_write(0x2000, 1)?;
        self.flash_sequence(&[
            UNLOCK[0],
            UNLOCK[1],
            (0xaaa, 0x80),
            UNLOCK[0],
            UNLOCK[1],
            (0xaaa, 0x10),
        ])?;
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            self.cancel.check()?;
            let status = self.read_range(0, 1)?[0];
            if status == 0xff {
                self.reset_flash()?;
                self.erased = true; // Transaction must still blank-check all 4 MiB.
                return Ok(());
            }
            if status & 0x20 != 0 && self.read_range(0, 1)?[0] != 0xff {
                return Err(Error::new("FLASH_ERASE_FAILED", "The Spansion flash reported an erase failure.",
                    "Keep both backups and the saved source. Reconnect and check the board/contact condition before recovery; protected sectors are not automatically unlocked.").exit(5));
            }
            if Instant::now() >= deadline {
                return Err(Error::new("ERASE_TIMEOUT", "The Spansion flash did not finish erasing within 120 seconds.",
                    "Keep both backups and the saved source. Reconnect before recovering this cartridge; do not treat it as blank.").exit(5));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    pub(super) fn program_spansion_bank(&mut self, bank: usize, data: &[u8]) -> Result<()> {
        self.require_spansion()?;
        if !self.program_ready || !self.erased || bank >= 256 || data.len() != 16384 {
            return Err(Error::check(
                "Programming requires an erased chip and a complete 16 KiB bank within 4 MiB.",
            ));
        }
        self.variable(2, 2, bank as u32)?;
        self.bank_write(0x3000, 0)?;
        self.bank_write(0x2000, bank as u8)?;
        for (index, chunk) in data.chunks(256).enumerate() {
            self.cancel.check()?;
            if chunk.iter().all(|&b| b == 0xff) {
                continue;
            }
            self.variable(4, 0, 0x4000 + (index * 256) as u32)?;
            self.variable(2, 0, chunk.len() as u32)?;
            let mut command = vec![0xd3];
            command.extend(chunk);
            // Never retry an ambiguous programming response.
            self.ack(&command, true).map_err(|mut e| {
                e.details["bank"] = json!(bank);
                e.details["offset"] = json!(index * 256);
                e
            })?;
        }
        Ok(())
    }
}
fn validate_identity(query: &Value) -> Result<()> {
    let normal = &query["id_observations"][0];
    let id = hex::decode(normal["raw"].as_str().unwrap_or("")).unwrap_or_default();
    let cfi = hex::decode(query["cfi_raw"].as_str().unwrap_or("")).unwrap_or_default();
    let exact_id = id.len() >= 0x20
        && [id[0], id[2], id[0x1c], id[0x1e]] == [1, 0x7e, 0x1a, 0]
        && id[..0x20].as_chunks::<2>().0.iter().all(|p| p[0] == p[1]);
    // R4 bottom-boot geometry: eight 8 KiB sectors, then sixty-three 64 KiB.
    let geometry = cfi.len() > 0x69
        && [0x2d, 0x2e, 0x2f, 0x30, 0x31, 0x32, 0x33, 0x34].map(|i| cfi[i * 2])
            == [7, 0, 0x20, 0, 0x3e, 0, 0, 1];
    if !exact_id
        || !geometry
        || normal["changed_to_id_mode"] != true
        || normal["unlock"] != "normal"
        || query["rom_prefix_unchanged"] != true
        || query["cfi"]["capacity"] != rom::SPANSION_CAPACITY
        || query["cfi"]["stride"] != 2
        || query["cfi"]["swapped_d0_d1"] != false
        || query["cfi"]["command_set"] != 2
        || query["cfi"]["erase_region_count"] != 2
    {
        return Err(Error::new("FLASH_NOT_IDENTIFIED", "The cartridge does not match the qualified S29GL032M R4 WR/MBC5 profile.",
            "Select this profile only for the confirmed 4 MiB, normal-data-wiring board. Check seating with USB unplugged. Nothing has been erased.")
            .details(json!({"detection":query})).exit(3));
    }
    Ok(())
}
