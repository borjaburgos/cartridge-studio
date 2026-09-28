//! Native Epilogue GB Operator ROM transport.
//!
//! The reader uses 64-byte frames on its CDC data interface. The provisional
//! ROM writer failed physical qualification and remains gated off outside
//! injected tests. Save-memory commands are not implemented.
use crate::{gb, gba, storage::Cancel, Error, Result};
use nusb::{
    transfer::{Buffer, Bulk, In, Out, TransferError},
    Endpoint, Interface, MaybeFuture,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const VID: u16 = 0x16d0;
const PID: u16 = 0x123d;
const FRAME: usize = 64;
const PAYLOAD: usize = 60;
const ACK_INTERVAL: usize = 320;
// The real 9.5.0/Ferrante attempt failed erase/program qualification.
// Only injected tests may exercise the provisional transport until requalified.
const LEGACY_WRITE_QUALIFIED: bool = cfg!(test);

fn io_error(e: impl std::fmt::Display) -> Error {
    Error::new(
        "OPERATOR_USB_IO",
        "Communication with the GB Operator failed.",
        "Close Playback and other cartridge software, reconnect the GB Operator directly with a data USB cable, then retry.",
    )
    .details(json!({"reason":e.to_string()}))
    .exit(3)
}

fn transfer_error(e: TransferError) -> Error {
    let (code, message, action) = match e {
        TransferError::Disconnected => (
            "READER_DISCONNECTED",
            "The GB Operator disconnected.",
            "Reconnect it directly with a data USB cable, then retry.",
        ),
        TransferError::Cancelled => (
            "OPERATOR_USB_TIMEOUT",
            "The GB Operator did not respond in time.",
            "Close Playback, reconnect the reader, and retry. Clean and reseat the cartridge with USB unplugged if this repeats.",
        ),
        TransferError::Stall => (
            "OPERATOR_COMMAND_REJECTED",
            "The GB Operator rejected a command.",
            "Reconnect the reader and retry. Keep the diagnostic report when requesting support for this firmware or cartridge.",
        ),
        _ => (
            "OPERATOR_USB_IO",
            "Communication with the GB Operator failed.",
            "Reconnect the reader directly with a data USB cable, then retry.",
        ),
    };
    Error::new(code, message, action)
        .details(json!({"reason":e.to_string()}))
        .exit(3)
}

trait Wire {
    fn identity(&self) -> Value;
    fn write_frame(&mut self, frame: &[u8; FRAME]) -> Result<()>;
    fn read_chunk(&mut self) -> Result<Vec<u8>>;
    fn close(&mut self) -> Result<()>;
}

struct UsbWire {
    tx: Option<Endpoint<Bulk, Out>>,
    rx: Option<Endpoint<Bulk, In>>,
    _interface: Option<Interface>,
    device: Option<nusb::Device>,
    detached_control: bool,
    identity: Value,
}

impl UsbWire {
    fn open() -> Result<Self> {
        let mut matches = nusb::list_devices()
            .wait()
            .map_err(io_error)?
            .filter(|d| d.vendor_id() == VID && d.product_id() == PID)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(Error::new(
                "OPERATOR_COUNT",
                format!("Expected one GB Operator; found {}.", matches.len()),
                "Connect one GB Operator with a data USB cable. Close Playback and disconnect additional Operators.",
            )
            .exit(3));
        }
        let info = matches.pop().unwrap();
        let manufacturer = info.manufacturer_string().unwrap_or("").to_owned();
        let product = info.product_string().unwrap_or("").to_owned();
        if manufacturer != "Epilogue" || product != "GB Operator" {
            return Err(Error::new(
                "OPERATOR_IDENTITY_MISMATCH",
                "A USB device used the GB Operator identifiers but not its product identity.",
                "Disconnect the unexpected device and connect a genuine GB Operator.",
            )
            .details(json!({"manufacturer":manufacturer,"product":product}))
            .exit(3));
        }
        let identity = json!({
            "driver":"operator",
            "product":product,
            "manufacturer":manufacturer,
            "serial":info.serial_number(),
            "usb_version":format!("{:x}.{:02x}",info.usb_version() >> 8, info.usb_version() & 0xff),
            "device_version":format!("{:x}.{:02x}",info.device_version() >> 8, info.device_version() & 0xff),
            "vid":format!("{VID:04x}"),
            "pid":format!("{PID:04x}"),
            "capabilities":{"slots":["gameboy","gba"],"read":true,"backup":true,"verify":true,"write":false,"wipe":false,"save_ram":false}
        });
        let device = info.open().wait().map_err(|e| {
            if e.kind() == nusb::ErrorKind::PermissionDenied {
                Error::new(
                    "USB_PERMISSION_DENIED",
                    "The GB Operator cannot be opened with this account.",
                    "Install Cartridge Studio's exact GB Operator udev rule, reload udev rules, reconnect the reader, then retry.",
                )
                .exit(3)
            } else {
                io_error(e)
            }
        })?;
        // cdc_acm owns the interface association through its control interface.
        // Firmware 9+ accepts bulk commands only after that driver is detached;
        // the data interface itself is then claimed normally.
        let detached_control = device.detach_kernel_driver(0).is_ok();
        let interface = device.claim_interface(1).wait().map_err(|e| {
            Error::new(
                "READER_BUSY",
                "The GB Operator data interface is busy.",
                "Close Playback and other cartridge software, reconnect the reader, then retry.",
            )
            .details(json!({"reason":e.to_string()}))
            .exit(3)
        })?;
        let tx = interface.endpoint::<Bulk, Out>(0x01).map_err(io_error)?;
        let rx = interface.endpoint::<Bulk, In>(0x81).map_err(io_error)?;
        Ok(Self {
            tx: Some(tx),
            rx: Some(rx),
            _interface: Some(interface),
            device: Some(device),
            detached_control,
            identity,
        })
    }
}

