//! Native, read-only GBA access through the INLretro shared GB/GBA connector.
use crate::{gba, usb::Bus, Error, Result};
use serde_json::{json, Value};
use std::time::Duration;

const IO: u8 = 2;
const IO_RESET: u8 = 0;
const GBA_INIT: u8 = 6;
const GB_POWER_3V: u8 = 10;
const GBA: u8 = 13;
const GBA_READ: u8 = 0;
const GBA_LATCH_ADDRESS: u8 = 2;
const GBA_RELEASE_BUS: u8 = 3;
const GBA_ROM_PAGE: u16 = 0x27dd;

pub struct Reader {
    bus: Bus,
    initialized: bool,
    closed: bool,
}

impl Reader {
    pub fn new(bus: Bus) -> Self {
        Self {
            bus,
            initialized: false,
            closed: false,
        }
    }

    fn latch(&mut self, byte_address: usize) -> Result<()> {
        let word_address = byte_address / 2;
        self.bus.cmd(
            GBA,
            GBA_LATCH_ADDRESS,
            word_address as u16,
            (word_address >> 16) as u8,
        )
    }

    fn release(&mut self) -> Result<()> {
        self.bus.cmd(GBA, GBA_RELEASE_BUS, 0, 0)
    }

    fn read_direct(&mut self, length: usize) -> Result<Vec<u8>> {
        let mut data = Vec::with_capacity(length);
        for _ in 0..length / 2 {
            self.bus.cancel.check()?;
            let word = self.bus.transfer(GBA, GBA_READ, 0, 0, 4, false)?;
            data.extend_from_slice(&word[2..4]);
        }
        Ok(data)
    }

    fn ensure_read(&self, address: usize, length: usize) -> Result<()> {
        if self.closed
            || !self.initialized
            || !address.is_multiple_of(2)
            || length == 0
            || !length.is_multiple_of(2)
            || address
                .checked_add(length)
                .is_none_or(|end| end > gba::MAX_SIZE)
        {
            return Err(Error::new(
                "GBA_READ_INVALID",
                "The requested INLretro GBA ROM read is invalid or the reader is not initialized.",
                "Retry the operation from Cartridge Studio. GBA ROM addresses and lengths must be even and remain within 32 MiB.",
            ));
        }
        Ok(())
    }
}

impl gba::RomReader for Reader {
    fn initialize(&mut self) -> Result<()> {
        if self.closed {
            return Err(Error::check("The INLretro GBA reader is already closed."));
        }
        self.bus.cancel.check()?;
        if self.initialized {
            self.release()?;
        }
        self.bus.cmd(IO, IO_RESET, 0, 0)?;
        self.bus.cmd(IO, GBA_INIT, 0, 0)?;
        // State the safe voltage explicitly even though GBA_INIT also defaults to 3.3 V.
        self.bus.cmd(IO, GB_POWER_3V, 0, 0)?;
        std::thread::sleep(Duration::from_millis(100));
        self.initialized = true;
        Ok(())
    }

    fn read(&mut self, address: usize, length: usize) -> Result<Vec<u8>> {
        self.ensure_read(address, length)?;
        self.bus.cancel.check()?;
        self.latch(address)?;
        // Small probes use the firmware's direct 16-bit command. This gives header and
        // mirroring checks an independent path; sustained ROM reads use double buffering.
        let result = if length >= 1024 && length.is_multiple_of(128) {
            self.bus.block(GBA_ROM_PAGE, 0, length)
        } else {
            self.read_direct(length)
        };
        let cleanup = self.release();
        result.and_then(|data| cleanup.map(|_| data))
    }

    fn identity(&self) -> Value {
        let mut value = self.bus.identity();
        value["driver"] = json!("inlretro");
        value["name"] = json!("INLretro");
        value["firmware"] = value["firmware_usb"].clone();
        value["voltage"] = json!(3.3);
        value["capabilities"] = json!({
            "slots": ["gameboy", "gba", "nes", "famicom"],
            "read": true,
            "backup": true,
            "verify": true,
            "write": false,
            "wipe": false,
            "save_ram": false
        });
        value
    }

