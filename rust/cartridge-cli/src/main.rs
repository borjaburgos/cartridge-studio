use cartridge_core::{
    readers::Kind,
    rom,
    service::{self, Request},
    storage::{self, Cancel},
    Error, Result,
};
use clap::{Args, Parser, Subcommand};
use serde_json::{json, Value};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
#[derive(Parser)]
#[command(name="cartridge",version=cartridge_core::VERSION,about="Read, back up and rewrite supported cartridges with native cartridge readers. Rust core.")]
struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, global=true, default_value="auto", value_parser=["auto","inlretro","gbxcart","operator"])]
    reader: String,
    #[arg(long, global = true, value_name = "DEVICE")]
    port: Option<String>,
    #[arg(long, global = true, value_name = "FOLDER")]
    library: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Commands>,
}
#[derive(Subcommand)]
enum Commands {
    /// List qualified physical cartridge profiles.
    Profiles,
    /// Check reader discovery without accessing a cartridge.
    Doctor,
    /// Identify a supported board without erase/program commands.
    Detect {
        #[arg(long,value_parser=["gameboy","famicom","nes","gba"])]
        platform: String,
    },
    /// Inspect a saved ROM, its checksums and offline game identity.
    Inspect { rom: PathBuf },
    /// Compare a saved ROM against a CRC32, SHA-1 or SHA-256.
    Checksum { rom: PathBuf, expected: String },
    /// Identify a saved ROM in the offline game catalog.
    GameInfo {
        rom: PathBuf,
        #[arg(long)]
        artwork: bool,
    },
    /// Game Boy / Color cartridges. Automatic mode reads and verifies only.
    Gameboy(Cartridge),
    /// Game Boy Advance ROMs through INLretro, GBxCart RW or GB Operator (read-only).
    Gba(Cartridge),
    /// Famicom cartridges in the 60-pin slot.
    Famicom(Cartridge),
    /// NES cartridges in the separate 72-pin slot.
    Nes(Cartridge),
    /// Configure permanent USB access where the operating system requires it.
    UsbSetup {
        #[arg(long)]
        check: bool,
    },
    /// Open the installed terminal interface.
    Tui {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}
#[derive(Args)]
struct Cartridge {
    #[command(subcommand)]
    action: Action,
}
#[derive(Subcommand)]
enum Action {
    Probe {
        #[arg(long, default_value = "auto")]
        profile: String,
    },
    Read(ReadArgs),
    Backup(ReadArgs),
    Check {
        rom: PathBuf,
        #[arg(long)]
        profile: String,
        #[arg(long)]
        mirroring: Option<String>,
    },
    Verify {
        rom: PathBuf,
        #[command(flatten)]
        options: Physical,
    },
    /// Back up twice, erase, verify blank, program and verify twice.
    Write {
        rom: PathBuf,
        #[command(flatten)]
        options: Physical,
        #[arg(long)]
        yes: bool,
    },
    /// Back up twice, erase and verify the entire chip is blank.
    #[command(alias = "erase")]
    Wipe {
        #[command(flatten)]
        options: Physical,
        #[arg(long)]
        yes: bool,
    },
}
#[derive(Args)]
struct Physical {
    #[arg(long, default_value = "auto")]
    profile: String,
    #[arg(long)]
    backup_dir: Option<PathBuf>,
    /// Override the GBA read size in MiB (automatic uses catalog hints or mirroring).
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=32))]
    rom_size_mib: Option<u8>,
}
#[derive(Args)]
struct ReadArgs {
    #[command(flatten)]
    options: Physical,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long)]
    no_artwork: bool,
    #[arg(long)]
    single_read: bool,
    /// Allow a bad Game Boy global checksum on an ordinary read; GBA header checks remain mandatory.
    #[arg(long)]
    allow_bad_checksum: bool,
}
fn usb_setup(check: bool) -> Result<Value> {
    if !cfg!(target_os = "linux") {
        return Ok(
            json!({"message":"Linux udev rules are not needed on macOS. Connect the reader and run cartridge doctor."}),
        );
    }
    let content = "# Exact cartridge-reader identities used by Cartridge Studio.\n\
SUBSYSTEM==\"usb\", ENV{DEVTYPE}==\"usb_device\", ATTR{idVendor}==\"16c0\", ATTR{idProduct}==\"05dc\", ATTR{manufacturer}==\"InfiniteNesLives.com\", ATTR{product}==\"INL Retro-Prog\", TAG+=\"uaccess\"\n\
SUBSYSTEM==\"usb\", ENV{DEVTYPE}==\"usb_device\", ATTR{idVendor}==\"16d0\", ATTR{idProduct}==\"123d\", ATTR{manufacturer}==\"Epilogue\", ATTR{product}==\"GB Operator\", TAG+=\"uaccess\"\n";
    let path = Path::new("/etc/udev/rules.d/70-cartridge-studio.rules");
    if check {
        return Ok(json!({"message":format!("{}:\n{content}",path.display())}));
    }
    fn udev(args: &[&str]) -> Result<()> {
        let out = Command::new("udevadm").args(args).output().map_err(|e| {
            Error::new(
                "UDEV_MISSING",
                "Linux udev tools could not be started.",
                "Install your distribution's udev/systemd tools, then rerun usb-setup.",
            )
            .details(json!({"reason":e.to_string()}))
        })?;
        if !out.status.success() {
            return Err(Error::new("USB_SETUP_FAILED","USB access setup failed.","Run usb-setup with administrator access and check that udev is running. The rule may already be installed; reconnect after resolving the error.").details(json!({"output":String::from_utf8_lossy(&out.stderr)})));
        }
        Ok(())
    }
    if path.exists() {
        if std::fs::read_to_string(path)? != content {
            return Err(Error::new("USB_RULE_EXISTS","A different rule already exists at the Cartridge Studio USB rule path.","Review that rule with your administrator before replacing it. It has not been overwritten."));
        }
    } else {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .open(path)
            .map_err(|e| {
                Error::new(
                    "USB_SETUP_PERMISSION",
                    "Installing the USB rule requires administrator access.",
                    "Run sudo cartridge usb-setup, then reconnect the reader.",
                )
                .details(json!({"reason":e.to_string()}))
            })?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
    }
    udev(&["control", "--reload-rules"])?;
    let mut count = 0;
    let identities = [
        [
            ("idVendor", "16c0"),
            ("idProduct", "05dc"),
            ("manufacturer", "InfiniteNesLives.com"),
            ("product", "INL Retro-Prog"),
        ],
        [
            ("idVendor", "16d0"),
            ("idProduct", "123d"),
            ("manufacturer", "Epilogue"),
            ("product", "GB Operator"),
        ],
    ];
    for entry in std::fs::read_dir("/sys/bus/usb/devices")? {
        let p = entry?.path();
        if identities.iter().any(|identity| {
            identity
                .iter()
                .all(|(k, v)| std::fs::read_to_string(p.join(k)).is_ok_and(|s| s.trim() == *v))
        }) {
            udev(&[
                "trigger",
                "--action=change",
                p.to_str()
                    .ok_or_else(|| Error::check("Invalid USB device path"))?,
            ])?;
            count += 1;
        }
    }
    udev(&["settle", "--timeout=10"])?;
    Ok(
        json!({"message":format!("Installed {}; access for the active desktop account; applied to {count} reader(s).",path.display())}),
    )
}
#[cfg(target_os = "linux")]
fn serial_access_setup_action() -> &'static str {
    "Add your account to the serial-device group used by your distribution, sign in again, and reconnect. usb-setup only installs narrowly matched INLretro and GB Operator USB rules; it does not grant generic serial-device access."
}

