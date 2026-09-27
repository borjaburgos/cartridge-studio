use crate::{Error, Result};
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Clone, Default)]
pub struct Cancel(pub Arc<AtomicBool>);
impl Cancel {
    pub fn check(&self) -> Result<()> {
        if self.0.load(Ordering::Relaxed) {
            Err(Error::interrupted())
        } else {
            Ok(())
        }
    }
}
pub fn data_root() -> PathBuf {
    if let Some(p) =
        std::env::var_os("CARTRIDGE_STUDIO_DATA_DIR").or_else(|| std::env::var_os("INL_DATA_DIR"))
    {
        return expand(Path::new(&p));
    }
    // A developer checkout is inferred from this executable's location, never
    // from the machine-specific build path embedded by Cargo.
    if let Ok(executable) = std::env::current_exe() {
        for parent in executable.ancestors().take(6) {
            if parent.join("rust/cartridge-core/Cargo.toml").is_file()
                && parent.join("Cargo.toml").is_file()
            {
                return parent.join("tmp");
            }
        }
    }
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let (current, legacy) = if cfg!(target_os = "macos") {
        (
            home.join("Library/Application Support/Cartridge Studio"),
            home.join("Library/Application Support/INL"),
        )
    } else {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or(home.join(".local/share"));
        (base.join("cartridge-studio/library"), base.join("inl"))
    };
    // Keep existing reports, artwork and preferences at their original location.
    // Reports contain absolute recovery paths, so never rename a library silently.
    if !current.exists() && legacy.is_dir() {
        legacy
    } else {
        current
    }
}
pub fn expand(path: &Path) -> PathBuf {
    let p = if let Ok(tail) = path.strip_prefix("~") {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(tail)
    } else {
        path.to_owned()
    };
    if p.is_absolute() {
        p
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    }
}
pub fn stamp() -> String {
    chrono::Utc::now().format("%Y%m%d-%H%M%S-%6f").to_string()
}
pub fn new_directory(platform: &str, action: &str) -> PathBuf {
    data_root().join("cartridge-backups").join(format!(
        "{platform}-{action}-{}-{}",
        stamp(),
        std::process::id()
    ))
}
pub fn read_rom(path: &Path) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    File::open(path)?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut data)?;
    if data.len() > 64 * 1024 * 1024 {
        return Err(Error::new(
            "ROM_TOO_LARGE",
            "This file is larger than the supported ROM limit.",
            "Choose an uncompressed Game Boy, Game Boy Color, Game Boy Advance, or NES/Famicom ROM, up to 64 MiB.",
        ));
    }
    Ok(data)
}
pub fn write_new(path: &Path, data: &[u8]) -> Result<()> {
    let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
    f.write_all(data)?;
    f.sync_all()?;
    sync_parent(path)?;
    Ok(())
}
pub fn sync_parent(path: &Path) -> Result<()> {
    File::open(path.parent().unwrap_or(Path::new(".")))?.sync_all()?;
    Ok(())
}
pub fn atomic(path: &Path, data: &[u8]) -> Result<()> {
    // A unique sibling is required: a stale/symlinked temp file must never be followed.
    let temp = path.with_extension(format!("tmp-{}-{}", std::process::id(), stamp()));
    let result = (|| {
        write_new(&temp, data)?;
        fs::rename(&temp, path)?;
        sync_parent(path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
pub fn save_report(path: &Path, report: &Value) -> Result<()> {
    atomic(path, &serde_json::to_vec_pretty(report)?)
}
pub fn prepare_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| Error::check("Choose an operation folder."))?,
    )?;
    if path.try_exists()? {
        return Err(Error::new(
            "BACKUP_FOLDER_EXISTS",
            "That operation folder already exists.",
            "Choose a new folder; existing backups are never overwritten.",
        ));
    }
    if fs2::available_space(path.parent().unwrap())? < 64 * 1024 * 1024 {
        return Err(Error::new(
            "INSUFFICIENT_SPACE",
            "Less than 64 MiB is available for backups and verification files.",
            "Free space or choose another backup location before trying again.",
        ));
    }
    Ok(())
}
