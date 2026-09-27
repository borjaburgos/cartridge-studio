use crate::{
    files,
    worker::{Event, Job},
    Error, Result,
};
use cartridge_core::{
    readers::{self, Kind},
    rom::{BOARDS, GB_PROFILE},
    service::Request,
    storage,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    path::{Path, PathBuf},
};

pub const DESKTOP_MIN: (u32, u32) = (1000, 700);
pub const TERMINAL_MIN: (u16, u16) = (120, 32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Platform {
    Nes,
    Famicom,
    #[default]
    GameBoy,
    Gba,
}
impl Platform {
    pub const ALL: [Self; 4] = [Self::Nes, Self::Famicom, Self::GameBoy, Self::Gba];
    pub fn id(self) -> &'static str {
        match self {
            Self::Nes => "nes",
            Self::Famicom => "famicom",
            Self::GameBoy => "gameboy",
            Self::Gba => "gba",
        }
    }
    pub fn family(self) -> &'static str {
        match self {
            Self::Nes | Self::Famicom => "famicom",
            Self::GameBoy => "gameboy",
            Self::Gba => "gba",
        }
    }
    pub fn connector(self) -> &'static str {
        match self {
            Self::Nes => "NES · 72-pin slot",
            Self::Famicom => "Famicom · 60-pin slot",
            Self::GameBoy => "Game Boy / Color · 32-pin slot",
            Self::Gba => "Game Boy Advance · GBA connector",
        }
    }
    pub fn profiles(self) -> Vec<Profile> {
        let mut out = vec![Profile {
            id: "auto",
            name: "Automatic detection",
            writable: false,
        }];
        match self {
            Self::Gba => (),
            Self::GameBoy => out.push(Profile {
                id: GB_PROFILE,
                name: "Ferrante 512 · SST39SF040 AUDIO/MBC5",
                writable: true,
            }),
            Self::Nes | Self::Famicom => out.extend(BOARDS.iter().map(|b| Profile {
                id: b.id,
                name: b.name,
                writable: b.writable,
            })),
        }
        out
    }
}
impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Nes => "NES",
            Self::Famicom => "Famicom",
            Self::GameBoy => "Game Boy / Color",
            Self::Gba => "Game Boy Advance",
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Profile {
    pub id: &'static str,
    pub name: &'static str,
    pub writable: bool,
}
impl std::fmt::Display for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GbaSize {
    #[default]
    Auto,
    K256,
    K512,
    M1,
    M2,
    M4,
    M8,
    M16,
    M32,
}
impl GbaSize {
    pub const ALL: [Self; 9] = [
        Self::Auto,
        Self::K256,
        Self::K512,
        Self::M1,
        Self::M2,
        Self::M4,
        Self::M8,
        Self::M16,
        Self::M32,
    ];
    pub fn bytes(self) -> Option<usize> {
        Self::ALL
            .iter()
            .position(|&s| s == self)
            .filter(|&n| n > 0)
            .map(|n| 1usize << (17 + n))
    }
}
impl std::fmt::Display for GbaSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.bytes() {
            None => f.write_str("Automatic"),
            Some(n) if n < 1048576 => write!(f, "{} KiB", n / 1024),
            Some(n) => write!(f, "{} MiB", n / 1048576),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub reader: Kind,
    pub double_read: bool,
    pub strict_checksum: bool,
    pub gba_size: GbaSize,
    pub download_artwork: bool,
    pub data_directory: PathBuf,
    pub color_scheme: u8,
    pub tui_last_folder: PathBuf,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}