fn tui_binary() -> Result<PathBuf> {
    let sibling = std::env::current_exe()?.with_file_name("cartridge-tui");
    if sibling.is_file() {
        return Ok(sibling);
    }
    #[cfg(target_os = "macos")]
    {
        let installed = PathBuf::from("/usr/local/bin/cartridge-tui");
        if installed.is_file() {
            return Ok(installed);
        }
    }
    #[cfg(target_os = "macos")]
    let action = "Rerun Cartridge Studio.pkg and include TUI in your component selection.";
    #[cfg(not(target_os = "macos"))]
    let action = "Rerun the installer and include TUI in your selection, or install the matching cartridge-studio TUI package for your distribution.";
    Err(Error::new(
        "TUI_NOT_INSTALLED",
        "The terminal interface is not installed.",
        action,
    ))
}
#[cfg(target_os = "macos")]
fn serial_access_setup_action() -> &'static str {
    "macOS does not use Linux serial-device groups or udev rules. Close FlashGBX and other cartridge applications, reconnect GBxCart directly, and choose its /dev/cu.usbserial port."
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn serial_access_setup_action() -> &'static str {
    "Use the operating system's serial-device access controls, reconnect GBxCart, and choose its current serial port."
}
fn show(v: &Value, structured: bool) {
    if structured {
        println!("{}", serde_json::to_string_pretty(v).unwrap());
        return;
    }
    if let Some(m) = v["message"].as_str() {
        println!("{m}");
    } else if v["status"] == "complete" {
        println!("Operation completed.");
    } else {
        println!("{}", serde_json::to_string_pretty(v).unwrap());
    }
    if let Some(p) = v["output"].as_str() {
        println!("ROM: {p}");
    }
    if let Some(p) = v["directory"].as_str() {
        println!("Backups and report: {p}");
    }
    if let Some(p) = v["warning"].as_str() {
        println!("Note: {p}");
    }
    if let Some(n) = v["game"]["game"]["name"].as_str() {
        println!("Game: {n}");
    }
    if let Some(n) = v["game"]["name"].as_str() {
        println!("Game: {n}");
    }
}
fn execute(cli: Cli, cancel: Cancel) -> Result<()> {
    let Some(command) = cli.command else {
        use clap::CommandFactory;
        Cli::command().print_help().map_err(Error::from)?;
        println!();
        return Ok(());
    };
    let mut r = Request::new("doctor");
    r.data_directory = cli.library;
    r.reader = match cli.reader.as_str() {
        "inlretro" => Kind::Inlretro,
        "gbxcart" => Kind::Gbxcart,
        "operator" => Kind::Operator,
        _ => Kind::Auto,
    };
    r.port = cli.port;
    let mut output = None;
    let mut only_game = false;
    let mut mirror = None;
    match command {
        Commands::Profiles => r.action = "profiles".into(),
        Commands::Doctor => (),
        Commands::Detect { platform } => {
            r.action = "probe".into();
            r.platform = platform;
        }
        Commands::Inspect { rom } => {
            r.action = "inspect".into();
            r.source = Some(rom);
        }
        Commands::Checksum { rom, expected } => {
            r.action = "checksum".into();
            r.source = Some(rom);
            r.expected = expected;
        }
        Commands::GameInfo { rom, artwork } => {
            r.action = if artwork { "game" } else { "inspect" }.into();
            r.source = Some(rom);
            only_game = true;
        }
        Commands::UsbSetup { check } => {
            if r.reader == Kind::Gbxcart || r.port.is_some() {
                return Err(Error::new(
                    "SERIAL_ACCESS_SETUP",
                    "GBxCart uses your operating system’s serial-device access permissions.",
                    serial_access_setup_action(),
                ));
            }
            show(&usb_setup(check)?, cli.json);
            return Ok(());
        }
        Commands::Tui { args } => {
            let path = tui_binary()?;
            let mut command = Command::new(path);
            if let Some(root) = r.data_directory {
                command.arg("--library").arg(root);
            }
            command.args(args);
            use std::os::unix::process::CommandExt;
            return Err(command.exec().into());
        }
        Commands::Gameboy(ref c)
        | Commands::Gba(ref c)
        | Commands::Famicom(ref c)
        | Commands::Nes(ref c) => {
            r.platform = if matches!(command, Commands::Gameboy(_)) {
                "gameboy"
            } else if matches!(command, Commands::Gba(_)) {
                "gba"
            } else if matches!(command, Commands::Nes(_)) {
                "nes"
            } else {
                "famicom"
            }
            .into();
            let physical = |r: &mut Request, p: &Physical| {
                r.profile = p.profile.clone();
                r.directory = p.backup_dir.clone();
                r.gba_rom_bytes = p.rom_size_mib.map(|n| n as usize * 1024 * 1024);
            };
            match &c.action {
                Action::Probe { profile } => {
                    r.action = "probe".into();
                    r.profile = profile.clone();
                }
                Action::Read(a) | Action::Backup(a) => {
                    r.action = if matches!(c.action, Action::Read(_)) {
                        "read"
                    } else {
                        "backup"
                    }
                    .into();
                    physical(&mut r, &a.options);
                    r.download_artwork = !a.no_artwork;
                    r.double_read = !a.single_read;
                    r.strict_checksum = !a.allow_bad_checksum;
                    output = a.output.clone();
                }
                Action::Check {
                    rom,
                    profile,
                    mirroring,
                } => {
                    r.action = "check".into();
                    r.source = Some(rom.clone());
                    r.profile = profile.clone();
                    mirror = mirroring.clone();
                }
                Action::Verify { rom, options } => {
                    r.action = "verify".into();
                    r.source = Some(rom.clone());
                    physical(&mut r, options);
                }
                Action::Write { rom, options, yes } => {
                    r.action = "write".into();
                    r.source = Some(rom.clone());
                    physical(&mut r, options);
                    r.confirmed = *yes;
                }
                Action::Wipe { options, yes } => {
                    r.action = "wipe".into();
                    physical(&mut r, options);
                    r.confirmed = *yes;
                }
            }
        }
    }
    if let Some(ref p) = output {
        let path = storage::expand(p);
        if path.exists() {
            return Err(Error::new(
                "OUTPUT_EXISTS",
                "The output already exists.",
                "Choose a new --output path; existing files are never overwritten.",
            ));
        }
        let parent = path.parent().unwrap();
        std::fs::create_dir_all(parent)?;
    }
    if matches!(r.action.as_str(), "write" | "verify") {
        if let Some(p) = &r.source {
            r.source_sha256 = Some(rom::sha(&storage::read_rom(&storage::expand(p))?));
        }
    }
    if let Some(m) = mirror {
        if r.platform != "famicom" && r.platform != "nes" {
            return Err(Error::check(
                "Mirroring applies only to Famicom/NES cartridges.",
            ));
        }
        let bytes = storage::read_rom(&storage::expand(r.source.as_ref().unwrap()))?;
        rom::compatibility(
            &rom::NesRom::parse(&bytes)?,
            rom::board(&r.profile)?,
            Some(&m),
            true,
        )?;
    }
    let mut v = service::run(&r, cancel, &mut |m| eprintln!("{m}"))?;
    if let Some(p) = output {
        if let Some(source) = v["output"].as_str() {
            let p = storage::expand(&p);
            if let Err(mut e) = storage::write_new(&p, &storage::read_rom(Path::new(source))?) {
                e.details["saved_rom"] = json!(source);
                e.details["backup_directory"] = v["directory"].clone();
                return Err(e);
            }
            v["exported_output"] = json!(p);
        }
    }
    show(if only_game { &v["game"] } else { &v }, cli.json);
    Ok(())
}
fn main() {
    let mut arguments = std::env::args_os();
    let invoked_as_worker = arguments.next().is_some_and(|p| {
        Path::new(&p)
            .file_name()
            .is_some_and(|n| n == "cartridge-worker")
    });
    if invoked_as_worker || arguments.next().is_some_and(|a| a == "--worker") {
        cartridge_worker::run();
        return;
    }
    let cli = Cli::parse();
    let structured = cli.json;
    let root = cli.library.clone().unwrap_or_else(storage::data_root);
    let cancel = Cancel::default();
    let c = cancel.clone();
    if ctrlc::set_handler(move || c.0.store(true, std::sync::atomic::Ordering::Relaxed)).is_err() {
        eprintln!(
            "Safe cancellation could not be initialized. Restart before accessing a cartridge."
        );
        std::process::exit(1);
    }
    if let Err(mut e) = execute(cli, cancel) {
        cartridge_core::operations::diagnostic(&mut e, &root);
        if structured {
            println!("{}", serde_json::to_string_pretty(&e).unwrap());
        } else {
            eprintln!("{e}");
            if e.details != json!({}) {
                eprintln!("{}", serde_json::to_string_pretty(&e.details).unwrap());
            }
        }
        std::process::exit(e.exit_code);
    }
}