impl Wire for UsbWire {
    fn identity(&self) -> Value {
        self.identity.clone()
    }
    fn write_frame(&mut self, frame: &[u8; FRAME]) -> Result<()> {
        let completion = self
            .tx
            .as_mut()
            .ok_or_else(|| io_error("USB interface is closed"))?
            .transfer_blocking(frame.to_vec().into(), Duration::from_secs(3));
        completion.status.map_err(transfer_error)?;
        if completion.actual_len != FRAME {
            return Err(io_error(format!(
                "sent {} of {FRAME} bytes",
                completion.actual_len
            )));
        }
        Ok(())
    }
    fn read_chunk(&mut self) -> Result<Vec<u8>> {
        let rx = self
            .rx
            .as_mut()
            .ok_or_else(|| io_error("USB interface is closed"))?;
        let completion = rx.transfer_blocking(Buffer::new(FRAME), Duration::from_secs(3));
        completion.status.map_err(transfer_error)?;
        if completion.actual_len == 0 {
            return Err(io_error("the reader returned an empty USB transfer"));
        }
        Ok(completion.buffer[..completion.actual_len].to_vec())
    }
    fn close(&mut self) -> Result<()> {
        self.tx.take();
        self.rx.take();
        self._interface.take();
        if self.detached_control {
            self.detached_control = false;
            if let Some(device) = &self.device {
                device.attach_kernel_driver(0).map_err(io_error)?;
            }
        }
        self.device.take();
        Ok(())
    }
}

