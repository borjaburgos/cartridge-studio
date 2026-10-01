use crate::{
    games,
    operations::{self, Journal},
    readers::{self, Device, Kind},
    rom::{self, BOARDS, GB_PROFILE},
    storage::{self, Cancel},
    usb::Bus,
    Error, Result,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
fn yes() -> bool {
    true
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub action: String,
    #[serde(default)]
    pub reader: Kind,
    #[serde(default)]
    pub port: Option<String>,
    #[serde(default)]
    pub platform: String,
    #[serde(default = "automatic")]
    pub profile: String,
    #[serde(default)]
    pub source: Option<PathBuf>,
    #[serde(default)]
    pub source_sha256: Option<String>,
    #[serde(default)]
    pub directory: Option<PathBuf>,
    #[serde(default)]
    pub data_directory: Option<PathBuf>,
    #[serde(default)]
    pub expected: String,
    #[serde(default = "yes")]
    pub double_read: bool,
    #[serde(default = "yes")]
    pub strict_checksum: bool,
    #[serde(default = "yes")]
    pub download_artwork: bool,
    #[serde(default)]
    pub confirmed: bool,
    #[serde(default)]
    pub gba_rom_bytes: Option<usize>,
}
fn automatic() -> String {
    "auto".into()
}
impl Request {
    pub fn new(action: &str) -> Self {
        Self {
            action: action.into(),
            reader: Kind::Auto,
            port: None,
            platform: String::new(),
            profile: automatic(),
            source: None,
            source_sha256: None,
            directory: None,
            data_directory: None,
            expected: String::new(),
            double_read: true,
            strict_checksum: true,
            download_artwork: true,
            confirmed: false,
            gba_rom_bytes: None,
        }
    }
    pub fn root(&self) -> PathBuf {
        self.data_directory
            .as_deref()
            .map(storage::expand)
            .unwrap_or_else(storage::data_root)
    }
    pub fn prepare(&mut self) {
        if ["read", "backup", "verify", "write", "wipe"].contains(&self.action.as_str())
            && self.directory.is_none()
        {
            self.directory = Some(self.root().join("cartridge-backups").join(format!(
                "{}-{}-{}-{}",
                self.platform,
                self.action,
                storage::stamp(),
                std::process::id()
            )));
        }
    }
}
fn source(r: &Request) -> Result<PathBuf> {
    r.source.as_deref().map(storage::expand).ok_or_else(|| {
        Error::new(
            "SOURCE_REQUIRED",
            "Load a ROM first.",
            "Choose Load ROM, then retry this operation.",
        )
    })
}
fn check(data: &[u8], path: &Path, platform: &str, profile: &str, write: bool) -> Result<Value> {
    let info = rom::inspect(path, data)?;
    if info["platform"] != platform {
        return Err(Error::new(
            "WRONG_PLATFORM",
            format!(
                "This is a {} ROM, but a different platform is selected.",
                info["format"].as_str().unwrap_or("different")
            ),
            "Select the matching platform and physical cartridge model. Nothing has been erased.",
        ));
    }
    if platform == "famicom" {
        rom::compatibility(
            &rom::NesRom::parse(data)?,
            rom::board(profile)?,
            None,
            write,
        )?;
    } else if write {
        if !rom::gb_flash_profile(profile) {
            return Err(Error::new("WRITE_PROFILE_REQUIRED","Automatic Game Boy detection supports reading only.","Select a supported flash profile only if it matches your physical board and reader. Original retail cartridges cannot be rewritten."));
        }
        rom::validate_gb_flash_profile(data, profile)?;
    }
    Ok(info)
}
pub fn run(request: &Request, cancel: Cancel, progress: &mut dyn FnMut(String)) -> Result<Value> {
    run_with_device(request, cancel, progress, &mut |cancel| {
        readers::open(request.reader, request.port.as_deref(), cancel)
    })
}
pub fn run_with(
    request: &Request,
    cancel: Cancel,
    progress: &mut dyn FnMut(String),
    open: &mut dyn FnMut(Cancel) -> Result<Bus>,
) -> Result<Value> {
    run_with_device(request, cancel, progress, &mut |c| {
        open(c).map(Device::Inlretro)
    })
}
fn run_with_device(
    request: &Request,
    cancel: Cancel,
    progress: &mut dyn FnMut(String),
    open: &mut dyn FnMut(Cancel) -> Result<Device>,
) -> Result<Value> {
    cancel.check()?;
    let mut req = request.clone();
    req.prepare();
    let r = &req;
    let root = r.root();
    let action = r.action.as_str();
    if action == "profiles" {
        return Ok(
            json!({"profiles":BOARDS,"gba_profiles":[{"id":"auto","name":"Game Boy Advance ROM · read-only","writable":false,"maximum_rom_bytes":crate::gba::MAX_SIZE}],"gameboy_profiles":[{"id":"auto","name":"Automatic · read/verify","writable":false},{"id":GB_PROFILE,"name":"Ferrante 512 · SST39SF040 AUDIO/MBC5","writable":true},{"id":rom::SPANSION_PROFILE,"name":"Spansion S29GL032M R4 · WR/MBC5 · 4 MiB","writable":true,"qualification":"read-erase-program-verified-pcb6-l14"}],"version":crate::VERSION}),
        );
    }
    if ["inspect", "game", "checksum"].contains(&action) {
        let path = source(r)?;
        let data = storage::read_rom(&path)?;
        let mut info = rom::inspect(&path, &data)?;
        if action == "checksum" {
            return rom::match_hash(&info, &r.expected);
        }
        info["game"] = games::lookup(
            &path,
            &data,
            info["platform"].as_str().unwrap(),
            &root,
            action == "game",
            action == "game",
            &cancel,
        );
        return Ok(info);
    }
    if action == "doctor" {
        let bus = open(cancel)?;
        return Ok(
            json!({"device":bus.identity(),"message":"Reader connected","backend":"rust","version":crate::VERSION}),
        );
    }
    // NES and Famicom are distinct connectors sharing the NES bus commands and
    // ROM family. Preserve the caller's physical slot in reports and preflight.
    let slot = r.platform.as_str();
    let platform = if slot == "nes" { "famicom" } else { slot };
    let profile = r.profile.as_str();
    if platform != "gameboy" && platform != "famicom" && platform != "gba" {
        return Err(Error::new(
            "PLATFORM_REQUIRED",
            "Select a physical cartridge slot.",
            "Choose NES (72-pin), Famicom (60-pin), Game Boy / Color (32-pin), or Game Boy Advance (32-pin GBA connector).",
        ));
    }
    readers::check(r.reader, platform, profile, action)?;
    if let Some(n) = r.gba_rom_bytes {
        if platform != "gba" {
            return Err(Error::check(
                "A GBA ROM size applies only to Game Boy Advance.",
            ));
        }
        crate::gba::validate_size(n)?;
    }
    let auto = platform == "famicom" && (profile.is_empty() || profile == "auto");
    let board = if platform == "famicom" {
        if auto {
            if !["probe", "read", "backup", "verify"].contains(&action) {
                return Err(Error::new("BOARD_DETECTION_REQUIRED","Detect the cartridge before checking compatibility or reviewing a write.","Choose Automatic and press Detect. Supported boards enable write review; unknown boards remain blocked."));
            }
            BOARDS[0]
        } else {
            rom::board(profile)?
        }
    } else {
        if profile != "auto" && !rom::gb_flash_profile(profile) {
            return Err(Error::new("BOARD_PROFILE_REQUIRED","Choose a Game Boy cartridge profile.","Use automatic detection for reading, or the exact physical flash board for writing."));
        }
        BOARDS[0]
    };
    readers::check(r.reader, platform, profile, action)?;
    if action == "check" {
        let path = source(r)?;
        let data = storage::read_rom(&path)?;
        let info = check(&data, &path, platform, profile, true)?;
        return Ok(
            json!({"message":"File matches this board profile. Physical checks run again before erasing.","slot":slot,"rom":info}),
        );
    }
    if action == "probe" {
        let bus = open(cancel)?;
        readers::check(bus.kind(), platform, profile, action)?;
        let bus = bus.with_gameboy_profile(profile)?;
        let identity = bus.identity();
        let result = if platform == "gba" {
            let mut reader = bus.gba()?;
            let v = crate::gba::detect(reader.as_mut());
            let c = reader.close();
            operations::cleanup(v, c)
        } else if platform == "gameboy" {
            let mut reader = bus.gameboy();
            if profile == "auto" {
                let v = crate::gb::detect(reader.as_mut());
                let c = reader.close();
                operations::cleanup(v, c)
            } else {
                let v = (|| {
                    let flash = reader.identify_flash()?;
                    reader.initialize()?;
                    let info = rom::gb_header(&reader.header_bytes()?)?;
                    Ok(
                        json!({"device":reader.identity(),"cartridge":info,"flash":flash,"message":format!("{} · {}",info["title"].as_str().unwrap_or("Untitled"),info["mapper"].as_str().unwrap_or("Blank/invalid header"))}),
                    )
                })();
                let c = reader.close();
                operations::cleanup(v, c)
            }
        } else {
            let mut reader = crate::famicom::Reader::new(bus.inlretro()?, board);
            let v = if auto {
                reader.detect(progress)
            } else {
                reader.inspect().map(|mut v| {
                    v["message"] = json!(format!(
                        "{} · {} mirroring",
                        board.name,
                        v["mirroring"].as_str().unwrap_or("unknown")
                    ));
                    v
                })
            };
            let c = reader.close();
            operations::cleanup(v, c)
        };
        return result
            .map_err(|mut error| {
                error.details["device"] = identity;
                error.details["slot"] = json!(slot);
                error
            })
            .map(|mut value| {
                value["slot"] = json!(slot);
                value["slot_source"] = json!("user_selection");
                value
            });
    }
    if !["read", "backup", "verify", "write", "wipe"].contains(&action) {
        return Err(Error::new(
            "UNKNOWN_OPERATION",
            format!("Unknown operation: {action}"),
            "Restart the application and choose a supported operation.",
        ));
    }
    let writing = action == "write" || action == "wipe";
    if writing {
        if (platform == "gameboy" && !rom::gb_flash_profile(profile))
            || (platform == "famicom" && !board.writable)
        {
            return Err(Error::new(
                "BOARD_READ_ONLY",
                "This cartridge profile is read-only.",
                "Select a qualified flash board before erasing or writing.",
            ));
        }
        if !r.confirmed {
            return Err(Error::new("CONFIRMATION_REQUIRED","Erase/write requires confirmation.","Review the board and source in the confirmation dialog, or supply --yes to the explicit CLI write/wipe command."));
        }
    }
    // Read once, pin and validate before opening USB. The caller cannot substitute a path later.
    let data = if action == "write" || action == "verify" {
        let path = source(r)?;
        let bytes = storage::read_rom(&path)?;
        if r.source_sha256.as_deref() != Some(rom::sha(&bytes).as_str()) {
            return Err(Error::new(
                "SOURCE_CHANGED",
                "The ROM changed since it was loaded.",
                "Load it again and review the new checksums. Nothing has been erased.",
            ));
        }
        if auto {
            let info = rom::inspect(&path, &bytes)?;
            if info["platform"] != platform {
                return Err(Error::new(
                    "WRONG_PLATFORM",
                    "The loaded ROM belongs to a different platform.",
                    "Select its platform and retry.",
                ));
            }
            rom::compatibility(&rom::NesRom::parse(&bytes)?, board, None, false)?;
        } else {
            check(&bytes, &path, platform, profile, action == "write")?;
        }
        Some(bytes)
    } else {
        None
    };
    let nes = if platform == "famicom" {
        data.as_deref().map(rom::NesRom::parse).transpose()?
    } else {
        None
    };
    let directory = storage::expand(r.directory.as_ref().unwrap());
    storage::prepare_directory(&directory)?;
    cancel.check()?;
    let mut journal = Journal::new(&directory, platform, action)?;
    journal.report["slot"] = json!(slot);
    journal.report["slot_source"] = json!("user_selection");
    journal.save()?;
    // Retain a source snapshot before hardware access, including failure to open USB.
    if let Some(d) = data.as_deref() {
        journal.put(
            if platform == "gba" {
                "reviewed-source.gba"
            } else if platform == "gameboy" {
                "reviewed-source.gb"
            } else {
                "reviewed-source.nes"
            },
            d,
        )?;
    }
    let bus = match open(cancel.clone()) {
        Ok(b) => b,
        Err(e) => return journal.finish(Err(e)),
    };
    if let Err(e) = readers::check(bus.kind(), platform, profile, action) {
        return journal.finish(Err(e));
    }
    let bus = bus.with_gameboy_profile(profile)?;
    journal.report["device"] = bus.identity();
    let mut report = if platform == "famicom" {
        operations::nes_operation(
            bus.inlretro()?,
            &mut journal,
            action,
            board,
            auto,
            nes.as_ref(),
            if r.double_read { 2 } else { 1 },
            progress,
        )?
    } else if platform == "gba" {
        crate::gba::read_rom(
            bus.gba()?,
            &mut journal,
            action,
            data.as_deref(),
            if r.double_read { 2 } else { 1 },
            r.gba_rom_bytes,
            progress,
        )?
    } else if writing {
        operations::gb_write_with(bus.gameboy_flash(), &mut journal, data.as_deref(), progress)?
    } else {
        operations::gb_read_with(
            bus.gameboy(),
            &mut journal,
            action,
            profile == rom::SPANSION_PROFILE
                || (profile == GB_PROFILE && ["backup", "verify"].contains(&action)),
            data.as_deref(),
            if r.double_read { 2 } else { 1 },
            r.strict_checksum,
            progress,
        )?
    };
    games::after_read(&mut report, &root, r.download_artwork, &cancel, progress);
    if ["read", "backup"].contains(&action) {
        if let Some(output) = report["output"].as_str() {
            if let Ok(bytes) = storage::read_rom(Path::new(output)) {
                if let Ok(mut info) = rom::inspect(Path::new(output), &bytes) {
                    info["game"] = report["game"].clone();
                    report["rom"] = info;
                }
            }
        }
    }
    Ok(report)
}
