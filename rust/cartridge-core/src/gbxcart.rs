//! Native serial transport for the GBxCart RW extended (L) firmware protocol.
//! Protocol references and the supported envelope are documented in docs/readers.md.
use crate::{gb, gba, storage::Cancel, Error, Result};
use serde_json::{json, Value};
use std::{
    io::{self, Read, Write},
    time::{Duration, Instant},
};
mod flash;

trait Wire: Read + Write {
    fn discard_input(&self) -> io::Result<()>;
}
impl Wire for serialport::TTYPort {
    fn discard_input(&self) -> io::Result<()> {
        use serialport::SerialPort;
        self.clear(serialport::ClearBuffer::Input)
            .map_err(io::Error::from)
    }
}
pub struct Reader {
    wire: Box<dyn Wire>,
    cancel: Cancel,
    identity: Value,
    protocol: Protocol,
    power_control: bool,
    command_ack: bool,
    mapper: Option<String>,
    touched_power: bool,
    mode: u8,
    read_retries: usize,
    flash_session: bool,
    flash_verified: bool,
    program_ready: bool,
    erased: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Protocol {
    Extended,
    LegacyV13,
}
fn communication(e: impl std::fmt::Display) -> Error {
    Error::new("SERIAL_IO", "Communication with GBxCart RW failed.", "Close other cartridge software, reconnect the reader directly using a data USB cable, then retry. Retain the partial read and report.")
        .details(json!({"reason":e.to_string()})).exit(3)
}
fn incompatible(detail: Value) -> Error {
    Error::new("GBXCART_FIRMWARE_UNSUPPORTED", "This reader or firmware is outside the supported GBxCart RW configuration.", "For GBA ROM reading, use GBxCart RW v1.3 with working software voltage selection, or v1.4/v1.4a/b/c with extended firmware L12–L15. Check the port and reader information; no firmware has been changed and the cartridge has not been accessed.")
        .details(detail).exit(3)
}
#[cfg(target_os = "linux")]
fn serial_access_action() -> &'static str {
    "Close FlashGBX and other cartridge applications. Grant your account access to this serial device using your distribution's serial-device group, sign in again, reconnect, and retry."
}
#[cfg(target_os = "macos")]
fn serial_access_action() -> &'static str {
    "Close FlashGBX and other cartridge applications, reconnect the reader directly, and retry. Choose the correct /dev/cu.usbserial port if it changed."
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn serial_access_action() -> &'static str {
    "Close other cartridge applications, reconnect the reader directly, choose the correct serial port, and retry."
}
impl Reader {
    pub fn open(path: &str, cancel: Cancel) -> Result<Self> {
        cancel.check()?;
        let port = serialport::new(path, 1_000_000)
            .timeout(Duration::from_millis(100))
            .flow_control(serialport::FlowControl::None)
            .open_native()
            .map_err(|e| {
                let code = match e.kind() {
                    serialport::ErrorKind::Io(io::ErrorKind::PermissionDenied) => {
                        "SERIAL_PERMISSION_DENIED"
                    }
                    serialport::ErrorKind::Io(io::ErrorKind::NotFound)
                    | serialport::ErrorKind::NoDevice => "READER_DISCONNECTED",
                    _ => "SERIAL_OPEN_FAILED",
                };
                Error::new(
                    code,
                    format!("Cannot open the GBxCart serial port {path}."),
                    serial_access_action(),
                )
                .details(json!({"port":path,"reason":e.to_string()}))
                .exit(3)
            })?;
        // TTYPort holds an exclusive OS lock. Opening does not assert DTR or reset firmware.
        Self::connect(Box::new(port), path, cancel)
    }
    fn connect(wire: Box<dyn Wire>, path: &str, cancel: Cancel) -> Result<Self> {
        let mut r = Self {
            wire,
            cancel,
            identity: Value::Null,
            protocol: Protocol::Extended,
            power_control: false,
            command_ack: true,
            mapper: None,
            touched_power: false,
            mode: 0,
            read_retries: 0,
            flash_session: false,
            flash_verified: false,
            program_ready: false,
            erased: false,
        };
        r.wire.discard_input().map_err(communication)?;
        let pcb = r.query(0x68, 1)?[0];
        if pcb == 4 {
            r.legacy_command(b'0', false)?;
            r.wire.discard_input().map_err(communication)?;
        }
        let original = r.query(0x56, 1)?[0];
        if pcb == 4 && original > 0 {
            let cartridge_mode = r.query(b'C', 1)?[0];
            let confirmed_pcb = r.query(b'h', 1)?[0];
            if ![1, 2].contains(&cartridge_mode) || confirmed_pcb != 4 || original > 30 {
                return Err(incompatible(
                    json!({"pcb":confirmed_pcb,"cartridge_mode":cartridge_mode,"firmware_revision":original,"port":path}),
                ));
            }
            r.protocol = Protocol::LegacyV13;
            r.identity = json!({
                "driver":"gbxcart",
                "product":"GBxCart RW",
                "manufacturer":"insideGadgets",
                "path":path,
                "baud":1_000_000,
                "pcb":pcb,
                "hardware":"v1.3",
                "firmware":format!("R{original}"),
                "firmware_revision_reported":true,
                "power_control":false,
                "capabilities":{
                    "slots":["gba"],
                    "read":true,
                    "backup":true,
                    "verify":true,
                    "write":false,
                    "wipe":false,
                    "save_ram":false
                }
            });
            return Ok(r);
        }
        if ![4, 5, 6].contains(&pcb) {
            return Err(incompatible(json!({"pcb":pcb,"port":path})));
        }
        let length = r.query(0xa1, 1)?[0];
        if length != 8 {
            return Err(incompatible(json!({"info_length":length})));
        }
        let info = r.receive(8, true)?;
        let revision = u16::from_be_bytes([info[1], info[2]]);
        let supported_revision =
            (pcb == 4 && revision == 1) || ([5, 6].contains(&pcb) && (12..=15).contains(&revision));
        if info[0] != b'L' || !supported_revision || info[3] != pcb {
            return Err(incompatible(json!({"info":info,"pcb":pcb,"port":path})));
        }
        let (name, flags) = if revision >= 12 {
            let length = r.receive(1, true)?[0] as usize;
            if !(1..=64).contains(&length) {
                return Err(incompatible(json!({"name_length":length})));
            }
            (r.receive(length, true)?, r.receive(2, true)?)
        } else {
            (b"GBxCart RW\0".to_vec(), vec![0, 0])
        };
        let power_control = flags[0] & 1 != 0;
        if name.strip_suffix(&[0]).unwrap_or(&name) != b"GBxCart RW"
            || (pcb == 4 && power_control)
            || ([5, 6].contains(&pcb) && !power_control)
        {
            return Err(incompatible(json!({"name":name,"flags":flags})));
        }
        r.power_control = power_control;
        r.command_ack = revision >= 12;
        let hardware = match pcb {
            4 => "v1.3",
            5 => "v1.4",
            _ => "v1.4a/b/c",
        };
        let slots = if pcb == 4 {
            json!(["gba"])
        } else {
            json!(["gameboy", "gba"])
        };
        r.identity = json!({"driver":"gbxcart","product":"GBxCart RW","manufacturer":"insideGadgets","path":path,"baud":1_000_000,"pcb":pcb,"hardware":hardware,"firmware":format!("L{revision}"),"firmware_original":original,"firmware_timestamp":u32::from_be_bytes(info[4..8].try_into().unwrap()),"power_control":power_control,"capabilities":{"slots":slots,"read":true,"backup":true,"verify":true,"write":false,"wipe":false,"save_ram":false}});
        let writable = pcb == 6 && revision == 14;
        r.identity["capabilities"]["write"] = json!(writable);
        r.identity["capabilities"]["wipe"] = json!(writable);
        r.identity["capabilities"]["write_profiles"] = if writable {
            json!([crate::rom::GB_PROFILE])
        } else {
            json!([])
        };
        Ok(r)
    }
    fn legacy_command(&mut self, command: u8, cancellable: bool) -> Result<()> {
        self.send(&[command], cancellable)?;
        std::thread::sleep(Duration::from_millis(1));
        Ok(())
    }
    fn legacy_number(&mut self, command: u8, value: u32) -> Result<()> {
        let mut data = vec![command];
        data.extend(format!("{value:x}").bytes());
        data.push(0);
        self.send(&data, true)?;
        std::thread::sleep(Duration::from_millis(6));
        Ok(())
    }
    fn legacy_gba_read(&mut self, address: usize, length: usize) -> Result<Vec<u8>> {
        self.legacy_number(b'A', (address / 2) as u32)?;
        self.legacy_command(b'r', true)?;
        let wire_length = length.div_ceil(64) * 64;
        let result = (|| {
            let mut data = Vec::with_capacity(wire_length);
            while data.len() < wire_length {
                self.cancel.check()?;
                data.extend(self.receive(64, true)?);
                if data.len() < wire_length {
                    self.legacy_command(b'1', true)?;
                }
            }
            data.truncate(length);
            Ok(data)
        })();
        let stop = self.legacy_command(b'0', false);
        result.and_then(|data| stop.map(|_| data))
    }
    fn send(&mut self, data: &[u8], cancellable: bool) -> Result<()> {
        if cancellable {
            self.cancel.check()?;
        }
        self.wire.write_all(data).map_err(communication)?;
        // Do not use tcdrain: unplugged serial drivers may block it indefinitely.
        #[cfg(target_os = "macos")]
        std::thread::sleep(Duration::from_micros(1400));
        Ok(())
    }
    fn receive(&mut self, length: usize, cancellable: bool) -> Result<Vec<u8>> {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut data = vec![0; length];
        let mut offset = 0;
        while offset < length {
            if cancellable {
                self.cancel.check()?;
            }
            if Instant::now() >= deadline {
                return Err(Error::new("SERIAL_TIMEOUT", "GBxCart RW did not send a complete response.", "Close other cartridge applications and reconnect the reader. Check the selected serial port and use firmware at its normal 1,000,000 baud setting.").details(json!({"expected":length,"received":offset})).exit(3));
            }
            match self.wire.read(&mut data[offset..]) {
                Ok(0) => return Err(communication("Serial device closed the connection")),
                Ok(n) => offset += n,
                Err(e)
                    if [
                        io::ErrorKind::TimedOut,
                        io::ErrorKind::WouldBlock,
                        io::ErrorKind::Interrupted,
                    ]
                    .contains(&e.kind()) => {}
                Err(e) => return Err(communication(e)),
            }
        }
        Ok(data)
    }
    fn query(&mut self, command: u8, length: usize) -> Result<Vec<u8>> {
        self.send(&[command], true)?;
        self.receive(length, true)
    }
    fn ack(&mut self, data: &[u8], cancellable: bool) -> Result<()> {
        self.send(data, cancellable)?;
        let ack = self.receive(1, cancellable)?[0];
        if ack != 1 {
            return Err(Error::new("GBXCART_COMMAND_REJECTED", "GBxCart RW rejected a command or sent an unexpected reply.", "Retain the report, original backup and source. Reconnect the reader before retrying. If programming had started, restore the saved source; incomplete writes are never reported as successful.").details(json!({"command":data[0],"reply":ack})).exit(3));
        }
        Ok(())
    }
    fn control(&mut self, data: &[u8], cancellable: bool) -> Result<()> {
        if self.command_ack {
            self.ack(data, cancellable)
        } else {
            self.send(data, cancellable)
        }
    }
    fn variable(&mut self, width: u8, key: u32, value: u32) -> Result<()> {
        let mut cmd = vec![0xa6, width];
        cmd.extend(key.to_be_bytes());
        cmd.extend(value.to_be_bytes());
        self.control(&cmd, true)
    }
    fn read_range(&mut self, address: u16, length: usize) -> Result<Vec<u8>> {
        if self.mode != 1 {
            return Err(Error::check("Game Boy mode is not initialized."));
        }
        if address as usize + length > 0x8000 || length == 0 {
            return Err(Error::check("Invalid Game Boy ROM read range."));
        }
        let mut out = Vec::with_capacity(length);
        while out.len() < length {
            let size = (length - out.len()).min(4096);
            self.variable(1, 1, 1)?; // DMG_ACCESS_MODE = ROM read
            self.variable(2, 0, size as u32)?;
            self.variable(4, 0, address as u32 + out.len() as u32)?;
            out.extend(self.query(0xb1, size)?);
        }
        Ok(out)
    }
    fn bank_write(&mut self, address: u16, value: u8) -> Result<()> {
        if self.mode != 1
            || self.mapper.is_none()
            || ![0x2000, 0x2100, 0x3000, 0x4000, 0x6000].contains(&address)
        {
            return Err(Error::check("Disallowed Game Boy bank register write."));
        }
        let mut cmd = vec![0xb2];
        cmd.extend((address as u32).to_be_bytes());
        cmd.push(value);
        self.ack(&cmd, true)
    }
}
impl gb::RomReader for Reader {
    fn initialize(&mut self) -> Result<()> {
        if self.identity["pcb"] == 4 {
            return Err(Error::new(
                "GBXCART_V13_GB_UNSUPPORTED",
                "This release supports GBxCart RW v1.3 for read-only GBA ROM access only.",
                "Choose Game Boy Advance with the Automatic profile. GB/GBC support for v1.3 needs separate physical qualification.",
            ));
        }
        self.mode = 0;
        self.flash_verified = false;
        self.program_ready = false;
        self.erased = false;
        self.cancel.check()?;
        self.touched_power = true; // Cleanup must run even if configuration fails halfway.
        if self.power_control {
            self.ack(&[0xf3], true)?;
            std::thread::sleep(Duration::from_millis(200));
        }
        self.ack(&[0xa3], true)?;
        self.ack(&[0xa5], true)?;
        self.variable(1, 0x0b, 1)?; // GBxCart standard A15 ROM read method.
        self.variable(1, 0, 1)?;
        self.variable(1, 8, 0)?;
        self.variable(1, 9, 0)?;
        self.variable(1, 0x0f, 1)?;
        self.variable(4, 1, 5000)?;
        if self.power_control {
            self.ack(&[0xf2], true)?;
            std::thread::sleep(Duration::from_millis(100));
        }
        self.ack(&[0xb4], true)?;
        self.mode = 1;
        Ok(())
    }
    fn header_bytes(&mut self) -> Result<Vec<u8>> {
        self.read_range(0, 0x150)
    }
    fn set_mapper(&mut self, mapper: String) {
        self.mapper = Some(mapper);
    }
    fn read_bank(&mut self, bank: usize) -> Result<Vec<u8>> {
        self.cancel.check()?;
        let mapper = self.mapper.clone();
        let address = gb::select_bank(mapper.as_deref(), bank, &mut |a, v| self.bank_write(a, v))?;
        self.read_range(address, 16384)
    }
    fn identity(&self) -> Value {
        self.identity.clone()
    }
    fn close(&mut self) -> Result<()> {
        self.mode = 0;
        self.flash_verified = false;
        self.program_ready = false;
        self.erased = false;
        if !self.touched_power {
            return Ok(());
        }
        self.touched_power = false;
        if self.protocol == Protocol::LegacyV13 {
            let stopped = self.legacy_command(b'0', false);
            let _ = self.wire.discard_input();
            return stopped;
        }
        // Interrupted reads can leave payload bytes queued. Power-off is unconditional;
        // discard stale input before consuming its acknowledgement, ignoring cancellation.
        let _ = self.wire.discard_input();
        let command = if self.power_control { 0xf3 } else { 0xa8 };
        self.control(&[command], false).map_err(|e| Error::new("READER_CLEANUP_FAILED", if self.power_control { "Cartridge power-off could not be confirmed." } else { "Cartridge bus release could not be confirmed." }, "Unplug USB before removing or changing the cartridge. Keep the completed reads and diagnostic report.").details(json!({"cause":e})))
    }
    fn identify_flash(&mut self) -> Result<Value> {
        self.identify_sst()
    }
    fn enable_audio(&mut self) -> Result<()> {
        self.configure_audio()
    }
}
impl gba::RomReader for Reader {
    fn initialize(&mut self) -> Result<()> {
        self.mode = 0;
        self.mapper = None;
        self.flash_session = false;
        self.flash_verified = false;
        self.program_ready = false;
        self.erased = false;
        self.cancel.check()?;
        self.touched_power = true;
        if self.protocol == Protocol::LegacyV13 {
            // v1.3 powers the cartridge from USB and switches voltage in firmware.
            // Stop any old stream before selecting GBA's required 3.3 V.
            self.legacy_command(b'0', false)?;
            self.wire.discard_input().map_err(communication)?;
            self.legacy_command(b'3', true)?;
            std::thread::sleep(Duration::from_millis(500));
            self.legacy_command(b'g', true)?;
            self.mode = 2;
            return Ok(());
        }
        if self.power_control {
            self.ack(&[0xf3], true)?;
            std::thread::sleep(Duration::from_millis(200));
        }
        self.control(&[0xa2], true)?; // GBA bus, configured while cartridge power is off.
        self.control(&[0xa4], true)?; // 3.3 V. Never use the Game Boy 5 V command here.
        self.variable(1, 0x0c, 0)?;
        self.variable(1, 0, 2)?;
        self.variable(1, 0x10, 0)?;
        self.variable(4, 0, 0)?;
        self.variable(1, 0x0f, 1)?;
        self.variable(4, 1, 5000)?;
        if self.power_control {
            self.ack(&[0xf2], true)?;
            std::thread::sleep(Duration::from_millis(100));
        }
        self.control(&[0xc9], true)?;
        self.mode = 2;
        Ok(())
    }
    fn read(&mut self, address: usize, length: usize) -> Result<Vec<u8>> {
        if self.mode != 2
            || length == 0
            || address & 1 != 0
            || length & 1 != 0
            || address
                .checked_add(length)
                .is_none_or(|end| end > gba::MAX_SIZE)
        {
            return Err(Error::check("Invalid or uninitialized GBA ROM read. Addresses and lengths must be even and within 32 MiB."));
        }
        if self.protocol == Protocol::LegacyV13 {
            return self.legacy_gba_read(address, length);
        }
        let mut out = Vec::with_capacity(length);
        self.variable(4, 0, (address / 2) as u32)?;
        let mut transfer_size = 0;
        while out.len() < length {
            let size = (length - out.len()).min(1024);
            if transfer_size != size {
                self.variable(2, 0, size as u32)?;
                transfer_size = size;
            }
            // C1 advances the firmware's word address by the bytes returned / 2.
            let mut attempt = 0;
            loop {
                // Leave a short gap between sustained GBA bursts on the serial bridge.
                std::thread::sleep(Duration::from_millis(1));
                match self.query(0xc1, size) {
                    Ok(bytes) => {
                        out.extend(bytes);
                        break;
                    }
                    Err(e)
                        if e.code == "SERIAL_TIMEOUT" && attempt < 2 && self.read_retries < 16 =>
                    {
                        // Only ROM reads are repeatable here. Discard the incomplete reply
                        // and re-address that exact block; never fill or invent missing bytes.
                        self.cancel.check()?;
                        self.wire.discard_input().map_err(communication)?;
                        self.variable(4, 0, ((address + out.len()) / 2) as u32)?;
                        self.variable(2, 0, size as u32)?;
                        attempt += 1;
                        self.read_retries += 1;
                        self.identity["rom_read_retries"] = json!(self.read_retries);
                    }
                    Err(mut e) => {
                        e.details["byte_address"] = json!(address + out.len());
                        e.details["rom_read_retries"] = json!(self.read_retries);
                        return Err(e);
                    }
                }
            }
        }
        Ok(out)
    }
    fn identity(&self) -> Value {
        self.identity.clone()
    }
    fn close(&mut self) -> Result<()> {
        gb::RomReader::close(self)
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        let _ = gb::RomReader::close(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        gb::RomReader,
        operations::{self, Journal},
        rom,
    };
    use std::{cell::RefCell, collections::VecDeque, rc::Rc, sync::atomic::Ordering};
    struct Firmware {
        queued: VecDeque<u8>,
        sent: Vec<Vec<u8>>,
        rom: Vec<u8>,
        address: usize,
        bank: usize,
        size: usize,
        power: bool,
        gba: bool,
        voltage: u8,
        power_ons: usize,
        bad_second: bool,
        pcb: u8,
        firmware: u8,
        extended_firmware: u8,
        ack: u8,
        short: bool,
        cancel_on_read: Option<Cancel>,
        cancel_on_program: Option<Cancel>,
        drop_reads: usize,
        pending_timeout: bool,
        flash_id_mode: bool,
        flash_pin: usize,
        flash_target_bank: usize,
        flash_configured: bool,
        flash_bank1: bool,
        erases: usize,
        programs: usize,
        fault: &'static str,
    }
    impl Default for Firmware {
        fn default() -> Self {
            let mut rom = vec![0; 32768];
            for (i, b) in rom.iter_mut().enumerate() {
                *b = (i.wrapping_mul(17) >> 4) as u8;
            }
            rom[0x100..0x150].fill(0);
            rom[0x104..0x134].copy_from_slice(&rom::LOGO);
            rom[0x134..0x13d].copy_from_slice(b"CART TEST");
            rom[0x147] = 0x19;
            rom[0x14d] = rom[0x134..0x14d]
                .iter()
                .fold(0u8, |a, b| a.wrapping_sub(*b).wrapping_sub(1));
            let sum = rom.iter().map(|b| *b as u32).sum::<u32>() as u16;
            rom[0x14e..0x150].copy_from_slice(&sum.to_be_bytes());
            Self {
                queued: VecDeque::new(),
                sent: vec![],
                rom,
                address: 0,
                bank: 0,
                size: 0,
                power: false,
                gba: false,
                voltage: 0,
                power_ons: 0,
                bad_second: false,
                pcb: 6,
                firmware: 14,
                extended_firmware: 14,
                ack: 1,
                short: false,
                cancel_on_read: None,
                cancel_on_program: None,
                drop_reads: 0,
                pending_timeout: false,
                flash_id_mode: false,
                flash_pin: 0,
                flash_target_bank: 0,
                flash_configured: false,
                flash_bank1: false,
                erases: 0,
                programs: 0,
                fault: "",
            }
        }
    }
    struct Fake(Rc<RefCell<Firmware>>);
    impl Read for Fake {
        fn read(&mut self, data: &mut [u8]) -> io::Result<usize> {
            let mut f = self.0.borrow_mut();
            if f.short {
                return Ok(0);
            }
            // Real serial reads may split even a small firmware message.
            let n = data.len().min(7).min(f.queued.len());
            if n == 0 && f.pending_timeout {
                std::thread::sleep(Duration::from_millis(25));
                return Err(io::ErrorKind::TimedOut.into());
            }
            assert!(n > 0, "unexpected read with no response queued");
            for b in &mut data[..n] {
                *b = f.queued.pop_front().unwrap();
            }
            Ok(n)
        }
    }
    impl Write for Fake {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            let mut f = self.0.borrow_mut();
            f.sent.push(data.to_vec());
            let mut response = vec![f.ack];
            match data {
                [0x68] => response = vec![f.pcb],
                [0x56] => response = vec![if f.pcb == 4 { f.firmware } else { 42 }],
                [0xa1] => {
                    response = vec![
                        8,
                        b'L',
                        0,
                        f.extended_firmware,
                        f.pcb,
                        0x68,
                        0x30,
                        0x3d,
                        0x4c,
                    ];
                    if f.extended_firmware >= 12 {
                        response.push(11);
                        response.extend(b"GBxCart RW\0");
                        response.extend([u8::from(f.pcb != 4), 0]);
                    }
                }
                [0xa8] => f.power = false,
                [0xf3] => {
                    f.power = false;
                    f.flash_id_mode = false;
                }
                [0xf2] => {
                    assert_eq!(f.voltage, if f.gba { 33 } else { 50 });
                    f.power = true;
                    f.bank = 0;
                    f.power_ons += 1;
                }
                [0xa2] => {
                    assert!(!f.power);
                    f.gba = true;
                }
                [0xa3] => {
                    assert!(!f.power);
                    f.gba = false;
                }
                [0xa4] => {
                    assert!(!f.power && f.gba);
                    f.voltage = 33;
                }
                [0xa5] => {
                    assert!(!f.power && !f.gba);
                    f.voltage = 50;
                }
                [0xc9] => assert!((f.power || f.pcb == 4) && f.gba),
                [0xb4] => assert!(f.power && !f.gba),
                [0xa6, width, rest @ ..] => {
                    assert_eq!(rest.len(), 8);
                    let key = u32::from_be_bytes(rest[..4].try_into().unwrap());
                    let v = u32::from_be_bytes(rest[4..].try_into().unwrap()) as usize;
                    match (*width, key) {
                        (4, 0) => f.address = v,
                        (2, 0) => {
                            assert!(v <= 4096);
                            f.size = v;
                        }
                        (1, 0) => assert_eq!(v, if f.gba { 2 } else { 1 }),
                        (1, 1) | (1, 0x0b) | (1, 0x0f) => assert_eq!(v, 1),
                        (1, 0x0c) | (1, 0x10) | (1, 8) | (1, 9) => assert_eq!(v, 0),
                        (4, 1) => assert_eq!(v, 5000),
                        (1, 4) => {
                            assert_eq!(v, 2);
                            f.flash_pin = v;
                        }
                        (1, 5 | 7 | 10) => assert_eq!(v, 0),
                        (1, 6) => {
                            assert_eq!(v, 1);
                            f.flash_bank1 = true;
                        }
                        (2, 1) => assert_eq!(v, 0),
                        (2, 2) => {
                            assert!(v < 32);
                            f.flash_target_bank = v;
                        }
                        (2, 5 | 6) => assert_eq!(v, 0x80),
                        _ => panic!("unexpected variable {width}/{key}"),
                    }
                }
                [0xc1] => {
                    assert!((f.power || f.pcb == 4) && f.gba && f.voltage == 33);
                    response = (f.address * 2..f.address * 2 + f.size)
                        .map(|n| f.rom[n % f.rom.len()])
                        .collect();
                    f.address += f.size / 2;
                    if f.drop_reads > 0 {
                        f.drop_reads -= 1;
                        response.pop();
                        f.pending_timeout = true;
                    }
                    if f.bad_second && f.power_ons >= 3 {
                        response[0] ^= 1;
                    }
                    if let Some(c) = &f.cancel_on_read {
                        c.0.store(true, Ordering::Relaxed);
                    }
                }
                [0xb2, 0, 0, high, low, value] => {
                    assert!(!f.gba);
                    assert_eq!(*low, 0);
                    match high {
                        0x30 => f.bank = (f.bank & 255) | ((*value as usize) << 8),
                        0x20 => f.bank = (f.bank & 256) | *value as usize,
                        _ => panic!("unexpected bank register"),
                    }
                }
                [0xb1] => {
                    assert!(f.power && !f.gba);
                    let offset = if f.address < 0x4000 {
                        f.address
                    } else {
                        f.bank * 0x4000 + f.address - 0x4000
                    };
                    response = f.rom[offset..offset + f.size].to_vec();
                    if f.flash_id_mode && f.fault != "id" {
                        assert_eq!((offset, f.size), (0, 2));
                        response = vec![0xbf, 0xb7];
                    }
                    if f.size == 4096
                        && ((f.fault == "backup" && f.power_ons == 2)
                            || (f.fault == "final" && f.power_ons == 5))
                    {
                        response[0] ^= 1;
                    }
                    if f.bad_second && f.power_ons >= 3 {
                        response[0] ^= 1;
                    }
                    if let Some(c) = &f.cancel_on_read {
                        c.0.store(true, Ordering::Relaxed);
                    }
                }
                [0xd1, 0, 0, 0, 0, 0xf0] => {
                    assert!(f.power && !f.gba && f.flash_pin == 2);
                    f.flash_id_mode = false;
                }
                [0xd4, 1, count, commands @ ..] => {
                    assert!(f.power && !f.gba && f.flash_pin == 2);
                    assert_eq!(f.bank, 1, "unlock must use physical bank 1");
                    assert_eq!(commands.len(), *count as usize * 6);
                    let pairs: Vec<_> = commands
                        .as_chunks::<6>()
                        .0
                        .iter()
                        .map(|c| {
                            (
                                u32::from_be_bytes(c[..4].try_into().unwrap()),
                                u16::from_be_bytes(c[4..].try_into().unwrap()),
                            )
                        })
                        .collect();
                    match pairs.as_slice() {
                        [(0x5555, 0xaa), (0x2aaa, 0x55), (0x5555, 0x90)] => f.flash_id_mode = true,
                        [(0x5555, 0xaa), (0x2aaa, 0x55), (0x5555, 0x80), (0x5555, 0xaa), (0x2aaa, 0x55), (0x5555, 0x10)] =>
                        {
                            assert!(f.power_ons >= 3, "erase before independent backups");
                            f.erases += 1;
                            f.rom.fill(255);
                            if f.fault == "blank" {
                                f.rom[500000] = 0;
                            }
                            if f.fault == "erase-ack" {
                                response = vec![0];
                            }
                        }
                        _ => panic!("unexpected flash sequence {pairs:?}"),
                    }
                }
                [0xa7, 1, 1, 2, commands @ ..] => {
                    let pairs: Vec<_> = commands
                        .as_chunks::<6>()
                        .0
                        .iter()
                        .map(|c| {
                            (
                                u32::from_be_bytes(c[..4].try_into().unwrap()),
                                u16::from_be_bytes(c[4..].try_into().unwrap()),
                            )
                        })
                        .collect();
                    assert_eq!(
                        pairs,
                        [
                            (0x5555, 0xaa),
                            (0x2aaa, 0x55),
                            (0x5555, 0xa0),
                            (0, 0),
                            (0, 0),
                            (0, 0)
                        ]
                    );
                    f.flash_configured = true;
                }
                [0xb8, 0] => assert!(f.flash_configured && f.flash_bank1),
                [0xd3, payload @ ..] => {
                    assert!(
                        f.flash_configured && f.flash_bank1 && f.flash_pin == 2 && f.erases == 1
                    );
                    assert_eq!(payload.len(), f.size);
                    assert!(payload.len() <= 256);
                    let bank = f.flash_target_bank;
                    let address = f.address;
                    assert!(address + payload.len() <= if bank == 0 { 0x4000 } else { 0x8000 });
                    let offset = bank * 16384 + (address & 0x3fff);
                    f.programs += 1;
                    if f.fault != "program" {
                        for (n, b) in payload.iter().enumerate() {
                            f.rom[offset + n] &= *b;
                        }
                    }
                    if f.fault == "program-ack" {
                        response = vec![0];
                    }
                    if let Some(c) = &f.cancel_on_program {
                        c.0.store(true, Ordering::Relaxed);
                    }
                    if f.fault == "program-disconnect" {
                        return Err(io::ErrorKind::BrokenPipe.into());
                    }
                    if f.fault == "program-timeout" {
                        response.clear();
                        f.pending_timeout = true;
                    }
                }
                [b'3'] if f.pcb == 4 => {
                    response.clear();
                    f.voltage = 33;
                    f.power = true;
                }
                [b'g'] if f.pcb == 4 => {
                    response.clear();
                    assert_eq!(f.voltage, 33);
                    f.gba = true;
                }
                [b'C'] if f.pcb == 4 => response = vec![if f.gba { 2 } else { 1 }],
                [b'0'] if f.pcb == 4 => response.clear(),
                [b'r'] if f.pcb == 4 => {
                    response = (f.address * 2..f.address * 2 + 64)
                        .map(|n| f.rom[n % f.rom.len()])
                        .collect();
                    f.address += 32;
                }
                [b'1'] if f.pcb == 4 => {
                    response = (f.address * 2..f.address * 2 + 64)
                        .map(|n| f.rom[n % f.rom.len()])
                        .collect();
                    f.address += 32;
                }
                data if f.pcb == 4 && data.first() == Some(&b'A') && data.last() == Some(&0) => {
                    response.clear();
                    let value = std::str::from_utf8(&data[1..data.len() - 1]).unwrap();
                    f.address = usize::from_str_radix(value, 16).unwrap();
                }
                _ => panic!("Forbidden or unknown command: {data:02x?}"),
            }
            if f.pcb == 4
                && f.firmware == 0
                && f.extended_firmware < 12
                && [0xa2, 0xa4, 0xa6, 0xa8, 0xc9].contains(&data[0])
            {
                response.clear();
            }
            f.queued.extend(response);
            Ok(data.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Wire for Fake {
        fn discard_input(&self) -> io::Result<()> {
            self.0.borrow_mut().queued.clear();
            self.0.borrow_mut().pending_timeout = false;
            Ok(())
        }
    }
    fn flash_transaction(
        action: &str,
        fault: &'static str,
    ) -> (Result<Value>, Rc<RefCell<Firmware>>, tempfile::TempDir) {
        let f = Rc::new(RefCell::new(Firmware::default()));
        let mut source = f.borrow().rom.clone();
        if fault == "full" {
            source.resize(rom::CAPACITY, 255);
            for bank in 2..32 {
                source[bank * 16384..(bank + 1) * 16384].fill(bank as u8);
            }
            source[0x148] = 4;
            source[0x14d] = source[0x134..0x14d]
                .iter()
                .fold(0u8, |a, b| a.wrapping_sub(*b).wrapping_sub(1));
            source[0x14e..0x150].fill(0);
            let sum = source.iter().map(|&b| b as u32).sum::<u32>() as u16;
            source[0x14e..0x150].copy_from_slice(&sum.to_be_bytes());
        }
        f.borrow_mut().rom.resize(rom::CAPACITY, 0x42);
        f.borrow_mut().fault = fault;
        let cancel = Cancel::default();
        if fault == "program-cancel" {
            f.borrow_mut().cancel_on_program = Some(cancel.clone());
        }
        let reader = Reader::connect(Box::new(Fake(f.clone())), "/dev/test", cancel).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let mut journal = Journal::new(&temp.path().join("operation"), "gameboy", action).unwrap();
        let result = operations::gb_write_with(
            Box::new(reader),
            &mut journal,
            (action == "write").then_some(source.as_slice()),
            &mut |_| {},
        );
        assert!(!f.borrow().power);
        (result, f, temp)
    }
    #[test]
    fn flash_write_and_wipe_keep_backups_and_verify_full_capacity() {
        for action in ["write", "wipe"] {
            let (result, f, temp) = flash_transaction(action, "");
            let report = result.unwrap();
            assert_eq!(report["status"], "complete");
            assert_eq!(report["identical_final_reads"], true);
            assert_eq!(report["blank_verified_bytes"], rom::CAPACITY);
            let original = std::fs::read(temp.path().join("operation/before.read1.bin")).unwrap();
            assert_eq!(original.len(), rom::CAPACITY);
            assert_eq!(
                original,
                std::fs::read(temp.path().join("operation/before.read2.bin")).unwrap()
            );
            assert_eq!(f.borrow().erases, 1);
            if action == "write" {
                assert!(f.borrow().programs > 0);
                assert_eq!(f.borrow().rom[..32768], Firmware::default().rom);
                assert!(f.borrow().rom[32768..].iter().all(|&b| b == 255));
            } else {
                assert_eq!(f.borrow().programs, 0);
                assert!(f.borrow().rom.iter().all(|&b| b == 255));
            }
        }
    }
    #[test]
    fn flash_failures_never_report_success_or_retry_programming() {
        for (fault, code, erases) in [
            ("id", "FLASH_NOT_IDENTIFIED", 0),
            ("backup", "BACKUP_MISMATCH", 0),
            ("blank", "BLANK_CHECK_FAILED", 1),
            ("program", "BANK_VERIFY_FAILED", 1),
            ("final", "FINAL_VERIFY_FAILED", 1),
            ("erase-ack", "GBXCART_COMMAND_REJECTED", 1),
            ("program-ack", "GBXCART_COMMAND_REJECTED", 1),
            ("program-timeout", "SERIAL_TIMEOUT", 1),
            ("program-disconnect", "SERIAL_IO", 1),
            ("program-cancel", "INTERRUPTED", 1),
        ] {
            let (result, f, temp) = flash_transaction("write", fault);
            assert_eq!(result.unwrap_err().code, code, "{fault}");
            assert_eq!(f.borrow().erases, erases);
            if ["id", "backup", "blank", "erase-ack"].contains(&fault) {
                assert_eq!(f.borrow().programs, 0);
            }
            if [
                "program-ack",
                "program-timeout",
                "program-disconnect",
                "program-cancel",
            ]
            .contains(&fault)
            {
                assert_eq!(f.borrow().programs, 1);
            }
            let report: Value = serde_json::from_slice(
                &std::fs::read(temp.path().join("operation/report.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(
                report["status"],
                if fault == "program-cancel" {
                    "interrupted"
                } else {
                    "failed"
                }
            );
            assert!(!temp.path().join("operation/readback.gb").exists());
        }
    }
    #[test]
    fn flash_programs_all_32_banks_without_aliasing() {
        let (result, f, temp) = flash_transaction("write", "full");
        let report = result.unwrap();
        assert_eq!(report["banks_verified"], 32);
        let source = std::fs::read(temp.path().join("operation/source.gb")).unwrap();
        assert_eq!(source, f.borrow().rom);
        assert_eq!(
            source,
            std::fs::read(temp.path().join("operation/after.read2.bin")).unwrap()
        );
    }
    #[test]
    fn flash_requires_identification_and_qualified_firmware() {
        use crate::gb::FlashWriter;
        let f = Rc::new(RefCell::new(Firmware::default()));
        let mut r =
            Reader::connect(Box::new(Fake(f.clone())), "/dev/test", Cancel::default()).unwrap();
        assert!(r.erase().is_err());
        assert!(r.prepare_program().is_err());
        assert!(r.program_bank(0, &[0; 16384]).is_err());
        r.identity["firmware"] = json!("L15");
        assert_eq!(
            r.identify_flash().unwrap_err().code,
            "GBXCART_FLASH_FIRMWARE_UNQUALIFIED"
        );
        assert_eq!(f.borrow().power_ons, 0);
    }
    fn connect(f: &Rc<RefCell<Firmware>>, cancel: Cancel) -> Result<Reader> {
        Reader::connect(Box::new(Fake(f.clone())), "fixture", cancel)
    }
    #[test]
    fn identity_is_validated_before_cartridge_power_or_access() {
        for revision in [0, 11, 16, 255] {
            let f = Rc::new(RefCell::new(Firmware {
                extended_firmware: revision,
                ..Default::default()
            }));
            assert_eq!(
                connect(&f, Cancel::default()).err().unwrap().code,
                "GBXCART_FIRMWARE_UNSUPPORTED"
            );
            assert_eq!(f.borrow().sent, vec![vec![0x68], vec![0x56], vec![0xa1]]);
        }
        let f = Rc::new(RefCell::new(Firmware::default()));
        let r = connect(&f, Cancel::default()).unwrap();
        assert_eq!(r.identity()["firmware"], "L14");
        drop(r);
        assert_eq!(f.borrow().sent.len(), 3);
        assert!(!f.borrow().power);
    }
    #[test]
    fn two_pass_transaction_uses_rom_reads_and_bank_registers_only() {
        for corrupt in [false, true] {
            let f = Rc::new(RefCell::new(Firmware {
                bad_second: corrupt,
                ..Default::default()
            }));
            let r = connect(&f, Cancel::default()).unwrap();
            let root = tempfile::tempdir().unwrap();
            let mut j = Journal::new(&root.path().join("read"), "gameboy", "read").unwrap();
            let result = operations::gb_read_with(
                Box::new(r),
                &mut j,
                "read",
                false,
                None,
                2,
                true,
                &mut |_| {},
            );
            assert!(!f.borrow().power);
            assert_eq!(
                std::fs::read(j.directory.join("read1.bin")).unwrap(),
                f.borrow().rom
            );
            if corrupt {
                assert_eq!(result.unwrap_err().code, "BACKUP_MISMATCH");
            } else {
                let result = result.unwrap();
                assert_eq!(result["identical_reads"], true);
                assert_eq!(result["cartridge"]["global_checksum_valid"], true);
            }
        }
    }
    #[test]
    fn cancellation_short_reads_and_rejected_ack_still_power_off() {
        for mode in 0..3 {
            let cancel = Cancel::default();
            let f = Rc::new(RefCell::new(Firmware::default()));
            let mut r = connect(&f, cancel.clone()).unwrap();
            r.initialize().unwrap();
            match mode {
                0 => f.borrow_mut().cancel_on_read = Some(cancel),
                1 => f.borrow_mut().short = true,
                _ => f.borrow_mut().ack = 2,
            }
            let err = r.header_bytes().unwrap_err();
            assert_eq!(
                err.code,
                ["INTERRUPTED", "SERIAL_IO", "GBXCART_COMMAND_REJECTED"][mode]
            );
            f.borrow_mut().short = false;
            f.borrow_mut().ack = 1;
            r.close().unwrap();
            assert!(!f.borrow().power);
            assert_eq!(f.borrow().sent.last().unwrap(), &[0xf3]);
        }
    }
    #[test]
    fn failed_probe_retains_both_header_and_power_off_diagnostics() {
        let f = Rc::new(RefCell::new(Firmware::default()));
        f.borrow_mut().rom[0x104] = 0;
        let mut r = connect(&f, Cancel::default()).unwrap();
        let result = gb::detect(&mut r);
        f.borrow_mut().ack = 2;
        let error = operations::cleanup(result, r.close()).unwrap_err();
        assert_eq!(error.code, "GB_HEADER_UNREADABLE");
        assert_eq!(
            error.details["cleanup_error"]["error"],
            "READER_CLEANUP_FAILED"
        );
    }
    #[test]
    fn high_mbc5_bank_bit_and_register_allowlist() {
        let f = Rc::new(RefCell::new(Firmware::default()));
        let mut r = connect(&f, Cancel::default()).unwrap();
        r.initialize().unwrap();
        let start = f.borrow().sent.len();
        r.set_mapper("MBC5".into());
        gb::select_bank(Some("MBC5"), 256, &mut |a, v| r.bank_write(a, v)).unwrap();
        assert_eq!(
            &f.borrow().sent[start..],
            &[vec![0xb2, 0, 0, 0x30, 0, 1], vec![0xb2, 0, 0, 0x20, 0, 0]]
        );
        let n = f.borrow().sent.len();
        for a in [0, 0xa000, 0x5555, 0x2aaa, 0x8000] {
            assert!(r.bank_write(a, 0xaa).is_err());
        }
        assert_eq!(f.borrow().sent.len(), n);
        assert!(r.enable_audio().is_err());
    }
    #[test]
    fn gba_reads_use_word_addresses_and_only_the_3v3_rom_bus() {
        let f = Rc::new(RefCell::new(Firmware::default()));
        let mut r = connect(&f, Cancel::default()).unwrap();
        assert!(gba::RomReader::read(&mut r, 0, 2).is_err());
        gba::RomReader::initialize(&mut r).unwrap();
        let bytes = gba::RomReader::read(&mut r, 0x100, 8192).unwrap();
        assert_eq!(bytes, f.borrow().rom[0x100..0x2100]);
        let n = f.borrow().sent.len();
        for (address, length) in [
            (1, 2),
            (0, 1),
            (0, 0),
            (gba::MAX_SIZE, 2),
            (usize::MAX - 1, 4),
        ] {
            assert!(gba::RomReader::read(&mut r, address, length).is_err());
        }
        assert!(r.header_bytes().is_err());
        r.set_mapper("MBC5".into());
        assert!(r.bank_write(0x2000, 1).is_err());
        assert_eq!(f.borrow().sent.len(), n);
        gba::RomReader::close(&mut r).unwrap();
        assert!(!f.borrow().power);
        assert!(!f.borrow().sent.iter().any(|v| [
            0xa3, 0xa5, 0xb1, 0xb2, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xca, 0xd2, 0xd3, 0xd4
        ]
        .contains(&v[0])));
    }
    #[test]
    fn legacy_v13_reads_gba_at_3v3_without_extended_or_save_commands() {
        let f = Rc::new(RefCell::new(Firmware {
            pcb: 4,
            firmware: 12,
            rom: gba::fixture(),
            ..Default::default()
        }));
        let mut r = connect(&f, Cancel::default()).unwrap();
        assert_eq!(gba::RomReader::identity(&r)["hardware"], "v1.3");
        assert_eq!(gba::RomReader::identity(&r)["firmware"], "R12");
        assert_eq!(gba::RomReader::identity(&r)["power_control"], false);
        assert!(gb::RomReader::initialize(&mut r).is_err());
        gba::RomReader::initialize(&mut r).unwrap();
        assert_eq!(
            gba::RomReader::read(&mut r, 0x100, 256).unwrap(),
            f.borrow().rom[0x100..0x200]
        );
        assert_eq!(
            gba::RomReader::read(&mut r, 0x1ffe000, 12).unwrap().len(),
            12
        );
        gba::RomReader::close(&mut r).unwrap();
        let sent = &f.borrow().sent;
        assert!(sent.iter().any(|command| command == b"3"));
        assert!(!sent
            .iter()
            .any(|command| b"mwepisbanqtfl".contains(&command[0])));
        assert_eq!(f.borrow().voltage, 33);
    }
    #[test]
    fn extended_l1_v13_uses_ackless_controls_and_releases_the_bus() {
        let f = Rc::new(RefCell::new(Firmware {
            pcb: 4,
            firmware: 0,
            extended_firmware: 1,
            rom: gba::fixture(),
            ..Default::default()
        }));
        let mut r = connect(&f, Cancel::default()).unwrap();
        assert_eq!(gba::RomReader::identity(&r)["hardware"], "v1.3");
        assert_eq!(gba::RomReader::identity(&r)["firmware"], "L1");
        assert_eq!(gba::RomReader::identity(&r)["power_control"], false);
        assert!(gb::RomReader::initialize(&mut r).is_err());
        gba::RomReader::initialize(&mut r).unwrap();
        assert_eq!(
            gba::RomReader::read(&mut r, 0x100, 256).unwrap(),
            f.borrow().rom[0x100..0x200]
        );
        gba::RomReader::close(&mut r).unwrap();
        let sent = &f.borrow().sent;
        assert!(sent.iter().any(|command| command == &[0xa8]));
        assert!(!sent
            .iter()
            .any(|command| command == &[0xf2] || command == &[0xf3]));
        assert!(!sent.iter().any(|command| b"3gr".contains(&command[0])));
        assert_eq!(f.borrow().voltage, 33);
    }
    #[test]
    fn gba_transactions_retain_both_reads_and_reject_inconsistency() {
        for corrupt in [false, true] {
            let f = Rc::new(RefCell::new(Firmware {
                rom: gba::fixture(),
                bad_second: corrupt,
                ..Default::default()
            }));
            let r = connect(&f, Cancel::default()).unwrap();
            let root = tempfile::tempdir().unwrap();
            let mut j = Journal::new(&root.path().join("read"), "gba", "backup").unwrap();
            let result = gba::read_rom(Box::new(r), &mut j, "backup", None, 1, None, &mut |_| {});
            assert!(!f.borrow().power);
            assert!(j.directory.join("read1.bin").is_file());
            assert!(j.directory.join("read2.bin").is_file());
            if corrupt {
                assert_eq!(result.unwrap_err().code, "BACKUP_MISMATCH");
            } else {
                let result = result.unwrap();
                assert_eq!(result["identical_reads"], true);
                assert_eq!(result["read_passes"], 2);
                assert_eq!(result["size_method"], "sampled_mirroring_estimate");
                assert!(result["warning"].is_string());
                assert_eq!(
                    std::fs::read(j.directory.join("cartridge.gba")).unwrap(),
                    f.borrow().rom
                );
            }
        }
    }
    #[test]
    fn gba_cancellation_and_failed_reads_power_off_without_save_commands() {
        let cancel = Cancel::default();
        let f = Rc::new(RefCell::new(Firmware::default()));
        let mut r = connect(&f, cancel.clone()).unwrap();
        gba::RomReader::initialize(&mut r).unwrap();
        f.borrow_mut().cancel_on_read = Some(cancel);
        assert_eq!(
            gba::RomReader::read(&mut r, 0, 4096).unwrap_err().code,
            "INTERRUPTED"
        );
        gba::RomReader::close(&mut r).unwrap();
        assert!(!f.borrow().power);
        assert_eq!(f.borrow().sent.last().unwrap(), &[0xf3]);
    }
    #[test]
    fn gba_retries_readdress_the_same_block_and_are_bounded() {
        for drops in [1, 3] {
            let f = Rc::new(RefCell::new(Firmware {
                drop_reads: drops,
                ..Default::default()
            }));
            let mut r = connect(&f, Cancel::default()).unwrap();
            gba::RomReader::initialize(&mut r).unwrap();
            let result = gba::RomReader::read(&mut r, 0x100, 4096);
            if drops == 1 {
                assert_eq!(result.unwrap(), f.borrow().rom[0x100..0x1100]);
                assert_eq!(r.read_retries, 1);
            } else {
                let e = result.unwrap_err();
                assert_eq!(e.code, "SERIAL_TIMEOUT");
                assert_eq!(e.details["byte_address"], 0x100);
                assert_eq!(r.read_retries, 2);
            }
            gba::RomReader::close(&mut r).unwrap();
            assert!(!f.borrow().power);
        }
    }
}
