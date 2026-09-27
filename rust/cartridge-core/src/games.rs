//! Optional exact game identity and artwork. No cartridge access occurs here.
use crate::{
    rom::{self, NesRom},
    storage::{self, Cancel},
    Error, Result,
};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
pub const ART_TYPES: [(&str, &str, &str); 3] = [
    ("boxart", "Named_Boxarts", "Box art"),
    ("screenshot", "Named_Snaps", "Gameplay"),
    ("title_screen", "Named_Titles", "Title screen"),
];
const MAX_IMAGE: usize = 4 * 1024 * 1024;
fn optional(e: impl std::fmt::Display) -> Error {
    Error::new(
        "GAME_INFO_UNAVAILABLE",
        "Game details could not be loaded. The ROM is safe.",
        "Check the connection and library folder access, then retry Fetch artwork.",
    )
    .details(json!({"reason":e.to_string()}))
}
fn catalog() -> Result<Connection> {
    let db = if let Some(path) = std::env::var_os("CARTRIDGE_STUDIO_GAME_CATALOG") {
        Connection::open_with_flags(
            storage::expand(Path::new(&path)),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(optional)?
    } else {
        let mut c = Connection::open_in_memory().map_err(optional)?;
        c.deserialize_bytes(
            "main",
            include_bytes!(env!("CARTRIDGE_STUDIO_EMBEDDED_CATALOG")),
        )
        .map_err(optional)?;
        c
    };
    if db
        .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
        .map_err(optional)?
        != 1
    {
        return Err(optional("Unsupported catalog version"));
    }
    Ok(db)
}
pub fn identify_with(db: &Connection, data: &[u8], platform: &str) -> Result<Value> {
    let mut result = json!({"status":"not_found","source_sha256":rom::sha(data),"message":"No exact database match. This may be a prototype, modified ROM, or an uncatalogued release.","artwork":{}});
    let mut catalog = json!({});
    for row in db
        .prepare("SELECT key,value FROM catalog")
        .map_err(optional)?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(optional)?
    {
        let (k, v) = row.map_err(optional)?;
        catalog[k] = json!(v);
    }
    result["catalog"] = catalog;
    let mut candidates = vec![("file", std::borrow::Cow::Borrowed(data))];
    if platform == "famicom" {
        let r = NesRom::parse(data)?;
        if r.trainer.is_empty() {
            let mut payload = r.prg;
            payload.extend(r.chr);
            candidates.push(("nes_payload", std::borrow::Cow::Owned(payload)));
        }
    }
    let (p1, p2) = match platform {
        "gameboy" => ("gb", "gbc"),
        "gba" => ("gba", "gba"),
        "famicom" | "nes" => ("nes", "nes"),
        _ => return Err(optional("Unknown game platform")),
    };
    for (method, bytes) in candidates {
        let digest = hex::encode(Sha1::digest(&bytes));
        let mut statement=db.prepare("SELECT id,metadata FROM games WHERE platform IN (?1,?2) AND size=?3 AND sha1=?4 ORDER BY name").map_err(optional)?;
        let rows = statement
            .query_map(params![p1, p2, bytes.len() as i64, digest], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(optional)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(optional)?;
        if rows.is_empty() {
            continue;
        }
        result["match"] =
            json!({"method":method,"algorithm":"sha1","value":digest,"bytes":bytes.len()});
        if rows.len() > 1 {
            let names: Vec<Value> = rows
                .iter()
                .filter_map(|(_, s)| {
                    serde_json::from_str::<Value>(s)
                        .ok()
                        .map(|v| v["name"].clone())
                })
                .collect();
            result["status"] = json!("ambiguous");
            result["candidates"] = json!(names);
            result["message"] = json!(
                "The checksum matches multiple catalog entries. No single release was selected."
            );
            return Ok(result);
        }
        let mut game: Value = serde_json::from_str(&rows[0].1)?;
        if !game.is_object() || !game["name"].is_string() {
            return Err(optional("Invalid catalog entry"));
        }
        game["id"] = json!(rows[0].0);
        result["status"] = json!("identified");
        result["game"] = game;
        result["message"] = json!(if method == "file" {
            "Exact file match · SHA-1 and size"
        } else {
            "Exact game-data match · SHA-1 and size; NES file header excluded"
        });
        return Ok(result);
    }
    Ok(result)
}
pub fn identify(data: &[u8], platform: &str) -> Value {
    catalog().and_then(|db|identify_with(&db,data,platform)).unwrap_or_else(|e|json!({"status":"unavailable","source_sha256":rom::sha(data),"artwork":{},"message":"Game catalog unavailable. Your ROM is safe. Reinstall the latest package, or build the game catalog when running from source.","detail":e.to_string()}))
}
/// A header code is only a size hint. Callers must confirm the complete ROM hash.
pub fn gba_size_hint(code: &str) -> Option<usize> {
    if code.len() != 4 || !code.bytes().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let db = catalog().ok()?;
    let mut statement = db.prepare("SELECT DISTINCT size FROM games WHERE platform='gba' AND json_extract(metadata,'$.serial')=?1").ok()?;
    let sizes = statement
        .query_map([hex::encode(code)], |r| r.get::<_, u32>(0))
        .ok()?
        .collect::<std::result::Result<Vec<_>, _>>()
        .ok()?;
    if sizes.len() == 1 {
        Some(sizes[0] as usize)
    } else {
        None
    }
}
pub fn thumbnail_url(game: &Value, kind: &str) -> Result<String> {
    let system = match game["platform"].as_str() {
        Some("gb") => "Nintendo - Game Boy",
        Some("gbc") => "Nintendo - Game Boy Color",
        Some("gba") => "Nintendo - Game Boy Advance",
        Some("nes") => "Nintendo - Nintendo Entertainment System",
        _ => return Err(optional("Unknown artwork platform")),
    };
    let folder = ART_TYPES
        .iter()
        .find(|a| a.0 == kind)
        .ok_or_else(|| optional("Unknown artwork type"))?
        .1;
    let name = game["name"]
        .as_str()
        .ok_or_else(|| optional("Missing game name"))?;
    let filename: String = name
        .chars()
        .map(|c| {
            if c < ' ' || "&*/:`<>?\\|\"".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    // Python's quote(..., safe='') encoding, including spaces as %20.
    let mut encoded = String::new();
    for b in format!("{filename}.png").bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            encoded.push(b as char)
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    Ok(format!(
        "https://raw.githubusercontent.com/libretro-thumbnails/{}/master/{folder}/{encoded}",
        system.replace(' ', "_")
    ))
}
pub fn validate_png(data: &[u8]) -> Result<()> {
    if !(33..=MAX_IMAGE).contains(&data.len())
        || data[..16] != *b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR"
    {
        return Err(optional("Invalid or oversized PNG"));
    }
    let width = u32::from_be_bytes(data[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(data[20..24].try_into().unwrap());
    if !(1..=2048).contains(&width)
        || !(1..=2048).contains(&height)
        || data[data.len() - 12..] != *b"\x00\x00\x00\x00IEND\xaeB\x60\x82"
    {
        return Err(optional("Incomplete image or unsupported dimensions"));
    }
    let mut pos = 8;
    let mut idat = false;
    while pos < data.len() {
        if pos + 12 > data.len() {
            return Err(optional("Truncated PNG chunk"));
        }
        let size = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        let end = pos
            .checked_add(8 + size)
            .filter(|&n| n + 4 <= data.len())
            .ok_or_else(|| optional("Truncated PNG chunk"))?;
        if crc32fast::hash(&data[pos + 4..end])
            != u32::from_be_bytes(data[end..end + 4].try_into().unwrap())
        {
            return Err(optional("Artwork image checksum failed"));
        }
        idat |= &data[pos + 4..pos + 8] == b"IDAT";
        pos = end + 4;
    }
    if !idat {
        return Err(optional("Artwork has no image data"));
    }
    Ok(())
}
fn image_read(path: &Path) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    fs::File::open(path)?
        .take(MAX_IMAGE as u64 + 1)
        .read_to_end(&mut data)?;
    validate_png(&data)?;
    Ok(data)
}
fn download(address: &str, cancel: &Cancel) -> Result<Option<Vec<u8>>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(4)))
        .build()
        .into();
    let mut url = url::Url::parse(address).map_err(optional)?;
    for _ in 0..5 {
        cancel.check()?;
        if url.scheme() != "https"
            || url.host_str() != Some("raw.githubusercontent.com")
            || url.port_or_known_default() != Some(443)
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(optional(
                "The artwork provider redirected to an unexpected location",
            ));
        }
        let mut response = agent
            .get(url.as_str())
            .header("User-Agent", "Cartridge-Studio")
            .header("Accept", "image/png")
            .call()
            .map_err(optional)?;
        let status = response.status().as_u16();
        if status == 404 {
            return Ok(None);
        }
        if (300..400).contains(&status) {
            let location = response
                .headers()
                .get("location")
                .and_then(|s| s.to_str().ok())
                .ok_or_else(|| optional("Missing artwork redirect location"))?;
            url = url.join(location).map_err(optional)?;
            continue;
        }
        if status != 200 {
            return Err(optional(format!("Artwork server returned HTTP {status}")));
        }
        if response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<usize>().ok())
            .is_some_and(|n| n > MAX_IMAGE)
        {
            return Err(optional("Artwork exceeds the download limit"));
        }
        let data = response
            .body_mut()
            .with_config()
            .limit(MAX_IMAGE as u64)
            .read_to_vec()
            .map_err(optional)?;
        cancel.check()?;
        validate_png(&data)?;
        return Ok(Some(data));
    }
    Err(optional("Too many artwork redirects"))
}
pub fn artwork(
    result: &mut Value,
    root: &Path,
    backup: Option<&Path>,
    online: bool,
    refresh: bool,
    cancel: &Cancel,
) {
    if result["status"] != "identified" {
        return;
    }
    result["artwork"] = json!({});
    let directory = root.join("game-cache/artwork");
    for (kind, _, title) in ART_TYPES {
        let address = match thumbnail_url(&result["game"], kind) {
            Ok(u) => u,
            Err(e) => {
                result["artwork_message"] = json!(e.message);
                continue;
            }
        };
        let key = rom::sha(address.as_bytes());
        let path = directory.join(format!("{key}.png"));
        let missing = directory.join(format!("{key}.missing"));
        let mut item = json!({"title":title,"url":address,"status":"not_downloaded"});
        let outcome = (|| {
            let mut saved = vec![path.clone()];
            if let Some(b) = backup {
                saved.push(b.join("artwork").join(format!("{kind}.png")));
            }
            for p in saved {
                if image_read(&p).is_ok() {
                    item["status"] = json!("cached");
                    item["path"] = json!(p);
                    return Ok(());
                }
            }
            if !online {
                item["message"] = json!("Not cached. Choose Fetch artwork when online.");
                return Ok(());
            }
            if !refresh
                && missing
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age < Duration::from_secs(86400))
            {
                item["status"] = json!("missing");
                item["message"] = json!(
                    "No image is available for this release. Retry later with Fetch artwork."
                );
                return Ok(());
            }
            if let Some(data) = download(&address, cancel)? {
                fs::create_dir_all(&directory)?;
                storage::atomic(&path, &data)?;
                let _ = fs::remove_file(&missing);
                item["status"] = json!("cached");
                item["path"] = json!(path);
            } else {
                item["status"] = json!("missing");
                item["message"] = json!("No image is available for this release.");
                let _ = fs::create_dir_all(&directory)
                    .map_err(Error::from)
                    .and_then(|_| storage::atomic(&missing, b"404\n"));
            }
            Ok::<(), Error>(())
        })();
        if let Err(e) = outcome {
            item["status"] = json!("unavailable");
            item["message"]=json!("Artwork could not be downloaded or cached. Check your connection and library folder access, then choose Fetch artwork to retry.");
            item["detail"] = json!(e.to_string());
        }
        result["artwork"][kind] = item;
    }
}
pub fn archive(directory: &Path, game: &Value) -> Result<()> {
    let mut saved = game.clone();
    for (kind, _, _) in ART_TYPES {
        if let Some(p) = game["artwork"][kind]["path"].as_str() {
            let data = image_read(Path::new(p))?;
            let relative = PathBuf::from("artwork").join(format!("{kind}.png"));
            fs::create_dir_all(directory.join("artwork"))?;
            storage::atomic(&directory.join(&relative), &data)?;
            saved["artwork"][kind]["path"] = json!(relative);
        }
    }
    storage::save_report(&directory.join("game.json"), &saved)
}
pub fn lookup(
    path: &Path,
    data: &[u8],
    platform: &str,
    root: &Path,
    online: bool,
    refresh: bool,
    cancel: &Cancel,
) -> Value {
    let mut result = identify(data, platform);
    let parent = path.parent().unwrap_or(Path::new("."));
    let sidecar = parent.join("game.json");
    let backup = if sidecar.metadata().is_ok_and(|m| m.len() < 1024 * 1024) {
        fs::read(&sidecar)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .filter(|s| {
                result["status"] == "identified"
                    && s["source_sha256"] == result["source_sha256"]
                    && s["game"]["id"] == result["game"]["id"]
            })
            .map(|_| parent)
    } else {
        None
    };
    artwork(&mut result, root, backup, online, refresh, cancel);
    if online && backup.is_some() {
        if let Err(e) = archive(parent, &result) {
            result["artwork_message"] = json!(e.message);
        }
    }
    result
}
pub fn after_read(
    report: &mut Value,
    root: &Path,
    online: bool,
    cancel: &Cancel,
    progress: &mut dyn FnMut(String),
) {
    if report["status"] != "complete"
        || !matches!(report["operation"].as_str(), Some("read" | "backup"))
        || !report["output"].is_string()
    {
        return;
    }
    progress("Cartridge saved. Looking up game details…".into());
    let outcome = (|| {
        let path = PathBuf::from(report["output"].as_str().unwrap());
        let data = storage::read_rom(&path)?;
        let game = lookup(
            &path,
            &data,
            report["platform"].as_str().unwrap_or(""),
            root,
            online,
            false,
            cancel,
        );
        report["game"] = game;
        let directory = PathBuf::from(report["directory"].as_str().unwrap());
        archive(&directory, &report["game"])?;
        storage::save_report(&directory.join("report.json"), report)?;
        Ok::<(), Error>(())
    })();
    if let Err(e) = outcome {
        report["game_warning"]=json!("Game details could not be fully saved. The ROM and original read report are preserved. Check library folder access and retry the lookup.");
        report["game_detail"] = json!(e.to_string());
    }
}