impl Default for Settings {
    fn default() -> Self {
        let root = storage::data_root();
        Self {
            reader: Kind::Auto,
            double_read: true,
            strict_checksum: true,
            gba_size: GbaSize::Auto,
            download_artwork: true,
            data_directory: root.clone(),
            color_scheme: 1,
            tui_last_folder: root,
            extra: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct Source {
    pub path: PathBuf,
    pub info: Value,
}
impl Source {
    pub fn hash(&self) -> String {
        value_text(&self.info["hashes"]["sha256"])
    }
    pub fn title(&self) -> String {
        self.info["game"]["game"]["name"]
            .as_str()
            .or(self.info["title"].as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                self.path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into()
            })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Workspace,
    History,
    Support,
    Settings,
}

#[derive(Clone, Debug)]
pub struct Review {
    request: Request,
    pub word: String,
}
impl Review {
    pub fn action(&self) -> &str {
        &self.request.action
    }
    pub fn text(&self) -> String {
        let r = &self.request;
        format!("{} cartridge\n\nReader: {}\nPhysical slot: {}\nPhysical profile: {}\nSource: {}\nSHA-256: {}\nBackup library: {}\n\nTwo complete backups must agree before erasing. Writing verifies each bank and two final readbacks. Wiping verifies the entire chip is blank.\n\nKeep USB connected until the operation finishes. Type {} to confirm.", if r.action == "write" { "Write" } else { "Wipe" }, r.reader, Platform::ALL.into_iter().find(|p| p.id() == r.platform).map(|p| p.connector()).unwrap_or("Unknown slot"), r.profile, r.source.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "No source needed for wipe".into()), r.source_sha256.as_deref().unwrap_or("Not applicable"), r.root().display(), r.action.to_uppercase())
    }
    pub fn ready(&self) -> bool {
        self.word == self.request.action.to_uppercase()
    }
}
struct Operation {
    job: Job,
    request: Request,
    review_after: bool,
}

pub struct App {
    pub platform: Platform,
    pub profile: Profile,
    pub settings: Settings,
    settings_path: PathBuf,
    settings_valid: bool,
    pub page: Page,
    pub connected: bool,
    pub no_device: bool,
    pub reader: String,
    pub connected_reader: Option<Kind>,
    pub detection: Option<Value>,
    pub source: Option<Source>,
    pub review: Option<Review>,
    operation: Option<Operation>,
    pub status: String,
    pub error: Option<Error>,
    pub log: VecDeque<String>,
    pub directory: Option<PathBuf>,
    pub last_result: Option<Value>,
    pub history: Vec<files::History>,
}
impl App {
    pub fn new(library: Option<PathBuf>, no_device: bool) -> Self {
        let root = library
            .as_deref()
            .map(storage::expand)
            .unwrap_or_else(storage::data_root);
        let settings_path = root.join("studio/settings.json");
        let (mut settings, error) = if settings_path.exists() {
            match files::json_file(&settings_path).and_then(|v| serde_json::from_value::<Settings>(v).map_err(Error::from)) {
                Ok(s) => (s, None),
                Err(e) => (Settings::default(), Some(Error::new("SETTINGS_UNREADABLE", "Saved preferences could not be loaded. Safe defaults are in use.", "Keep or repair studio/settings.json in your library, then restart. The existing file has not been replaced.").details(json!({"reason":e.to_string()})))),
            }
        } else {
            (Settings::default(), None)
        };
        if library.is_some() || !settings_path.exists() {
            settings.data_directory = root.clone();
        }
        if !settings.tui_last_folder.is_dir() {
            settings.tui_last_folder = root;
        }
        Self {
            platform: Platform::GameBoy,
            profile: Platform::GameBoy.profiles()[0],
            settings,
            settings_path,
            settings_valid: error.is_none(),
            page: Page::Workspace,
            connected: false,
            no_device,
            reader: "Reader not checked".into(),
            connected_reader: None,
            detection: None,
            source: None,
            review: None,
            operation: None,
            status: "Load a ROM or check the USB reader to begin.".into(),
            error,
            log: VecDeque::new(),
            directory: None,
            last_result: None,
            history: vec![],
        }
    }
    pub fn busy(&self) -> bool {
        self.operation.is_some()
    }
    pub fn stopping(&self) -> bool {
        self.operation.as_ref().is_some_and(|o| o.job.stopping)
    }
    pub fn action(&self) -> &str {
        self.operation
            .as_ref()
            .map(|o| o.request.action.as_str())
            .unwrap_or("")
    }
    fn idle(&self) -> Result<()> {
        if self.busy() {
            Err(Error::new("OPERATION_BUSY", "An operation is still running.", "Wait for it to finish, or request Stop and allow the cartridge service to close safely."))
        } else {
            Ok(())
        }
    }
    pub fn set_reader(&mut self, reader: Kind) -> Result<()> {
        self.idle()?;
        self.settings.reader = reader;
        self.connected = false;
        self.connected_reader = None;
        self.reader = "Reader not checked".into();
        self.detection = None;
        self.review = None;
        self.status = "Reader changed. Choose Check USB to connect.".into();
        self.save_settings()
    }
    fn effective_reader(&self) -> Kind {
        if self.settings.reader == Kind::Auto {
            self.connected_reader.unwrap_or(Kind::Auto)
        } else {
            self.settings.reader
        }
    }
    pub fn reader_note(&self) -> &'static str {
        if self.platform == Platform::Gba {
            return "GBA uses INLretro, GBxCart RW or GB Operator at 3.3 V. Read, backup and verify ROM only; saved games are excluded. Size overrides are in Preferences.";
        }
        if self.effective_reader() == Kind::Gbxcart {
            "GBxCart RW: ROM read, backup and verify. GB write/wipe requires the Ferrante 512 profile and PCB 6 / L14 firmware. Save memory is not supported."
        } else if self.effective_reader() == Kind::Operator {
            "GB Operator: native GB / Color and GBA ROM detection, read, backup and verification. Close Playback before connecting. Save memory and ROM programming are not enabled."
        } else {
            "Reader selection and cartridge board detection are separate. Only a supported physical flash board enables writing."
        }
    }
    pub fn set_platform(&mut self, platform: Platform) -> Result<()> {
        self.idle()?;
        self.platform = platform;
        self.profile = platform.profiles()[0];
        self.detection = None;
        self.review = None;
        self.page = Page::Workspace;
        Ok(())
    }
    pub fn set_profile(&mut self, profile: Profile) -> Result<()> {
        self.idle()?;
        if !self.platform.profiles().contains(&profile) {
            return Err(Error::check("Choose a profile for the selected platform."));
        }
        self.profile = profile;
        self.detection = None;
        self.review = None;
        Ok(())
    }
    pub fn effective_profile(&self) -> &str {
        if self.profile.id == "auto" {
            self.detection
                .as_ref()
                .and_then(|v| v["profile"].as_str())
                .unwrap_or("auto")
        } else {
            self.profile.id
        }
    }
    pub fn writable(&self) -> bool {
        readers::check(
            self.effective_reader(),
            self.platform.family(),
            self.effective_profile(),
            "write",
        )
        .is_ok()
            && self
                .platform
                .profiles()
                .iter()
                .any(|p| p.id == self.effective_profile() && p.writable)
    }
    pub fn write_note(&self) -> &'static str {
        if self.busy() {
            "Cartridge controls are unavailable while an operation is running."
        } else if self.platform == Platform::Gba {
            "GBA write and wipe are not implemented. ROM reading and verification are available."
        } else if self.effective_reader() == Kind::Gbxcart && self.platform != Platform::GameBoy {
            "GBxCart does not support this slot. Choose Game Boy / Color or Game Boy Advance."
        } else if self.effective_reader() == Kind::Operator && self.platform != Platform::GameBoy {
            "GB Operator supports Game Boy / Color and Game Boy Advance ROM reading. Choose one of those cartridge families."
        } else if self.platform == Platform::GameBoy && !self.writable() {
            "Write/wipe: select Ferrante 512 only if it matches your cartridge. Automatic mode is read-only."
        } else if !self.writable() {
            "Write/wipe requires a supported flash board. Detect the board or select its exact profile."
        } else if !self.connected || self.no_device {
            "Choose Check USB to connect the reader before writing or wiping."
        } else if self.source.is_none() {
            "Load a compatible ROM to enable Write. Wipe erases the complete chip after two verified backups."
        } else {
            "Write and Wipe require review. Both retain two full-chip backups and verify the result."
        }
    }
    pub fn can(&self, action: &str) -> bool {
        if self.busy() || self.review.is_some() {
            return false;
        }
        if [
            "probe", "read", "backup", "verify", "check", "write", "wipe",
        ]
        .contains(&action)
            && readers::check(
                self.effective_reader(),
                self.platform.family(),
                self.effective_profile(),
                action,
            )
            .is_err()
        {
            return false;
        }
        match action {
            "doctor" => !self.no_device,
            "probe" | "read" | "backup" => self.connected && !self.no_device,
            "verify" => self.connected && !self.no_device && self.source.is_some(),
            "check" => self.source.is_some() && self.writable(),
            "write" => {
                self.connected && !self.no_device && self.source.is_some() && self.writable()
            }
            "wipe" => self.connected && !self.no_device && self.writable(),
            "checksum" | "game" => self.source.is_some(),
            "inspect" => true,
            _ => false,
        }
    }
    pub fn unavailable(&self, action: &str) -> Error {
        if self.busy() {
            return self.idle().unwrap_err();
        }
        if self.review.is_some() {
            return Error::new(
                "REVIEW_OPEN",
                "Finish or cancel the current review first.",
                "Cancel the review to return to the workspace.",
            );
        }
        if let Err(e) = readers::check(
            self.effective_reader(),
            self.platform.family(),
            self.effective_profile(),
            action,
        ) {
            if [
                "probe", "read", "backup", "verify", "check", "write", "wipe",
            ]
            .contains(&action)
            {
                return e;
            }
        }
        if ["write", "wipe", "check"].contains(&action) && !self.writable() {
            return Error::new("BOARD_REQUIRES_CHECK", "A supported flash board must be identified before writing or wiping.", "Use Detect for supported NES or Famicom boards, or select the exact Game Boy flash profile. Retail ROM cartridges cannot be rewritten.");
        }
        if ["write", "verify", "check", "checksum", "game"].contains(&action)
            && self.source.is_none()
        {
            return Error::new(
                "SOURCE_REQUIRED",
                "Load a ROM first.",
                "Choose Load ROM and select a .gb, .gbc, .gba or .nes file.",
            );
        }
        Error::new("READER_UNAVAILABLE", "The reader is not ready.", "Connect your selected reader with one cartridge, then choose Check USB. Unplug USB before changing cartridges.")
    }
    fn request(&self, action: &str) -> Request {
        let mut r = Request::new(action);
        r.reader = if action == "doctor" {
            self.settings.reader
        } else {
            self.effective_reader()
        };
        r.platform = self.platform.id().into();
        r.profile = if ["check", "write", "wipe"].contains(&action) {
            self.effective_profile()
        } else {
            self.profile.id
        }
        .into();
        if ["check", "write", "verify", "checksum", "game"].contains(&action) {
            r.source = self.source.as_ref().map(|s| s.path.clone());
            r.source_sha256 = self.source.as_ref().map(Source::hash);
        }
        r.data_directory = Some(self.settings.data_directory.clone());
        r.double_read = self.settings.double_read;
        r.strict_checksum = self.settings.strict_checksum;
        r.download_artwork = self.settings.download_artwork;
        if self.platform == Platform::Gba {
            r.gba_rom_bytes = self.settings.gba_size.bytes();
        }
        r
    }
    pub fn load(&mut self, path: &Path) -> Result<()> {
        self.idle()?;
        let path = storage::expand(path);
        self.review = None;
        self.source = None;
        let mut request = self.request("inspect");
        request.source = Some(path.clone());
        request.source_sha256 = None;
        self.settings.tui_last_folder = path
            .parent()
            .unwrap_or(&self.settings.data_directory)
            .into();
        self.start(request, false)
    }
    pub fn begin(&mut self, action: &str) -> Result<()> {
        if !self.can(action) {
            return Err(self.unavailable(action));
        }
        let mut r = self.request(action);
        match action {
            "wipe" => {
                self.review = Some(Review {
                    request: r,
                    word: String::new(),
                });
                Ok(())
            }
            "write" => {
                r.action = "check".into();
                self.start(r, true)
            }
            _ => self.start(r, false),
        }
    }
    pub fn checksum(&mut self, expected: &str) -> Result<()> {
        if !self.can("checksum") {
            return Err(self.unavailable("checksum"));
        }
        let mut r = self.request("checksum");
        r.expected = expected.into();
        self.start(r, false)
    }
    pub fn approve(&mut self) -> Result<()> {
        self.idle()?;
        let review = self
            .review
            .as_ref()
            .ok_or_else(|| Error::check("Open a write or wipe review first."))?;
        if !review.ready() {
            return Err(Error::new(
                "CONFIRMATION_REQUIRED",
                "The confirmation word does not match.",
                "Type the exact uppercase word shown in the review, or cancel.",
            ));
        }
        let mut r = self.review.take().unwrap().request;
        r.confirmed = true;
        self.start(r, false)
    }
    fn start(&mut self, request: Request, review_after: bool) -> Result<()> {
        self.idle()?;
        let job = Job::start(&request)?;
        self.status = format!("{} in progress…", action_name(&request.action));
        self.error = None;
        self.directory = None;
        self.append(format!(
            "{} · {} · {}",
            action_name(&request.action),
            request.platform,
            request.profile
        ));
        self.operation = Some(Operation {
            job,
            request,
            review_after,
        });
        Ok(())
    }
    pub fn stop(&mut self) {
        if let Some(o) = self.operation.as_mut() {
            o.job.stop();
            self.status =
                "Stopping safely… Keep USB connected while the service finishes cleanup.".into();
        }
    }
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        loop {
            let event = self.operation.as_ref().and_then(|o| o.job.poll());
            match event {
                None => break,
                Some(Event::Directory(p)) => {
                    self.directory = Some(p);
                    changed = true;
                }
                Some(Event::Progress(p)) => {
                    if !self.stopping() {
                        self.status = p.clone();
                    }
                    self.append(p);
                    changed = true;
                }
                Some(Event::Finished(result)) => {
                    let operation = self.operation.take().unwrap();
                    let request = operation.request.clone();
                    let review = operation.review_after;
                    drop(operation);
                    self.finish(&request, review, result);
                    changed = true;
                    break;
                }
            }
        }
        changed
    }
    fn finish(&mut self, request: &Request, review_after: bool, result: Result<Value>) {
        let value = match result {
            Ok(v) => v,
            Err(e) => {
                if [
                    "doctor", "probe", "read", "backup", "verify", "write", "wipe",
                ]
                .contains(&request.action.as_str())
                {
                    self.detection = None;
                    if request.action == "doctor"
                        || [
                            "READER_DISCONNECTED",
                            "USB_PERMISSION_DENIED",
                            "READER_COUNT",
                            "READER_AMBIGUOUS",
                            "SERIAL_PERMISSION_DENIED",
                            "SERIAL_IO",
                            "SERIAL_OPEN_FAILED",
                            "SERIAL_TIMEOUT",
                            "GBXCART_FIRMWARE_UNSUPPORTED",
                            "OPERATOR_USB_IO",
                            "OPERATOR_USB_TIMEOUT",
                            "OPERATOR_COMMAND_REJECTED",
                            "OPERATOR_COUNT",
                            "OPERATOR_IDENTITY_MISMATCH",
                        ]
                        .contains(&e.code.as_str())
                    {
                        self.connected = false;
                        self.connected_reader = None;
                        self.reader = "Reader unavailable".into();
                    }
                }
                self.fail(e);
                return;
            }
        };
        self.status = value["warning"]
            .as_str()
            .or(value["game_warning"].as_str())
            .or(value["message"].as_str())
            .unwrap_or("Operation complete. Backups and the verification report are retained.")
            .into();
        if let Some(device) = value.get("device") {
            self.connected = true;
            self.connected_reader = Some(match device["driver"].as_str() {
                Some("gbxcart") => Kind::Gbxcart,
                Some("operator") => Kind::Operator,
                _ => Kind::Inlretro,
            });
            self.reader = format!(
                "{} · {}",
                self.connected_reader.unwrap(),
                value_text(
                    device
                        .get("firmware")
                        .or_else(|| device.get("firmware_usb"))
                        .or_else(|| device.get("device_version"))
                        .unwrap_or(&Value::Null)
                )
            );
        }
        if let Some(detection) = value.get("detection") {
            self.detection = Some(detection.clone());
        }
        if ["inspect", "game"].contains(&request.action.as_str()) {
            self.set_source(value.clone(), request.source.clone());
        }
        if ["read", "backup"].contains(&request.action.as_str()) {
            if let Some(info) = value.get("rom") {
                self.set_source(info.clone(), value["output"].as_str().map(PathBuf::from));
            }
        }
        if let Some(directory) = value["directory"].as_str() {
            self.directory = Some(directory.into());
        }
        if request.action == "wipe" {
            self.detection = None;
        }
        if review_after {
            let mut r = request.clone();
            r.action = "write".into();
            self.review = Some(Review {
                request: r,
                word: String::new(),
            });
        }
        self.append(serde_json::to_string_pretty(&value).unwrap_or_default());
        self.last_result = Some(value);
    }
    fn set_source(&mut self, info: Value, fallback: Option<PathBuf>) {
        let path = info["path"].as_str().map(PathBuf::from).or(fallback);
        // A ROM format cannot identify the physical connector occupied by a cartridge.
        // In particular, .nes files work with both NES and Famicom board profiles.
        if info["platform"].as_str() != Some(self.platform.family()) {
            self.status =
                "ROM loaded. Select the cartridge's physical slot before reading or writing."
                    .into();
        }
        self.source = path.map(|path| Source { path, info });
    }
    pub fn fail(&mut self, e: Error) {
        self.status = if e.code == "INTERRUPTED" {
            "Operation stopped"
        } else {
            "Action could not be completed"
        }
        .into();
        if let Some(path) = e.details["backup_directory"].as_str() {
            self.directory = Some(path.into());
        }
        self.append(e.to_string());
        self.error = Some(e);
    }
    fn append(&mut self, text: String) {
        for line in files::plain(&text).lines() {
            self.log.push_back(line.chars().take(4096).collect());
        }
        while self.log.len() > 1000 {
            self.log.pop_front();
        }
    }
    pub fn save_settings(&mut self) -> Result<()> {
        self.idle()?;
        self.review = None;
        if !self.settings_valid {
            return Err(Error::new(
                "SETTINGS_UNREADABLE",
                "The existing preference file could not be read.",
                "Repair or move studio/settings.json, then restart. It has not been overwritten.",
            ));
        }
        fs::create_dir_all(self.settings_path.parent().unwrap())?;
        storage::atomic(
            &self.settings_path,
            &serde_json::to_vec_pretty(&self.settings)?,
        )
    }
    pub fn refresh_history(&mut self) -> Result<()> {
        self.history = files::history(&self.settings.data_directory)?;
        Ok(())
    }
    pub fn hashes(&self) -> String {
        self.source.as_ref().map(|s| format!("File: {}\n\nSHA-256\n{}\n\nSHA-1\n{}\n\nCRC32\n{}\n\nExpected hashes cover the complete file, including a NES header.", s.path.display(), value_text(&s.info["hashes"]["sha256"]), value_text(&s.info["hashes"]["sha1"]), value_text(&s.info["hashes"]["crc32"]))).unwrap_or_else(|| "Load a ROM to see its checksums.".into())
    }
    pub fn export(&mut self, name: &str, content: &str) -> Result<PathBuf> {
        let folder = self.settings.data_directory.join("exports");
        fs::create_dir_all(&folder)?;
        let path = folder.join(format!("{name}-{}.txt", storage::stamp()));
        storage::write_new(&path, content.as_bytes())?;
        self.status = format!("Saved {}", path.display());
        Ok(path)
    }
    pub fn save_rom(&mut self, path: &Path) -> Result<()> {
        self.idle()?;
        let source = self
            .source
            .as_ref()
            .ok_or_else(|| self.unavailable("checksum"))?;
        let bytes = storage::read_rom(&source.path)?;
        if cartridge_core::rom::sha(&bytes) != source.hash() {
            return Err(Error::new(
                "SOURCE_CHANGED",
                "The loaded ROM changed on disk.",
                "Load it again and review its checksums before saving or writing it.",
            ));
        }
        let path = storage::expand(path);
        storage::write_new(&path, &bytes)?;
        self.status = format!("ROM copy saved: {}", path.display());
        Ok(())
    }
    pub fn board_text(&self) -> String {
        self.detection
            .as_ref()
            .map(|v| {
                format!(
                    "{}\n{}",
                    value_text(&v["summary"]),
                    value_text(&v["limitation"])
                )
            })
            .unwrap_or_else(|| {
                "Choose the physical slot and press Detect. Unplug USB before changing cartridges; connect only one cartridge."
                    .into()
            })
    }
    pub fn game_text(&self) -> String {
        let Some(s) = &self.source else {
            return "Read a cartridge or load a ROM to identify a known game.".into();
        };
        let g = &s.info["game"];
        if g["status"] != "identified" {
            return g["message"].as_str().unwrap_or("No exact catalog match. The ROM remains available for inspection and verification.").into();
        }
        let details = ["releaseyear", "developer", "publisher", "genre", "users"]
            .iter()
            .filter_map(|k| {
                let v = value_text(&g["game"][k]);
                if v == "—" {
                    None
                } else {
                    Some(if *k == "users" {
                        format!("Players: {v}")
                    } else {
                        v
                    })
                }
            })
            .collect::<Vec<_>>()
            .join(" · ");
        format!("{}\n{}\n{}", s.title(), details, value_text(&g["message"]))
    }
}
pub fn value_text(v: &Value) -> String {
    match v {
        Value::Null => "—".into(),
        Value::String(s) => files::plain(s),
        v => v.to_string(),
    }
}
pub fn action_name(action: &str) -> &str {
    match action {
        "doctor" => "USB check",
        "probe" => "Cartridge detection",
        "inspect" => "ROM inspection",
        "game" => "Game artwork",
        "read" => "Read",
        "backup" => "Backup",
        "check" => "Compatibility check",
        "verify" => "Verification",
        "write" => "Write",
        "wipe" => "Wipe",
        "checksum" => "Checksum comparison",
        _ => action,
    }
}