fn crc32_mpeg2(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= (byte as u32) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04c1_1db7
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn command(payload: &[u8]) -> Result<[u8; FRAME]> {
    if payload.len() > PAYLOAD {
        return Err(Error::check(
            "GB Operator command payload exceeds 60 bytes.",
        ));
    }
    let mut frame = [0; FRAME];
    frame[..payload.len()].copy_from_slice(payload);
    let crc = crc32_mpeg2(&frame[..PAYLOAD]);
    frame[PAYLOAD..].copy_from_slice(&crc.to_le_bytes());
    Ok(frame)
}

#[derive(Clone, Debug)]
enum CartridgeInfo {
    GameBoy { rom_bytes: usize, ram_bytes: usize },
    Gba { record: Vec<u8> },
}

pub struct Reader {
    wire: Box<dyn Wire>,
    cancel: Cancel,
    identity: Value,
    info: Option<CartridgeInfo>,
    rom: Vec<u8>,
    mapper: Option<String>,
    closed: bool,
}

impl Reader {
    pub fn open(cancel: Cancel) -> Result<Self> {
        Self::connect(Box::new(UsbWire::open()?), cancel)
    }
    fn connect(wire: Box<dyn Wire>, cancel: Cancel) -> Result<Self> {
        let identity = wire.identity();
        Ok(Self {
            wire,
            cancel,
            identity,
            info: None,
            rom: vec![],
            mapper: None,
            closed: false,
        })
    }
    fn read_exact(&mut self, length: usize) -> Result<Vec<u8>> {
        let mut data = Vec::with_capacity(length);
        while data.len() < length {
            self.cancel.check()?;
            data.extend(self.wire.read_chunk()?);
        }
        if data.len() != length {
            return Err(Error::new(
                "OPERATOR_RESPONSE_LENGTH",
                "The GB Operator returned an unexpected response length.",
                "Reconnect the reader and retry. Keep the diagnostic report if this firmware uses a different protocol record.",
            )
            .details(json!({"expected":length,"received":data.len()})));
        }
        Ok(data)
    }
    fn exchange(&mut self, frame: &[u8; FRAME]) -> Result<Vec<u8>> {
        self.cancel.check()?;
        self.wire.write_frame(frame)?;
        self.read_exact(FRAME)
    }
    fn stage<T>(result: Result<T>, stage: &str) -> Result<T> {
        result.map_err(|mut error| {
            error.details["operator_stage"] = json!(stage);
            error
        })
    }
    fn cartridge_info(&mut self) -> Result<CartridgeInfo> {
        let request = command(&[0x04])?;
        let _ack = Self::stage(self.exchange(&request), "cartridge_info_ack")?;
        let response = Self::stage(self.read_exact(256), "cartridge_info_data")?;
        let first = &response[..PAYLOAD];
        if first[3] == 0 && first[4] == 0 {
            return Err(Error::new(
                "CARTRIDGE_NOT_FOUND",
                "The GB Operator did not detect a cartridge.",
                "Unplug USB, firmly reseat one cartridge, reconnect, and retry.",
            ));
        }
        if first[2] == 0x30 {
            return Ok(CartridgeInfo::Gba {
                record: first.to_vec(),
            });
        }
        // The low nibble distinguishes GB / Color variants. Restrict the
        // accepted values; an unfamiliar family must not fall through to GB.
        if ![0x20, 0x21, 0x22].contains(&first[2]) {
            return Err(Error::new(
                "OPERATOR_CARTRIDGE_INFO_INVALID",
                "The GB Operator returned an unknown cartridge information format.",
                "Reconnect the reader and retry. Keep the diagnostic report when requesting support for this cartridge or firmware.",
            )
            .details(json!({"cartridge_info":hex::encode(&first[..PAYLOAD])})));
        }
        if first[0] >= 9 && first[26] != 0 {
            self.identity["firmware_version"] =
                json!(format!("{}.{}.{}", first[26], first[27], first[28]));
        }
        self.identity["operator_cartridge_kind"] = json!(first[2]);
        self.identity["operator_cartridge_record"] = json!(hex::encode(first));
        let rom_bytes = u32::from_le_bytes([first[5], first[6], first[7], 0]) as usize;
        let ram_bytes = u32::from_le_bytes([first[9], first[10], first[11], 0]) as usize;
        if !(32 * 1024..=8 * 1024 * 1024).contains(&rom_bytes)
            || !rom_bytes.is_multiple_of(16 * 1024)
        {
            return Err(Error::new(
                "OPERATOR_CARTRIDGE_INFO_INVALID",
                "The GB Operator reported an invalid Game Boy ROM size.",
                "Unplug USB, clean and reseat the cartridge, reconnect, and retry. Keep the diagnostic report if the cartridge uses a special mapper.",
            )
            .details(json!({"rom_bytes":rom_bytes,"ram_bytes":ram_bytes,"cartridge_info":hex::encode(&first[..PAYLOAD])})));
        }
        Ok(CartridgeInfo::GameBoy {
            rom_bytes,
            ram_bytes,
        })
    }
    fn read_rom(&mut self, size: usize) -> Result<Vec<u8>> {
        let mut payload = vec![0x00, 0x00];
        let bytes = (size as u32).to_le_bytes();
        let significant = ((32 - (size as u32).leading_zeros()) as usize).div_ceil(8);
        payload.extend_from_slice(&bytes[..significant.max(1)]);
        let request = command(&payload)?;
        let _first_ack = Self::stage(self.exchange(&request), "rom_request_ack")?;
        let _second_ack = Self::stage(self.exchange(&[0; FRAME]), "rom_start_ack")?;
        let mut data = Vec::with_capacity(size);
        let mut transfers = 0;
        while data.len() < size {
            self.cancel.check()?;
            data.extend_from_slice(&Self::stage(self.wire.read_chunk(), "rom_data")?);
            transfers += 1;
            if transfers % ACK_INTERVAL == 0 && data.len() < size {
                let _ack = Self::stage(self.exchange(&[0; FRAME]), "rom_flow_ack")?;
            }
        }
        data.truncate(size);
        Ok(data)
    }
    fn checked_ack(&mut self, frame: &[u8; FRAME], stage: &str) -> Result<()> {
        let reply = Self::stage(self.exchange(frame), stage)?;
        if reply.iter().any(|b| *b != 0) {
            return Err(unexpected_reply(stage, &reply));
        }
        Ok(())
    }

    fn program_legacy(&mut self, target: &[u8], progress: &mut dyn FnMut(String)) -> Result<Value> {
        if !LEGACY_WRITE_QUALIFIED {
            return Err(crate::operator_programming::unavailable());
        }
        if target.len() != crate::rom::CAPACITY || self.identity["firmware_version"] != "9.5.0" {
            return Err(crate::operator_programming::unavailable());
        }
        let ram_bytes = match self.info {
            Some(CartridgeInfo::GameBoy { ram_bytes, .. }) => ram_bytes,
            _ => return Err(crate::operator_programming::unavailable()),
        };
        let mut payload = vec![1, if ram_bytes == 0 { 0 } else { 2 }];
        payload.extend_from_slice(&(target.len() as u32).to_le_bytes());
        payload.extend_from_slice(&(ram_bytes as u32).to_le_bytes());
        self.rom.clear();
        self.checked_ack(&command(&payload)?, "write_command_ack")?;
        let started = Instant::now();
        let mut busy_packets = 0u32;
        loop {
            self.cancel.check()?;
            if started.elapsed() >= Duration::from_secs(420) || busy_packets >= 100_000 {
                return Err(Error::new("OPERATOR_ERASE_TIMEOUT", "The Operator did not finish erasing in time.",
                    "Keep the backups and report. Reconnect USB before recovery; the cartridge may be partially erased."));
            }
            let reply = Self::stage(self.read_exact(FRAME), "erase_status")?;
            if reply.iter().all(|b| *b == 0) {
                break;
            }
            if reply[..2] == [0x33, 0xcc] && reply[2..].iter().all(|b| *b == 0) {
                busy_packets += 1;
            } else {
                return Err(unexpected_reply("erase_status", &reply));
            }
        }
        progress("Operator is ready for ROM data; final readbacks will verify the result.".into());
        for (index, frame) in target.as_chunks::<FRAME>().0.iter().enumerate() {
            if index.is_multiple_of(256) {
                progress(format!("Starting bank {}/32…", index / 256 + 1));
                self.checked_ack(&[0; FRAME], "write_bank_ack")?;
            }
            self.checked_ack(frame, "write_data_ack").map_err(|mut e| {
                e.details["offset"] = json!(index * FRAME);
                e
            })?;
            if (index + 1).is_multiple_of(256) {
                progress(format!(
                    "Programmed: {}/512 KiB",
                    (index + 1) * FRAME / 1024
                ));
            }
        }
        Ok(json!({"protocol":"legacy-9.5.0","command":1,
            "save_chip_parameter":payload[1],"save_bytes_parameter":ram_bytes,
            "erase_busy_packets":busy_packets,"erase_ready_reply":"zero-filled-64-byte-frame",
            "data_frames":target.len()/FRAME,"bank_handshakes":target.len()/16384,
            "acknowledgements":"zero-filled-64-byte-frames","verification":false}))
    }

    /// The caller must explicitly confirm the physical Ferrante 512 board.
    /// This never claims an electronically observed JEDEC ID.
    pub fn confirmed_ferrante512(self) -> FerranteProgrammer {
        FerranteProgrammer {
            reader: self,
            record: None,
        }
    }

    fn close_device(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        self.wire.close()
    }
}

fn unexpected_reply(stage: &str, reply: &[u8]) -> Error {
    Error::new("OPERATOR_UNEXPECTED_REPLY", "The Operator returned an unrecognized programming response.",
        "Keep the report and backups. Reconnect USB before recovery; do not treat this write as successful.")
        .details(json!({"operator_stage":stage,"reply":hex::encode(reply)}))
}

pub struct FerranteProgrammer {
    reader: Reader,
    record: Option<String>,
}
impl crate::operator_programming::Programmer for FerranteProgrammer {
    fn qualify(&mut self) -> Result<crate::operator_programming::Qualification> {
        if !LEGACY_WRITE_QUALIFIED {
            return Err(crate::operator_programming::unavailable());
        }
        let info = self.reader.cartridge_info()?;
        let record = self.reader.identity["operator_cartridge_record"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        let bytes = hex::decode(&record).map_err(|_| crate::operator_programming::unavailable())?;
        if self.reader.identity["firmware_version"] != "9.5.0"
            || bytes.len() < 29
            || ![0x19, 0x1a, 0x1b].contains(&bytes[14])
            || !matches!(info, CartridgeInfo::GameBoy {rom_bytes, ..} if rom_bytes <= crate::rom::CAPACITY)
        {
            return Err(
                crate::operator_programming::unavailable().details(self.reader.identity.clone())
            );
        }
        if self.record.as_ref().is_some_and(|old| old != &record) {
            return Err(Error::new(
                "CARTRIDGE_CHANGED",
                "The cartridge record changed during backup.",
                "Reconnect the confirmed Ferrante 512 and retry into a new folder.",
            ));
        }
        self.record = Some(record);
        self.reader.info = Some(info);
        Ok(crate::operator_programming::Qualification {
            device_serial: self.reader.identity["serial"].as_str().unwrap_or("").into(),
            firmware: "9.5.0".into(),
            profile: crate::rom::GB_PROFILE.into(),
            capacity: crate::rom::CAPACITY,
            identification: "user-confirmed-ferrante-512".into(),
            manufacturer_id: None,
            device_id: None,
        })
    }
    fn read_full(&mut self, capacity: usize) -> Result<Vec<u8>> {
        if capacity != crate::rom::CAPACITY || self.record.is_none() {
            return Err(crate::operator_programming::unavailable());
        }
        // Explicit size, fresh USB request each time; never read the cached ROM.
        self.reader.read_rom(capacity)
    }
    fn erase_and_program(
        &mut self,
        target: &[u8],
        progress: &mut dyn FnMut(String),
    ) -> Result<Value> {
        if self.record.is_none() {
            return Err(crate::operator_programming::unavailable());
        }
        self.reader.program_legacy(target, progress)
    }
    fn check_cancel(&self) -> Result<()> {
        self.reader.cancel.check()
    }
    fn close(&mut self) -> Result<()> {
        self.reader.close_device()
    }
}

impl gb::RomReader for Reader {
    fn initialize(&mut self) -> Result<()> {
        self.cancel.check()?;
        let info = self.cartridge_info()?;
        let (rom_bytes, ram_bytes) = match &info {
            CartridgeInfo::GameBoy {
                rom_bytes,
                ram_bytes,
            } => (*rom_bytes, *ram_bytes),
            CartridgeInfo::Gba { record } => {
                return Err(Error::new(
                    "WRONG_CARTRIDGE_SLOT",
                    "The GB Operator detected a Game Boy Advance cartridge while Game Boy / Color was selected.",
                    "Choose Game Boy Advance and retry; no save-memory or write commands were sent.",
                )
                .details(json!({"cartridge_info":hex::encode(record)})));
            }
        };
        let rom = self.read_rom(rom_bytes)?;
        self.identity["cartridge_ram_bytes"] = json!(ram_bytes);
        self.identity["cartridge_rom_bytes"] = json!(rom_bytes);
        self.info = Some(info);
        self.rom = rom;
        Ok(())
    }
    fn header_bytes(&mut self) -> Result<Vec<u8>> {
        if self.rom.len() < 0x150 {
            return Err(Error::check("GB Operator ROM is not initialized."));
        }
        Ok(self.rom[..0x150].to_vec())
    }
    fn set_mapper(&mut self, mapper: String) {
        self.mapper = Some(mapper);
    }
    fn read_bank(&mut self, bank: usize) -> Result<Vec<u8>> {
        self.cancel.check()?;
        if self.mapper.is_none() {
            return Err(Error::check(
                "GB Operator read requires a validated mapper.",
            ));
        }
        let start = bank
            .checked_mul(16 * 1024)
            .ok_or_else(|| Error::check("Game Boy bank address overflow."))?;
        let end = start + 16 * 1024;
        self.rom
            .get(start..end)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| Error::check("Game Boy bank exceeds the GB Operator ROM image."))
    }
    fn identity(&self) -> Value {
        self.identity.clone()
    }
    fn close(&mut self) -> Result<()> {
        self.close_device()
    }
}

impl gba::RomReader for Reader {
    fn initialize(&mut self) -> Result<()> {
        self.cancel.check()?;
        let info = self.cartridge_info()?;
        let record = match &info {
            CartridgeInfo::Gba { record } => record,
            CartridgeInfo::GameBoy { .. } => {
                return Err(Error::new(
                    "WRONG_CARTRIDGE_SLOT",
                    "The GB Operator detected a Game Boy / Color cartridge while Game Boy Advance was selected.",
                    "Choose Game Boy / Color and retry; no save-memory or write commands were sent.",
                ));
            }
        };
        self.identity["cartridge_family"] = json!("gba");
        self.identity["cartridge_info"] = json!(hex::encode(record));
        self.info = Some(info);
        self.rom.clear();
        self.mapper = None;
        Ok(())
    }
    fn prepare(&mut self, rom_bytes: usize) -> Result<()> {
        gba::validate_size(rom_bytes)?;
        self.rom = self.read_rom(rom_bytes)?;
        self.identity["cartridge_rom_bytes"] = json!(rom_bytes);
        Ok(())
    }
    fn read(&mut self, address: usize, length: usize) -> Result<Vec<u8>> {
        self.cancel.check()?;
        let end = address
            .checked_add(length)
            .ok_or_else(|| Error::check("GBA ROM address overflow."))?;
        if end > gba::MAX_SIZE {
            return Err(Error::check(
                "GBA ROM read exceeds the 32 MiB address window.",
            ));
        }
        if self.rom.len() < end {
            let transfer_bytes = if end <= 0x100 { 0x100 } else { gba::MAX_SIZE };
            self.rom = self.read_rom(transfer_bytes)?;
        }
        self.rom
            .get(address..end)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| Error::check("GBA ROM read exceeds the GB Operator image."))
    }
    fn identity(&self) -> Value {
        self.identity.clone()
    }
    fn close(&mut self) -> Result<()> {
        self.close_device()
    }
}

