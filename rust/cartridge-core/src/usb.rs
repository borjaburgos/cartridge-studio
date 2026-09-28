//! The original device-level vendor protocol, with no host libusb dependency.
use crate::{storage::Cancel, Error, Result};
use nusb::{
    transfer::{ControlIn, ControlOut, ControlType, Recipient, TransferError},
    MaybeFuture,
};
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    time::{Duration, Instant},
};

pub trait Transport {
    fn identity(&self) -> Value;
    fn input(&mut self, request: u8, value: u16, index: u16, length: u16) -> Result<Vec<u8>>;
    fn output(&mut self, request: u8, value: u16, index: u16, data: &[u8]) -> Result<()>;
}
pub struct Usb {
    device: nusb::Device,
    identity: Value,
    _lock: File,
}
fn usb_error(e: impl std::fmt::Display) -> Error {
    Error::new("USB_IO","USB communication failed.","Reconnect the reader directly to the computer and retry. Keep backups and the saved source if writing was interrupted.").details(json!({"reason":e.to_string()})).exit(3)
}
fn transfer_error(e: TransferError) -> Error {
    let (code,message,action)=match e{
        TransferError::Disconnected=>("READER_DISCONNECTED","The reader disconnected.","Reconnect it. If writing was interrupted, restore the source retained in the backup folder."),
        TransferError::Cancelled=>("USB_TIMEOUT","The reader did not respond in time.","Reconnect the reader and retry. Restore the saved source if writing was interrupted."),
        TransferError::Stall=>("USB_COMMAND_UNSUPPORTED","The reader rejected this USB command.","Check the selected console and board profile. Keep the diagnostic report for firmware compatibility troubleshooting."),
        _=>("USB_IO","USB communication failed.","Reconnect the reader directly to the computer and retry."),
    };
    Error::new(code, message, action)
        .details(json!({"reason":e.to_string()}))
        .exit(3)
}
#[cfg(target_os = "linux")]
fn permission_action() -> &'static str {
    "Install the included 70-cartridge-studio.rules file in /etc/udev/rules.d, reload udev rules and reconnect. With the optional CLI, run sudo cartridge usb-setup."
}
#[cfg(target_os = "macos")]
fn permission_action() -> &'static str {
    "Close other cartridge applications, reconnect the reader directly with a data-capable cable, and retry. If access is still denied, reconnect it after signing out of the other application."
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn permission_action() -> &'static str {
    "Close other cartridge applications, reconnect the reader directly with a data-capable cable, and retry."
}
impl Usb {
    pub fn open() -> Result<Self> {
        let mut matched = Vec::new();
        let mut denied = Vec::new();
        for info in nusb::list_devices().wait().map_err(usb_error)? {
            if info.vendor_id() != 0x16c0 || info.product_id() != 0x05dc {
                continue;
            }
            let dev = match info.open().wait() {
                Ok(d) => d,
                Err(e) if e.kind() == nusb::ErrorKind::PermissionDenied => {
                    denied.push(info.bus_id().to_owned());
                    continue;
                }
                Err(e) => return Err(usb_error(e)),
            };
            let desc = dev.device_descriptor();
            let string = |index: Option<std::num::NonZeroU8>| -> Result<String> {
                match index {
                    Some(i) => {
                        // This firmware truncates oversized standard descriptor requests.
                        // libusb requests 255 bytes; nusb's convenience API requests 4096.
                        let raw = dev
                            .control_in(
                                ControlIn {
                                    control_type: ControlType::Standard,
                                    recipient: Recipient::Device,
                                    request: 6,
                                    value: 0x300 | i.get() as u16,
                                    index: 0x409,
                                    length: 255,
                                },
                                Duration::from_secs(5),
                            )
                            .wait()
                            .map_err(transfer_error)?;
                        decode_string(&raw)
                    }
                    None => Ok(String::new()),
                }
            };
            let manufacturer = string(desc.manufacturer_string_index())?;
            let product = string(desc.product_string_index())?;
            if manufacturer != "InfiniteNesLives.com" || product != "INL Retro-Prog" {
                continue;
            }
            #[cfg(target_os = "linux")]
            let bus = info.busnum().to_string();
            #[cfg(not(target_os = "linux"))]
            let bus = info.bus_id().to_owned();
            let address = info.device_address();
            let path = if cfg!(target_os = "linux") {
                format!(
                    "/dev/bus/usb/{:03}/{address:03}",
                    bus.parse::<u8>().unwrap_or(0)
                )
            } else {
                format!("USB bus {bus}, device {address}")
            };
            matched.push((dev,json!({"product":product,"manufacturer":manufacturer,"firmware_usb":format!("{:04x}",info.device_version()),"bus":bus.parse::<u64>().map(Value::from).unwrap_or(json!(bus)),"address":address,"path":path}),bus,address));
        }
        if matched.is_empty() && !denied.is_empty() {
            return Err(Error::new(
                "USB_PERMISSION_DENIED",
                "The reader cannot be opened with this account.",
                permission_action(),
            )
            .details(json!({"devices":denied}))
            .exit(3));
        }
        if matched.len() != 1 {
            return Err(Error::new("READER_COUNT",format!("Expected one INLretro reader; found {}.",matched.len()),"Connect one INLretro in normal mode with a data-capable USB cable. Disconnect additional readers. Do not hold BL.").exit(3));
        }
        let (device, identity, bus, address) = matched.pop().unwrap();
        // Same lock as the retired Python transport, independent of library choice.
        let uid = unsafe { libc::getuid() };
        let dir = std::env::temp_dir().join(format!("inlretro-usb-{uid}"));
        match fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e.into()),
        }
        let meta = fs::symlink_metadata(&dir)?;
        if !meta.is_dir() || meta.uid() != uid || meta.mode() & 0o077 != 0 {
            return Err(Error::new("LOCK_DIRECTORY_UNSAFE","The shared USB lock directory has unsafe ownership or permissions.","Remove the stale INLretro lock directory from the system temporary folder, then retry.").exit(3));
        }
        let lock = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(dir.join(format!("reader-{bus}-{address}.lock")))?;
        if lock.metadata()?.uid() != uid {
            return Err(Error::check("The reader lock belongs to another account."));
        }
        fs2::FileExt::try_lock_exclusive(&lock).map_err(|_| {
            Error::new(
                "READER_BUSY",
                "Another cartridge application is already using this reader.",
                "Wait for that operation to finish or stop it before starting another command.",
            )
            .exit(3)
        })?;
        Ok(Self {
            device,
            identity,
            _lock: lock,
        })
    }
}
impl Transport for Usb {
    fn identity(&self) -> Value {
        self.identity.clone()
    }
    fn input(&mut self, request: u8, value: u16, index: u16, length: u16) -> Result<Vec<u8>> {
        self.device
            .control_in(
                ControlIn {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Device,
                    request,
                    value,
                    index,
                    length,
                },
                Duration::from_secs(5),
            )
            .wait()
            .map_err(transfer_error)
    }
    fn output(&mut self, request: u8, value: u16, index: u16, data: &[u8]) -> Result<()> {
        self.device
            .control_out(
                ControlOut {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Device,
                    request,
                    value,
                    index,
                    data,
                },
                Duration::from_secs(5),
            )
            .wait()
            .map_err(transfer_error)
    }
}
pub struct Bus {
    pub transport: Box<dyn Transport>,
    pub cancel: Cancel,
}
impl Bus {
    pub fn open(cancel: Cancel) -> Result<Self> {
        Ok(Self {
            transport: Box::new(Usb::open()?),
            cancel,
        })
    }
    pub fn identity(&self) -> Value {
        self.transport.identity()
    }
    // Raw transfers deliberately bypass cancellation so cleanup can always restore pins.
    pub fn transfer(
        &mut self,
        d: u8,
        o: u8,
        a: u16,
        m: u8,
        len: u16,
        payload: bool,
    ) -> Result<Vec<u8>> {
        let data = self
            .transport
            .input(d, o as u16 | ((m as u16) << 8), a, len)?;
        if data.len() != len as usize {
            return Err(Error::new(
                "USB_SHORT_READ",
                format!("Received {} bytes; expected {len}.", data.len()),
                "Reconnect the reader and repeat the backup. Do not use a partial ROM.",
            )
            .exit(3));
        }
        if !payload && (data.is_empty() || data[0] != 0 || (len > 1 && data[1] as u16 != len - 2)) {
            return Err(Error::new("FIRMWARE_COMMAND","The reader firmware rejected the requested operation.","Check the console and physical board profile. Keep this command information when reporting unsupported firmware.").details(json!({"dictionary":d,"opcode":o,"response":hex::encode(data)})).exit(3));
        }
        Ok(data)
    }
    pub fn cmd(&mut self, d: u8, o: u8, a: u16, m: u8) -> Result<()> {
        self.transfer(d, o, a, m, 1, false)?;
        Ok(())
    }
    pub fn byte(&mut self, d: u8, o: u8, a: u16) -> Result<u8> {
        Ok(self.transfer(d, o, a, 0, 3, false)?[2])
    }
    pub fn stop(&mut self) -> Result<()> {
        let a = self.cmd(7, 0, 1, 0);
        let b = self.cmd(5, 0, 0, 0);
        a.and(b)
    }
    pub fn wait(&mut self, accepted: &[u8], buffer: Option<u8>) -> Result<()> {
        let end = Instant::now() + Duration::from_secs(8);
        loop {
            self.cancel.check()?;
            let status = match buffer {
                None => self.byte(5, 0x61, 0)?,
                Some(n) => self.transfer(5, 0x50, 0, n, 8, false)?[3],
            };
            if accepted.contains(&status) {
                return Ok(());
            }
            if ![
                0, 0x20, 0x80, 0x90, 0x98, 0xc0, 0xd0, 0xd2, 0xd8, 0xe0, 0xf0, 0xf2, 0xf4, 0xf8,
            ]
            .contains(&status)
            {
                return Err(Error::new("BUFFER_FAILURE",format!("The reader reported buffer status ${status:02X}."),"Reconnect and check the board profile. If writing was interrupted, restore the saved source from the backup folder.").exit(3));
            }
            if Instant::now() > end {
                return Err(Error::new("BUFFER_TIMEOUT","The reader stopped making progress.","Reconnect USB. If writing was interrupted, restore the saved source from the backup folder.").exit(3));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    pub fn block(&mut self, mem: u16, address: u16, size: usize) -> Result<Vec<u8>> {
        self.cancel.check()?;
        self.stop()?;
        let result = (|| {
            for n in 0..2 {
                self.cmd(5, 0x80 + n, if n == 1 { 0x8004 } else { 0 }, 4)?;
                self.cmd(5, 0x90 + n, 0, 1)?;
                self.cmd(5, 0x30, mem, n)?;
                self.cmd(5, 0x32, address, n)?;
            }
            self.cmd(7, 0, 0xd2, 0)?;
            let mut data = Vec::with_capacity(size);
            for _ in 0..size / 128 {
                self.wait(&[0xd8], None)?;
                data.extend(self.transfer(5, 0x70, 0, 0, 128, true)?);
            }
            Ok(data)
        })();
        let cleanup = self.stop();
        result.and_then(|v| cleanup.map(|_| v))
    }
}

/// Decode a bounded USB string descriptor, accepting only its declared UTF-16 bytes.
pub fn decode_string(raw: &[u8]) -> Result<String> {
    if raw.len() < 2
        || raw[1] != 3
        || raw[0] < 2
        || raw[0] as usize > raw.len()
        || !raw[0].is_multiple_of(2)
    {
        return Err(usb_error(format!(
            "Invalid USB identity descriptor: {}",
            hex::encode(raw)
        )));
    }
    let units: Vec<u16> = raw[2..raw[0] as usize]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16(&units).map_err(usb_error)
}