pub const SUPPORT: &str = r#"READERS
INLretro: GB / Color, GBA, NES and Famicom. GBA ROM access is read-only.
GBxCart RW: v1.3 / L1 supports qualified read-only GBA access. The v1.4 family with L12–L15 supports GB / Color and GBA reading; PCB 6 / L14 also supports Ferrante 512 write/wipe.
GB Operator: qualified GB / Color and GBA ROM detection, read, backup and verification. Saved games and ROM programming are pending.
Select a reader in Preferences, or use Automatic when exactly one candidate is connected.

PHYSICAL SLOTS
NES: 72-pin connector. Famicom: separate 60-pin connector. INLretro shares its side-entry 32-pin connector between GB / Color and GBA. GBxCart RW and GB Operator each use one cartridge connector. GBA always uses 3.3 V. A ROM file cannot determine the occupied connector. Connect only one cartridge per reader.

NES AND FAMICOM BOARD SUPPORT
Broke Studio UNROM-512 v2.1 · mapper 30
512 KiB flash / 32 KiB CHR RAM. Read, back up, verify, wipe and write. Automatic detection recognizes the supported electrical interface, not the PCB brand or revision.

NROM-128 / NROM-256 · mapper 0
Read, back up and verify using a known manual profile. Physical qualification is pending.