impl gb::FlashWriter for Reader {
    fn check_cancel(&self) -> Result<()> {
        self.cancel.check()
    }
    fn prepare_program(&mut self) -> Result<()> {
        Err(crate::readers::unsupported("ROM programming"))
    }
    fn erase(&mut self) -> Result<()> {
        Err(crate::readers::unsupported("ROM erasing"))
    }
    fn program_bank(&mut self, _bank: usize, _data: &[u8]) -> Result<()> {
        Err(crate::readers::unsupported("ROM programming"))
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        let _ = self.close_device();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gb::RomReader;
    use std::{cell::RefCell, collections::VecDeque, rc::Rc};

    #[derive(Default)]
    struct State {
        queued: VecDeque<Vec<u8>>,
        rom: Vec<u8>,
        position: usize,
        stream_end: usize,
        streaming: bool,
        waiting_for_start: bool,
        waiting_for_ack: bool,
        commands: Vec<u8>,
        gba: bool,
        gb_kind: Option<u8>,
    }
    struct Fake(Rc<RefCell<State>>);
    fn checked_frame(mut payload: [u8; FRAME]) -> [u8; FRAME] {
        let crc = crc32_mpeg2(&payload[..PAYLOAD]);
        payload[PAYLOAD..].copy_from_slice(&crc.to_le_bytes());
        payload
    }
    impl Wire for Fake {
        fn identity(&self) -> Value {
            json!({"driver":"operator","product":"GB Operator"})
        }
        fn write_frame(&mut self, frame: &[u8; FRAME]) -> Result<()> {
            let mut s = self.0.borrow_mut();
            if frame.iter().all(|&b| b == 0) {
                if s.waiting_for_start {
                    s.waiting_for_start = false;
                    s.streaming = true;
                } else if s.waiting_for_ack {
                    s.waiting_for_ack = false;
                    s.streaming = true;
                } else {
                    panic!("unexpected acknowledgement");
                }
                s.queued.push_back(vec![0; PAYLOAD]);
                s.queued.push_back(vec![0; FRAME - PAYLOAD]);
                return Ok(());
            }
            assert_eq!(
                u32::from_le_bytes(frame[PAYLOAD..].try_into().unwrap()),
                crc32_mpeg2(&frame[..PAYLOAD])
            );
            s.commands.push(frame[0]);
            match frame[0] {
                0x04 => {
                    s.queued.push_back(vec![0; PAYLOAD]);
                    s.queued.push_back(vec![0; FRAME - PAYLOAD]);
                    let mut info = [0; FRAME];
                    info[0] = 9;
                    info[26..29].copy_from_slice(&[9, 5, 0]);
                    info[2] = if s.gba {
                        0x30
                    } else {
                        s.gb_kind.unwrap_or(0x20)
                    };
                    info[3] = 1;
                    if s.gba {
                        info[13..18].copy_from_slice(b"TTST0");
                    } else {
                        let size = s.rom.len() as u32;
                        info[5..8].copy_from_slice(&size.to_le_bytes()[..3]);
                        info[13] = b'S';
                        info[14] = 0;
                        info[15] = 0;
                    }
                    s.queued.push_back(checked_frame(info)[..PAYLOAD].to_vec());
                    s.queued.push_back(vec![0; PAYLOAD]);
                    s.queued.push_back(vec![0; PAYLOAD]);
                    s.queued.push_back(vec![0; PAYLOAD]);
                    s.queued.push_back(vec![0; 16]);
                }
                0x00 => {
                    let size = u32::from_le_bytes(frame[2..6].try_into().unwrap()) as usize;
                    assert!(size <= s.rom.len());
                    s.position = 0;
                    s.stream_end = size;
                    s.waiting_for_start = true;
                    s.queued.push_back(vec![0; PAYLOAD]);
                    s.queued.push_back(vec![0; FRAME - PAYLOAD]);
                }
                _ => panic!("unexpected command"),
            }
            Ok(())
        }
        fn read_chunk(&mut self) -> Result<Vec<u8>> {
            let mut s = self.0.borrow_mut();
            if let Some(frame) = s.queued.pop_front() {
                return Ok(frame);
            }
            assert!(s.streaming);
            let mut frame = [0; FRAME];
            let end = (s.position + FRAME).min(s.stream_end);
            frame[..end - s.position].copy_from_slice(&s.rom[s.position..end]);
            s.position = end;
            if (s.position / FRAME).is_multiple_of(ACK_INTERVAL) && s.position < s.stream_end {
                s.streaming = false;
                s.waiting_for_ack = true;
            }
            Ok(frame.to_vec())
        }
        fn close(&mut self) -> Result<()> {
            Ok(())
        }
    }

    struct ProgramWire {
        sent: Rc<RefCell<Vec<[u8; FRAME]>>>,
        replies: VecDeque<Vec<u8>>,
    }
    impl Wire for ProgramWire {
        fn identity(&self) -> Value {
            json!({"driver":"operator","serial":"synthetic","firmware_version":"9.5.0"})
        }
        fn write_frame(&mut self, frame: &[u8; FRAME]) -> Result<()> {
            self.sent.borrow_mut().push(*frame);
            Ok(())
        }
        fn read_chunk(&mut self) -> Result<Vec<u8>> {
            self.replies
                .pop_front()
                .ok_or_else(|| Error::new("OPERATOR_USB_TIMEOUT", "Timed out", "Reconnect"))
        }
        fn close(&mut self) -> Result<()> {
            Ok(())
        }
    }
    fn programming_reader(replies: Vec<Vec<u8>>) -> (Reader, Rc<RefCell<Vec<[u8; FRAME]>>>) {
        let sent = Rc::new(RefCell::new(vec![]));
        let mut r = Reader::connect(
            Box::new(ProgramWire {
                sent: sent.clone(),
                replies: replies.into(),
            }),
            Cancel::default(),
        )
        .unwrap();
        r.info = Some(CartridgeInfo::GameBoy {
            rom_bytes: 524288,
            ram_bytes: 32768,
        });
        (r, sent)
    }
    #[test]
    fn legacy_write_has_crc_command_bank_handshakes_and_exact_data() {
        let mut busy = vec![0; FRAME];
        busy[..2].copy_from_slice(&[0x33, 0xcc]);
        let mut replies = vec![vec![0; FRAME], busy, vec![0; FRAME]];
        replies.extend(vec![vec![0; FRAME]; 8192 + 32]);
        let (mut r, sent) = programming_reader(replies);
        let target: Vec<u8> = (0..524288).map(|i| ((i / 16384 + i) % 251) as u8).collect();
        let report = r.program_legacy(&target, &mut |_| {}).unwrap();
        assert_eq!(report["erase_busy_packets"], 1);
        assert_eq!(report["verification"], false);
        let sent = sent.borrow();
        assert_eq!(&sent[0][..10], &[1, 2, 0, 0, 8, 0, 0, 128, 0, 0]);
        assert_eq!(
            u32::from_le_bytes(sent[0][60..].try_into().unwrap()),
            crc32_mpeg2(&sent[0][..60])
        );
        assert_eq!(sent.len(), 1 + 32 + 8192);
        for bank in 0..32 {
            let start = 1 + bank * 257;
            assert_eq!(sent[start], [0; FRAME]);
            let bytes: Vec<u8> = sent[start + 1..start + 257]
                .iter()
                .flatten()
                .copied()
                .collect();
            assert_eq!(bytes, target[bank * 16384..(bank + 1) * 16384]);
        }
    }
    #[test]
    fn legacy_write_stops_on_unknown_ack_erase_status_or_timeout() {
        for (replies, code) in [
            (vec![vec![0xee; FRAME]], "OPERATOR_UNEXPECTED_REPLY"),
            (
                vec![vec![0; FRAME], vec![0x55; FRAME]],
                "OPERATOR_UNEXPECTED_REPLY",
            ),
            (vec![vec![0; FRAME]], "OPERATOR_USB_TIMEOUT"),
        ] {
            let (mut r, sent) = programming_reader(replies);
            assert_eq!(
                r.program_legacy(&vec![0; 524288], &mut |_| {})
                    .unwrap_err()
                    .code,
                code
            );
            assert_eq!(sent.borrow().len(), 1);
        }
    }
    #[test]
    fn vendor_command_ack_without_erase_ready_never_sends_rom_data() {
        // Playback's failed 9.5.0 reference attempt received a command ACK as
        // 60 + 4 bytes, then lost the device before erase readiness. That ACK
        // alone must never be treated as permission to start data streaming.
        let (mut r, sent) = programming_reader(vec![vec![0; 60], vec![0; 4]]);
        r.info = Some(CartridgeInfo::GameBoy {
            rom_bytes: 262144,
            ram_bytes: 0,
        });
        let error = r.program_legacy(&vec![0; 524288], &mut |_| {}).unwrap_err();
        assert_eq!(error.code, "OPERATOR_USB_TIMEOUT");
        assert_eq!(error.details["operator_stage"], "erase_status");
        assert_eq!(sent.borrow().len(), 1);
        assert_eq!(&sent.borrow()[0][..10], &[1, 0, 0, 0, 8, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn data_rejection_records_exact_offset_and_stops_streaming() {
        // Command, erase-ready, first bank handshake, then ten data ACKs.
        let mut replies = vec![vec![0; FRAME]; 13];
        replies.push(vec![0xee; FRAME]);
        let (mut r, sent) = programming_reader(replies);
        let error = r.program_legacy(&vec![0; 524288], &mut |_| {}).unwrap_err();
        assert_eq!(error.code, "OPERATOR_UNEXPECTED_REPLY");
        assert_eq!(error.details["operator_stage"], "write_data_ack");
        assert_eq!(error.details["offset"], 640);
        assert_eq!(sent.borrow().len(), 13); // command + handshake + eleven data frames
    }

    #[test]
    fn legacy_write_requires_exact_firmware_and_checks_cancellation_before_command() {
        let (mut r, sent) = programming_reader(vec![]);
        r.identity["firmware_version"] = json!("9.5.1");
        assert!(r.program_legacy(&vec![0; 524288], &mut |_| {}).is_err());
        assert!(sent.borrow().is_empty());
        r.identity["firmware_version"] = json!("9.5.0");
        r.cancel.0.store(true, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(
            r.program_legacy(&vec![0; 524288], &mut |_| {})
                .unwrap_err()
                .code,
            "INTERRUPTED"
        );
        assert!(sent.borrow().is_empty());
    }

    #[test]
    fn mpeg2_crc_and_command_frame_are_exact() {
        assert_eq!(crc32_mpeg2(b"123456789"), 0x0376_e6e7);
        let frame = command(&[4]).unwrap();
        assert_eq!(frame.len(), FRAME);
        assert_eq!(
            u32::from_le_bytes(frame[PAYLOAD..].try_into().unwrap()),
            crc32_mpeg2(&frame[..PAYLOAD])
        );
    }

    #[test]
    fn gameboy_rom_is_read_with_periodic_flow_control() {
        let mut rom = crate::gba::fixture();
        rom.resize(512 * 1024, 0x5a);
        rom[0x134..0x13d].copy_from_slice(b"CART TEST");
        rom[0x147] = 0x19;
        rom[0x148] = 4;
        rom[0x14d] = rom[0x134..0x14d]
            .iter()
            .fold(0u8, |sum, b| sum.wrapping_sub(*b).wrapping_sub(1));
        let state = Rc::new(RefCell::new(State {
            rom: rom.clone(),
            ..Default::default()
        }));
        let mut reader = Reader::connect(Box::new(Fake(state.clone())), Cancel::default()).unwrap();
        gb::RomReader::initialize(&mut reader).unwrap();
        reader.set_mapper("MBC5".into());
        assert_eq!(reader.header_bytes().unwrap(), rom[..0x150]);
        assert_eq!(reader.read_bank(31).unwrap(), rom[31 * 16384..]);
        assert_eq!(state.borrow().commands, [4, 0]);
    }

    #[test]
    fn color_cartridge_records_read_without_programming_commands() {
        for kind in [0x21, 0x22] {
            let rom = vec![0x5a; 32 * 1024];
            let state = Rc::new(RefCell::new(State {
                rom: rom.clone(),
                gb_kind: Some(kind),
                ..Default::default()
            }));
            let mut reader =
                Reader::connect(Box::new(Fake(state.clone())), Cancel::default()).unwrap();
            gb::RomReader::initialize(&mut reader).unwrap();
            reader.set_mapper("ROM".into());
            assert_eq!(reader.read_bank(1).unwrap(), rom[16384..]);
            assert_eq!(reader.identity["operator_cartridge_kind"], kind);
            assert_eq!(reader.identity["firmware_version"], "9.5.0");
            assert_eq!(state.borrow().commands, [4, 0]);
        }
    }

    #[test]
    fn repeated_initialization_fetches_fresh_bytes_instead_of_reusing_cache() {
        let state = Rc::new(RefCell::new(State {
            rom: vec![0x5a; 32 * 1024],
            gb_kind: Some(0x22),
            ..Default::default()
        }));
        let mut reader = Reader::connect(Box::new(Fake(state.clone())), Cancel::default()).unwrap();
        reader.set_mapper("ROM".into());
        gb::RomReader::initialize(&mut reader).unwrap();
        assert_eq!(reader.read_bank(1).unwrap()[0], 0x5a);
        state.borrow_mut().rom[16384] = 0xa5;
        gb::RomReader::initialize(&mut reader).unwrap();
        assert_eq!(reader.read_bank(1).unwrap()[0], 0xa5);
        assert_eq!(state.borrow().commands, [4, 0, 4, 0]);
    }

    #[test]
    fn unknown_cartridge_kind_stops_before_reading_rom() {
        let state = Rc::new(RefCell::new(State {
            rom: vec![0; 32 * 1024],
            gb_kind: Some(0x23),
            ..Default::default()
        }));
        let mut reader = Reader::connect(Box::new(Fake(state.clone())), Cancel::default()).unwrap();
        let error = gb::RomReader::initialize(&mut reader).unwrap_err();
        assert_eq!(error.code, "OPERATOR_CARTRIDGE_INFO_INVALID");
        assert_eq!(state.borrow().commands, [4]);
    }

    #[test]
    fn gba_uses_a_header_probe_then_one_complete_prepared_stream() {
        let rom = crate::gba::fixture();
        let state = Rc::new(RefCell::new(State {
            rom: rom.clone(),
            gba: true,
            ..Default::default()
        }));
        let mut reader = Reader::connect(Box::new(Fake(state.clone())), Cancel::default()).unwrap();
        gba::RomReader::initialize(&mut reader).unwrap();
        assert_eq!(
            gba::RomReader::read(&mut reader, 0, 0x100).unwrap(),
            rom[..0x100]
        );
        gba::RomReader::prepare(&mut reader, rom.len()).unwrap();
        assert_eq!(
            gba::RomReader::read(&mut reader, 0x20000, 4096).unwrap(),
            rom[0x20000..0x21000]
        );
        assert_eq!(state.borrow().commands, [4, 0, 0]);
        assert_eq!(reader.identity["cartridge_rom_bytes"], rom.len());
    }
}
