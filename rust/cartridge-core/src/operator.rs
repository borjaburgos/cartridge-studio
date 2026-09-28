//! Native, read-only Epilogue GB Operator transport.
//!
//! The reader uses 64-byte frames on its CDC data interface. ROM programming
//! and save-memory commands are intentionally absent until compatible writable
//! cartridges and save transactions receive separate physical qualification.
use crate::{gb, gba, storage::Cancel, Error, Result};
use nusb::{
    transfer::{Buffer, Bulk, In, Out, TransferError},
    Endpoint, Interface, MaybeFuture,
};
use serde_json::{json, Value};
use std::time::Duration;

const VID: u16 = 0x16d0;
const PID: u16 = 0x123d;
const FRAME: usize = 64;
const PAYLOAD: usize = 60;
const ACK_INTERVAL: usize = 320;

fn io_error(e: impl std::fmt::Display) -> Error {
    Error::new(
        "OPERATOR_USB_IO",
        "Communication with the GB Operator failed.",
        "Close Playback and other cartridge software, reconnect the GB Operator directly with a data USB cable, then retry.",
    )
    .details(json!({"reason":e.to_string()}))
    .exit(3)
}
#[cfg(target_os = "linux")]
fn permission_action() -> &'static str {
    "Install Cartridge Studio's exact GB Operator udev rule, reload udev rules, reconnect the reader, then retry."
}
#[cfg(target_os = "macos")]
fn permission_action() -> &'static str {
    "Close Playback and other cartridge applications, reconnect the GB Operator directly with a data-capable cable, then retry."
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn permission_action() -> &'static str {
    "Close other cartridge applications, reconnect the GB Operator directly with a data-capable cable, then retry."
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
                    permission_action(),
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
        if first[2] != 0x20 {
            return Err(Error::new(
                "OPERATOR_CARTRIDGE_INFO_INVALID",
                "The GB Operator returned an unknown cartridge information format.",
                "Reconnect the reader and retry. Keep the diagnostic report when requesting support for this cartridge or firmware.",
            )
            .details(json!({"cartridge_info":hex::encode(&first[..PAYLOAD])})));
        }
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
    fn close_device(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        self.wire.close()
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
                    info[2] = if s.gba { 0x30 } else { 0x20 };
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
