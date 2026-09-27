use crate::{Error, Result};
use cartridge_core::storage;
use serde_json::Value;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub fn json_file(path: &Path) -> Result<Value> {
    let mut data = Vec::new();
    fs::File::open(path)?
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut data)?;
    if data.len() > 2 * 1024 * 1024 {
        return Err(Error::new(
            "METADATA_TOO_LARGE",
            "This report is too large to display.",
            "Keep the report and open a smaller operation report.",
        ));
    }
    Ok(serde_json::from_slice(&data)?)
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub directory: bool,
}
impl Entry {
    pub fn label(&self) -> String {
        format!(
            "{}{}",
            self.path.file_name().unwrap_or_default().to_string_lossy(),
            if self.directory { "/" } else { "" }
        )
    }
}
#[derive(Clone, Debug)]
pub struct Browser {
    pub folder: PathBuf,
    pub entries: Vec<Entry>,
    pub path: String,
    pub note: String,
}
impl Browser {
    pub fn new(folder: &Path) -> Result<Self> {
        let mut this = Self {
            folder: folder.into(),
            entries: vec![],
            path: folder.display().to_string(),
            note: String::new(),
        };
        this.navigate(folder)?;
        Ok(this)
    }
    pub fn navigate(&mut self, folder: &Path) -> Result<()> {
        let folder = storage::expand(folder);
        let mut entries = Vec::new();
        let mut truncated = false;
        for entry in fs::read_dir(&folder)? {
            let path = entry?.path();
            if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                continue;
            }
            let directory = path.is_dir();
            if directory
                || path.extension().is_some_and(|e| {
                    ["gb", "gbc", "gba", "nes"]
                        .iter()
                        .any(|x| e.eq_ignore_ascii_case(x))
                })
            {
                entries.push(Entry { path, directory });
            }
            if entries.len() >= 1000 {
                truncated = true;
                break;
            }
        }
        entries.sort_by_key(|e| (!e.directory, e.label().to_lowercase()));
        self.path = folder.display().to_string();
        self.folder = folder;
        self.entries = entries;
        self.note = if truncated {
            "Showing 1,000 entries. Paste a full path to load an unlisted ROM."
        } else {
            "Choose a folder or ROM. You can also paste a full file path. Extract archives first."
        }
        .into();
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct History {
    pub path: PathBuf,
    pub report: Value,
}
pub fn history(root: &Path) -> Result<Vec<History>> {
    let folder = root.join("cartridge-backups");
    if !folder.exists() {
        return Ok(vec![]);
    }
    let mut paths = fs::read_dir(folder)?
        .filter_map(|e| e.ok().map(|e| e.path().join("report.json")))
        .filter(|p| p.is_file())
        .collect::<Vec<_>>();
    paths.sort_by_key(|p| std::cmp::Reverse(p.metadata().and_then(|m| m.modified()).ok()));
    Ok(paths
        .into_iter()
        .take(100)
        .map(|path| {
            let report = json_file(&path).unwrap_or_else(
                |e| serde_json::json!({"status":"unreadable","message":e.to_string()}),
            );
            History { path, report }
        })
        .collect())
}

/// Never emit terminal escapes or control sequences from a file/device/catalog.
pub fn plain(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}
