//! Reader selection and capabilities, independent of cartridge board profiles.
use crate::{
    gb::{self, RomReader},
    gba, gbxcart, inl_gba, operator,
    storage::Cancel,
    usb::Bus,
    Error, Result,
};
use nusb::MaybeFuture;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Auto,
    Inlretro,
    Gbxcart,
    Operator,
}
impl Kind {
    pub const ALL: [Self; 4] = [Self::Auto, Self::Inlretro, Self::Gbxcart, Self::Operator];
    pub fn id(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Inlretro => "inlretro",
            Self::Gbxcart => "gbxcart",
            Self::Operator => "operator",
        }
    }
}
impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Auto => "Automatic",
            Self::Inlretro => "INLretro",
            Self::Gbxcart => "GBxCart RW",
            Self::Operator => "GB Operator",
        })
    }
}
pub fn unsupported(action: &str) -> Error {
    Error::new("READER_OPERATION_UNSUPPORTED",format!("The selected reader does not support {action} with this profile."),"For Game Boy writing/wiping, use a physically matched and qualified flash profile with INLretro or GBxCart RW. Automatic mode and GB Operator support ROM reading and verification only. Save-memory access is not implemented.")
}
pub fn check(kind: Kind, platform: &str, profile: &str, action: &str) -> Result<()> {
    if platform == "gba"
        && (profile != "auto" || !["probe", "read", "backup", "verify"].contains(&action))
    {
        return Err(Error::new("GBA_OPERATION_UNSUPPORTED", "GBA supports ROM detection, reading, backup and verification through INLretro, GBxCart RW and GB Operator.", "Choose a supported reader and the Automatic profile. GBA ROM writing, erasing and save-memory access are not supported."));
    }
    if kind == Kind::Gbxcart && action != "doctor" {
        let read = ["gameboy", "gba"].contains(&platform)
            && profile == "auto"
            && ["probe", "read", "backup", "verify"].contains(&action);
        let flash = platform == "gameboy"
            && profile == crate::rom::GB_PROFILE
            && [
                "probe", "read", "backup", "verify", "check", "write", "wipe",
            ]
            .contains(&action);
        if !read && !flash {
            return Err(unsupported(action));
        }
    }
    if kind == Kind::Operator && action != "doctor" {
        if platform == "gameboy" && ["write", "wipe"].contains(&action) {
            return Err(crate::operator_programming::unavailable());
        }
        let read = ["gameboy", "gba"].contains(&platform)
            && profile == "auto"
            && ["probe", "read", "backup", "verify"].contains(&action);
        if !read {
            return Err(unsupported(action));
        }
    }
    Ok(())
}
pub enum Device {
    Inlretro(Bus),
    Gbxcart(gbxcart::Reader),
    Operator(operator::Reader),
}
impl Device {
    pub fn kind(&self) -> Kind {
        match self {
            Self::Inlretro(_) => Kind::Inlretro,
            Self::Gbxcart(_) => Kind::Gbxcart,
            Self::Operator(_) => Kind::Operator,
        }
    }
    pub fn identity(&self) -> Value {
        match self {
            Self::Inlretro(bus) => {
                let mut v = bus.identity();
                v["driver"] = json!("inlretro");
                v["name"] = json!("INLretro");
                v["firmware"] = v["firmware_usb"].clone();
                v
            }
            Self::Gbxcart(r) => r.identity(),
            Self::Operator(r) => r.identity(),
        }
    }
    pub fn gameboy(self) -> Box<dyn RomReader> {
        match self {
            Self::Inlretro(b) => Box::new(gb::Reader::new(b)),
            Self::Gbxcart(r) => Box::new(r),
            Self::Operator(r) => Box::new(r),
        }
    }
    pub fn gba(self) -> Result<Box<dyn gba::RomReader>> {
        match self {
            Self::Inlretro(bus) => Ok(Box::new(inl_gba::Reader::new(bus))),
            Self::Gbxcart(r) => Ok(Box::new(r)),
            Self::Operator(r) => Ok(Box::new(r)),
        }
    }
    pub fn gameboy_flash(self) -> Box<dyn gb::FlashWriter> {
        match self {
            Self::Inlretro(b) => Box::new(gb::Reader::new(b)),
            Self::Gbxcart(r) => Box::new(r),
            Self::Operator(r) => Box::new(r),
        }
    }
    pub fn inlretro(self) -> Result<Bus> {
        match self {
            Self::Inlretro(b) => Ok(b),
            _ => Err(unsupported("this operation")),
        }
    }
}
pub fn open(kind: Kind, port: Option<&str>, cancel: Cancel) -> Result<Device> {
    cancel.check()?;
    if port.is_some() && kind == Kind::Inlretro {
        return Err(Error::new(
            "READER_PORT_CONFLICT",
            "A serial port was selected with INLretro.",
            "Select GBxCart RW or Automatic when using --port, or remove the serial-port override.",
        ));
    }
    if kind == Kind::Inlretro {
        return Bus::open(cancel).map(Device::Inlretro);
    }
    if kind == Kind::Operator {
        if port.is_some() {
            return Err(Error::new(
                "READER_PORT_CONFLICT",
                "A serial port was selected with GB Operator.",
                "Remove the serial-port override. GB Operator uses its USB identity directly.",
            ));
        }
        return operator::Reader::open(cancel).map(Device::Operator);
    }
    if let Some(p) = port {
        return gbxcart::Reader::open(p, cancel).map(Device::Gbxcart);
    }
    let ports = serialport::available_ports().map_err(|e| Error::new("SERIAL_DISCOVERY_FAILED","Serial readers could not be listed.","Reconnect the reader, or select GBxCart RW with an explicit serial port using the CLI.").details(json!({"reason":e.to_string()})))?;
    let ports:Vec<_> = ports.into_iter().filter(|p| matches!(&p.port_type,serialport::SerialPortType::UsbPort(u) if u.vid==0x1a86 && u.pid==0x7523)).collect();
    let (inl, operator) = if kind == Kind::Auto {
        let devices = nusb::list_devices()
            .wait()
            .map_err(|e| {
                Error::new(
                    "USB_DISCOVERY_FAILED",
                    "USB readers could not be listed.",
                    "Reconnect the reader and check your USB permissions.",
                )
                .details(json!({"reason":e.to_string()}))
            })?
            .collect::<Vec<_>>();
        (
            devices
                .iter()
                .filter(|d| d.vendor_id() == 0x16c0 && d.product_id() == 0x05dc)
                .count(),
            devices
                .iter()
                .filter(|d| d.vendor_id() == 0x16d0 && d.product_id() == 0x123d)
                .count(),
        )
    } else {
        (0, 0)
    };
    let count = inl + operator + ports.len();
    if count == 0 {
        return Err(Error::new("READER_DISCONNECTED","No supported reader was found.","Connect an INLretro, GBxCart RW or GB Operator with a data USB cable. For GBxCart on a different serial bridge, choose its port with --reader gbxcart --port PATH.").exit(3));
    }
    if count != 1 {
        return Err(Error::new("READER_AMBIGUOUS","More than one possible reader is connected.","Choose the connected reader in Preferences. If multiple serial devices remain, disconnect the others or use --reader gbxcart --port PATH. A CH340 serial chip alone does not identify a GBxCart.").details(json!({"inlretro_candidates":inl,"operator_candidates":operator,"serial_candidates":ports.iter().map(|p|&p.port_name).collect::<Vec<_>>()})).exit(3));
    }
    if inl == 1 {
        Bus::open(cancel).map(Device::Inlretro)
    } else if operator == 1 {
        operator::Reader::open(cancel).map(Device::Operator)
    } else {
        gbxcart::Reader::open(&ports[0].port_name, cancel).map(Device::Gbxcart)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capabilities_allow_only_the_supported_gb_flash_profile() {
        for slot in ["gameboy", "gba", "nes", "famicom"] {
            for profile in ["auto", crate::rom::GB_PROFILE, "broke-unrom512"] {
                for action in [
                    "probe", "read", "backup", "verify", "write", "wipe", "check",
                ] {
                    assert_eq!(
                        check(Kind::Gbxcart, slot, profile, action).is_ok(),
                        (slot == "gameboy" && profile == crate::rom::GB_PROFILE)
                            || (["gameboy", "gba"].contains(&slot)
                                && profile == "auto"
                                && ["probe", "read", "backup", "verify"].contains(&action))
                    );
                }
            }
        }
    }
    #[test]
    fn inlretro_allows_only_read_only_gba_operations() {
        for profile in ["auto", crate::rom::GB_PROFILE, "broke-unrom512"] {
            for action in [
                "probe", "read", "backup", "verify", "write", "wipe", "check",
            ] {
                assert_eq!(
                    check(Kind::Inlretro, "gba", profile, action).is_ok(),
                    profile == "auto" && ["probe", "read", "backup", "verify"].contains(&action)
                );
            }
        }
    }
    #[test]
    fn operator_allows_only_read_only_automatic_gameboy_and_gba_operations() {
        for slot in ["gameboy", "gba", "nes", "famicom"] {
            for profile in ["auto", crate::rom::GB_PROFILE, "broke-unrom512"] {
                for action in [
                    "probe", "read", "backup", "verify", "write", "wipe", "check",
                ] {
                    assert_eq!(
                        check(Kind::Operator, slot, profile, action).is_ok(),
                        ["gameboy", "gba"].contains(&slot)
                            && profile == "auto"
                            && ["probe", "read", "backup", "verify"].contains(&action)
                    );
                }
            }
        }
    }
}