GAME BOY / COLOR
Automatic reading · ROM-only, MBC1, MBC2, MBC3, MBC5
Headers determine reading requirements, not flash wiring. Retail ROM cartridges cannot be rewritten.

SST39SF040 AUDIO/MBC5 · 5 V / 512 KiB
Manual flash profile for INLretro and GBxCart PCB 6 / L14: read, backup, verify, wipe and write. Write sources must be MBC5 type 0x19, without save RAM, within 512 KiB and with valid header and global checksums.

GAME BOY ADVANCE
INLretro, GBxCart RW and GB Operator support standard linear ROMs up to 32 MiB. Detect, read, backup and verify at 3.3 V. Automatic sizing uses a catalog size hint confirmed by the complete hash, or a mirroring estimate. GBA size overrides are in Preferences. Unknown images retain warnings; saves, RTC, DACS, banked boards and SD-card cartridges are excluded.

QUALIFICATION
Physical qualification, exact hashes and playtest results are recorded in the reader documentation. Two matching reads prove consistency; emulator smoke tests independently check that qualified dumps boot and accept input.

Save RAM, RTC and special peripherals are not included. Game identity uses exact hashes and size; a game match never identifies the physical board.

Neither INLretro physical button is normally needed. Leave BL alone. Disconnect USB before changing cartridges."#;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reader_capabilities_and_selection_are_shared_by_all_frontends() {
        let root = tempfile::tempdir().unwrap();
        let mut app = App::new(Some(root.path().into()), false);
        app.finish(
            &Request::new("doctor"),
            false,
            Ok(json!({"device":{"driver":"gbxcart","firmware":"L14"}})),
        );
        assert!(app.reader.contains("GBxCart RW"));
        assert!(app.can("read"));
        assert!(!app.can("wipe"));
        assert!(app.write_note().contains("Automatic mode is read-only"));
        app.set_profile(app.platform.profiles()[1]).unwrap();
        assert!(app.can("read"));
        assert!(app.can("wipe"));
        assert!(!app.can("write")); // Still needs a reviewed source.
        assert!(app.write_note().contains("Load a compatible ROM"));
        assert_eq!(app.request("read").reader, Kind::Gbxcart);
        app.set_reader(Kind::Inlretro).unwrap();
        assert!(!app.connected);
        assert!(app.detection.is_none());
        assert!(app.review.is_none());
        assert_eq!(
            App::new(Some(root.path().into()), false).settings.reader,
            Kind::Inlretro
        );
    }
    #[test]
    fn nes_and_famicom_keep_distinct_slots_when_loading_the_same_rom() {
        let root = tempfile::tempdir().unwrap();
        let mut app = App::new(Some(root.path().into()), false);
        app.connected = true;
        for slot in [Platform::Nes, Platform::Famicom] {
            app.set_platform(slot).unwrap();
            app.detection = Some(json!({"profile":"broke-unrom512"}));
            app.set_source(json!({"platform":"famicom","path":"test.nes"}), None);
            assert_eq!(app.platform, slot);
            assert!(app.detection.is_some());
            assert_eq!(app.request("read").platform, slot.id());
            app.begin("wipe").unwrap();
            assert!(app
                .review
                .as_ref()
                .unwrap()
                .text()
                .contains(slot.connector()));
            let other = if slot == Platform::Nes {
                Platform::Famicom
            } else {
                Platform::Nes
            };
            app.set_platform(other).unwrap();
            assert!(app.review.is_none());
            assert!(app.detection.is_none());
            assert!(!app.writable());
        }
        app.set_source(json!({"platform":"gameboy","path":"test.gb"}), None);
        assert_eq!(app.platform, Platform::Nes);
        assert!(app.status.contains("physical slot"));
    }
    #[test]
    fn unknown_and_retail_boards_never_enable_destructive_actions() {
        let root = tempfile::tempdir().unwrap();
        let mut app = App::new(Some(root.path().into()), false);
        app.connected = true;
        assert!(app.can("read"));
        assert!(!app.can("write"));
        assert!(!app.can("wipe"));
        app.detection = Some(json!({"profile":"auto","summary":"MBC1"}));
        assert!(!app.writable());
        app.set_platform(Platform::Famicom).unwrap();
        app.detection = Some(json!({"profile":"nrom128"}));
        assert!(!app.writable());
        app.detection = Some(json!({"profile":"broke-unrom512"}));
        assert!(app.writable());
        app.set_profile(app.platform.profiles()[0]).unwrap();
        assert!(app.detection.is_none());
        assert!(!app.writable());
    }
    #[test]
    fn write_review_pins_source_and_board_and_requires_exact_word() {
        let root = tempfile::tempdir().unwrap();
        let mut app = App::new(Some(root.path().into()), false);
        app.connected = true;
        app.set_platform(Platform::Famicom).unwrap();
        app.detection = Some(json!({"profile":"broke-unrom512"}));
        app.source = Some(Source {
            path: root.path().join("source.nes"),
            info: json!({"hashes":{"sha256":"123456"}}),
        });
        let request = app.request("check");
        app.finish(&request, true, Ok(json!({"message":"Compatible"})));
        let review = app.review.as_mut().unwrap();
        assert_eq!(review.request.profile, "broke-unrom512");
        assert_eq!(review.request.source_sha256.as_deref(), Some("123456"));
        assert!(!review.request.confirmed);
        assert!(!review.ready());
        review.word = "write".into();
        assert!(!review.ready());
        review.word = "WRITE".into();
        assert!(review.ready());
        assert!(!app.can("write"));
        assert!(!app.can("read"));
        app.set_platform(Platform::GameBoy).unwrap();
        assert!(app.review.is_none());
    }
    #[test]
    fn wipe_review_does_not_start_hardware_and_checks_cannot_weaken_write_options() {
        let root = tempfile::tempdir().unwrap();
        let mut app = App::new(Some(root.path().into()), false);
        app.connected = true;
        app.set_profile(app.platform.profiles()[1]).unwrap();
        app.settings.double_read = false;
        app.settings.strict_checksum = false;
        app.begin("wipe").unwrap();
        assert!(!app.busy());
        assert!(!app.review.as_ref().unwrap().request.confirmed);
        assert_eq!(app.approve().unwrap_err().code, "CONFIRMATION_REQUIRED");
        // Core tests independently prove write/backups ignore optional read flags.
        assert!(!app.review.as_ref().unwrap().request.double_read);
    }
    #[test]
    fn source_change_blocks_copy_and_existing_exports_are_never_overwritten() {
        let root = tempfile::tempdir().unwrap();
        let mut app = App::new(Some(root.path().into()), true);
        let path = root.path().join("test.gb");
        fs::write(&path, b"original").unwrap();
        app.source = Some(Source {
            path: path.clone(),
            info: json!({"hashes":{"sha256":cartridge_core::rom::sha(b"original")}}),
        });
        let dest = root.path().join("copy.gb");
        app.save_rom(&dest).unwrap();
        assert_eq!(app.save_rom(&dest).unwrap_err().code, "OUTPUT_EXISTS");
        fs::write(&path, b"changed").unwrap();
        assert_eq!(
            app.save_rom(&root.path().join("other.gb"))
                .unwrap_err()
                .code,
            "SOURCE_CHANGED"
        );
        assert!(!root.path().join("other.gb").exists());
    }
    #[test]
    fn legacy_preferences_are_preserved_and_corruption_is_actionable() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("studio");
        fs::create_dir(&folder).unwrap();
        let path = folder.join("settings.json");
        fs::write(
            &path,
            br#"{"double_read":false,"tui_theme":"custom","unrelated":42}"#,
        )
        .unwrap();
        let mut app = App::new(Some(root.path().into()), true);
        assert!(!app.settings.double_read);
        app.settings.strict_checksum = false;
        app.save_settings().unwrap();
        assert_eq!(files::json_file(&path).unwrap()["unrelated"], 42);
        fs::write(&path, b"not-json").unwrap();
        let mut app = App::new(Some(root.path().into()), true);
        assert_eq!(app.error.as_ref().unwrap().code, "SETTINGS_UNREADABLE");
        assert!(app.save_settings().is_err());
        assert_eq!(fs::read(&path).unwrap(), b"not-json");
    }
    #[test]
    fn failed_usb_operation_invalidates_detection_but_preserves_recovery_folder() {
        let root = tempfile::tempdir().unwrap();
        let mut app = App::new(Some(root.path().into()), false);
        app.connected = true;
        app.detection = Some(json!({"profile":"broke-unrom512"}));
        app.finish(
            &Request::new("write"),
            false,
            Err(
                Error::new("READER_DISCONNECTED", "Disconnected", "Restore source")
                    .details(json!({"backup_directory":root.path()})),
            ),
        );
        assert!(!app.connected);
        assert!(app.detection.is_none());
        assert_eq!(app.directory.as_deref(), Some(root.path()));
    }
}

