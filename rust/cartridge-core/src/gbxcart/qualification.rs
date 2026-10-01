//! Explicit, non-destructive investigation of the photographed S29GL032M board.
//! Not a production flash profile: none of these methods qualify programming.
use super::*;
use crate::operations::cleanup;

impl Reader {
    /// Caller must confirm the physical MBC5-labelled S29GL032M board.
    /// Its loaded ROM header is deliberately not used to select the mapper.
    pub fn initialize_spansion_mbc5_readonly(&mut self) -> Result<()> {
        if self.identity["pcb"] != 6 || self.identity["firmware"] != "L14" {
            return Err(Error::new(
                "GBXCART_DIAGNOSTIC_UNQUALIFIED",
                "This diagnostic requires GBxCart PCB 6 with firmware L14.",
                "Use the photographed board and the tested reader; no firmware was changed.",
            ));
        }
        self.initialize_gb_at_voltage(true)?;
        self.mapper = Some("MBC5".into());
        self.bank_write(0x3000, 0)?;
        self.bank_write(0x2000, 1)?;
        self.identity["diagnostic_voltage"] = json!(3.3);
        Ok(())
    }

    /// Read software ID and CFI only. Never send erase/program/unprotect commands.
    /// WR routing and both documented normal/swapped-D0/D1 unlock variants are
    /// investigated; observations are returned without asserting compatibility.
    pub fn inspect_spansion_mbc5_readonly(&mut self) -> Result<Value> {
        if self.mode != 1
            || self.identity["diagnostic_voltage"] != 3.3
            || self.mapper.as_deref() != Some("MBC5")
        {
            return Err(Error::check(
                "Initialize the confirmed Spansion board diagnostic first.",
            ));
        }
        self.flash_verified = false;
        self.program_ready = false;
        self.erased = false;
        self.bank_write(0x3000, 0)?;
        self.bank_write(0x2000, 1)?;
        let before = self.read_range(0, 0x400)?;
        self.variable(1, 4, 1)?; // FLASH_WE_PIN_WR
        for key in [5, 7, 10] {
            self.variable(1, key, 0)?;
        }
        let mut observations = vec![];
        for (name, unlock) in [("normal", [0xaa, 0x55]), ("swapped-d0-d1", [0xa9, 0x56])] {
            self.ack(&[0xd1, 0, 0, 0, 0, 0xf0], false)?;
            let result = (|| {
                let mut frame = vec![0xd4, 1, 3];
                for (address, value) in [(0xaaau32, unlock[0]), (0x555, unlock[1]), (0xaaa, 0x90)] {
                    frame.extend(address.to_be_bytes());
                    frame.extend((value as u16).to_be_bytes());
                }
                self.ack(&frame, true)?;
                std::thread::sleep(Duration::from_millis(10));
                self.read_range(0, 0x100)
            })();
            let reset = self.ack(&[0xd1, 0, 0, 0, 0, 0xf0], false);
            let id = cleanup(result, reset)?;
            let after = self.read_range(0, 0x400)?;
            if before != after {
                return Err(changed());
            }
            observations.push(json!({"unlock":name,"changed_to_id_mode":id != before[..0x100],"raw":hex::encode(id)}));
        }
        let result = (|| {
            self.ack(&[0xd1, 0, 0, 0x0a, 0xaa, 0x98], true)?;
            std::thread::sleep(Duration::from_millis(10));
            self.read_range(0, 0x400)
        })();
        let reset = self.ack(&[0xd1, 0, 0, 0, 0, 0xf0], false);
        let cfi = cleanup(result, reset)?;
        if self.read_range(0, 0x400)? != before {
            return Err(changed());
        }
        Ok(
            json!({"voltage":3.3,"write_pin":"WR","id_observations":observations,
            "cfi":decode_cfi(&cfi),"cfi_raw":hex::encode(cfi),
            "rom_prefix_unchanged":true,"programming_qualified":false}),
        )
    }
}
fn changed() -> Error {
    Error::new("FLASH_QUERY_RESET_FAILED", "ROM bytes did not return to their original values after the identification query.",
        "Keep the backups and diagnostic report. Unplug USB and reseat the cartridge before retrying. No erase/program commands were sent.")
}
fn decode_cfi(raw: &[u8]) -> Value {
    for stride in [1, 2] {
        for swapped in [false, true] {
            let bytes: Vec<u8> = raw
                .iter()
                .step_by(stride)
                .map(|&b| {
                    if swapped {
                        (b & !3) | ((b & 1) << 1) | ((b & 2) >> 1)
                    } else {
                        b
                    }
                })
                .collect();
            if bytes.len() < 0x31 || &bytes[0x10..0x13] != b"QRY" {
                continue;
            }
            return json!({"recognized":true,"stride":stride,"swapped_d0_d1":swapped,
                "capacity":1u32.checked_shl(bytes[0x27] as u32),
                "command_set":u16::from_le_bytes([bytes[0x13],bytes[0x14]]),
                "erase_region_count":bytes[0x2c]});
        }
    }
    json!({"recognized":false})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cfi_requires_signature_and_reports_unknown_instead_of_assuming_capacity() {
        assert_eq!(decode_cfi(&[255; 1024]), json!({"recognized":false}));
        let mut cfi = vec![0; 1024];
        cfi[0x20] = b'Q';
        cfi[0x22] = b'R';
        cfi[0x24] = b'Y';
        cfi[0x26] = 2;
        cfi[0x4e] = 22;
        assert_eq!(decode_cfi(&cfi)["capacity"], 4 * 1024 * 1024);
        for b in &mut cfi {
            *b = (*b & !3) | ((*b & 1) << 1) | ((*b & 2) >> 1);
        }
        assert_eq!(decode_cfi(&cfi)["swapped_d0_d1"], true);
        assert_eq!(decode_cfi(&cfi)["capacity"], 4 * 1024 * 1024);
    }
}