    fn close(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        self.initialized = false;
        let mut first = None;
        for result in [
            self.release(),
            self.bus.stop(),
            self.bus.cmd(IO, IO_RESET, 0, 0),
        ] {
            if let Err(error) = result {
                if first.is_none() {
                    first = Some(error);
                }
            }
        }
        first.map_or(Ok(()), Err)
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        if let Err(error) = gba::RomReader::close(self) {
            eprintln!("INLretro GBA cleanup: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{storage::Cancel, usb::Transport};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct State {
        calls: Vec<(u8, u8, u16, u8, u16)>,
        byte_address: usize,
    }

    struct Fake(Arc<Mutex<State>>);
    impl Transport for Fake {
        fn identity(&self) -> Value {
            json!({"firmware_usb":"0203"})
        }
        fn input(
            &mut self,
            dictionary: u8,
            value: u16,
            index: u16,
            length: u16,
        ) -> Result<Vec<u8>> {
            let opcode = value as u8;
            let misc = (value >> 8) as u8;
            let mut state = self.0.lock().unwrap();
            state.calls.push((dictionary, opcode, index, misc, length));
            if dictionary == GBA && opcode == GBA_LATCH_ADDRESS {
                state.byte_address = (((misc as usize) << 16) | index as usize) * 2;
            }
            if dictionary == GBA && opcode == GBA_READ {
                let address = state.byte_address;
                state.byte_address += 2;
                return Ok(vec![0, 2, pattern(address), pattern(address + 1)]);
            }
            if dictionary == 5 && opcode == 0x61 {
                return Ok(vec![0, 1, 0xd8]);
            }
            if dictionary == 5 && opcode == 0x70 {
                let address = state.byte_address;
                state.byte_address += length as usize;
                return Ok((0..length as usize)
                    .map(|offset| pattern(address + offset))
                    .collect());
            }
            Ok(if length == 1 {
                vec![0]
            } else {
                let mut response = vec![0; length as usize];
                response[1] = (length - 2) as u8;
                response
            })
        }
        fn output(&mut self, _request: u8, _value: u16, _index: u16, _data: &[u8]) -> Result<()> {
            Ok(())
        }
    }

    fn pattern(address: usize) -> u8 {
        ((address * 37) ^ (address >> 8)) as u8
    }

    fn reader() -> (Reader, Arc<Mutex<State>>) {
        let state = Arc::new(Mutex::new(State::default()));
        let bus = Bus {
            transport: Box::new(Fake(state.clone())),
            cancel: Cancel::default(),
        };
        (Reader::new(bus), state)
    }

    #[test]
    fn initialization_uses_only_the_gba_3v3_sequence() {
        let (mut reader, state) = reader();
        gba::RomReader::initialize(&mut reader).unwrap();
        let calls = &state.lock().unwrap().calls;
        assert!(calls.starts_with(&[
            (IO, IO_RESET, 0, 0, 1),
            (IO, GBA_INIT, 0, 0, 1),
            (IO, GB_POWER_3V, 0, 0, 1),
        ]));
        assert!(!calls.iter().any(|&(d, o, _, _, _)| d == IO && o == 9));
        assert_eq!(gba::RomReader::identity(&reader)["voltage"], 3.3);
    }

    #[test]
    fn reads_use_24_bit_word_addresses_and_release_the_bus() {
        let (mut reader, state) = reader();
        gba::RomReader::initialize(&mut reader).unwrap();
        let address = 0x12_3456;
        let data = gba::RomReader::read(&mut reader, address, 12).unwrap();
        assert_eq!(
            data,
            (0..12)
                .map(|offset| pattern(address + offset))
                .collect::<Vec<_>>()
        );
        let calls = &state.lock().unwrap().calls;
        let word = address / 2;
        assert!(calls.contains(&(GBA, GBA_LATCH_ADDRESS, word as u16, (word >> 16) as u8, 1)));
        assert!(calls
            .iter()
            .any(|&(d, o, _, _, _)| d == GBA && o == GBA_RELEASE_BUS));
    }

    #[test]
    fn bulk_reads_use_the_gba_page_buffer_exactly() {
        let (mut reader, state) = reader();
        gba::RomReader::initialize(&mut reader).unwrap();
        let data = gba::RomReader::read(&mut reader, 0x100, 1024).unwrap();
        assert_eq!(
            data,
            (0..1024)
                .map(|offset| pattern(0x100 + offset))
                .collect::<Vec<_>>()
        );
        assert!(state
            .lock()
            .unwrap()
            .calls
            .iter()
            .any(|&(d, o, a, m, _)| d == 5 && o == 0x30 && a == GBA_ROM_PAGE && m < 2));
    }

    #[test]
    fn invalid_reads_do_not_touch_the_cartridge_bus_and_close_is_idempotent() {
        let (mut reader, state) = reader();
        assert!(gba::RomReader::read(&mut reader, 0, 2).is_err());
        gba::RomReader::initialize(&mut reader).unwrap();
        let before = state.lock().unwrap().calls.len();
        for (address, length) in [(1, 2), (0, 1), (gba::MAX_SIZE, 2)] {
            assert!(gba::RomReader::read(&mut reader, address, length).is_err());
        }
        assert_eq!(state.lock().unwrap().calls.len(), before);
        gba::RomReader::close(&mut reader).unwrap();
        let after = state.lock().unwrap().calls.len();
        gba::RomReader::close(&mut reader).unwrap();
        assert_eq!(state.lock().unwrap().calls.len(), after);
    }
}