#[cfg(test)]
mod gba_tests {
    use super::*;
    #[test]
    fn gba_settings_requests_and_reader_gating_are_shared() {
        let root = tempfile::tempdir().unwrap();
        let mut app = App::new(Some(root.path().into()), false);
        app.set_platform(Platform::Gba).unwrap();
        app.set_reader(Kind::Gbxcart).unwrap();
        app.connected = true;
        assert!(app.can("read"));
        assert!(!app.can("write"));
        assert!(!app.can("wipe"));
        assert_eq!(app.request("read").gba_rom_bytes, None);
        app.settings.gba_size = GbaSize::M16;
        app.save_settings().unwrap();
        assert_eq!(app.request("read").gba_rom_bytes, Some(16 * 1024 * 1024));
        let loaded = App::new(Some(root.path().into()), false);
        assert_eq!(loaded.settings.gba_size, GbaSize::M16);
        app.set_reader(Kind::Inlretro).unwrap();
        app.connected = true;
        assert!(app.can("probe"));
        assert!(app.can("read"));
        assert!(!app.can("write"));
        assert!(!app.can("wipe"));
        app.set_reader(Kind::Operator).unwrap();
        app.connected = true;
        assert!(app.can("probe"));
        assert!(app.can("read"));
        assert!(!app.can("write"));
        assert!(!app.can("wipe"));
        app.set_platform(Platform::GameBoy).unwrap();
        assert_eq!(app.request("read").gba_rom_bytes, None);
    }
}
