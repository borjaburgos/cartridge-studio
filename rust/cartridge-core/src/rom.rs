use crate::{Error, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;
pub const GB_PROFILE: &str = "sst39sf040-audio-mbc5";
pub const CAPACITY: usize = 524288;
pub const SPANSION_PROFILE: &str = "s29gl032m-r4-wr-mbc5";
pub const SPANSION_CAPACITY: usize = 4 * 1024 * 1024;
pub fn gb_flash_profile(profile: &str) -> bool {
    matches!(profile, GB_PROFILE | SPANSION_PROFILE)
}
pub fn validate_gb_flash_profile(data: &[u8], profile: &str) -> Result<Value> {
    if profile == GB_PROFILE {
        return validate_gb_flash(data);
    }
    if profile != SPANSION_PROFILE {
        return Err(Error::check("Select an exact supported flash board."));
    }
    let i = gb_checks(data)?;
    validate_gb(&i)?;
    let cart_type = i["cartridge_type"].as_u64().unwrap_or(255);
    let ram = i["ram_size_code"].as_u64().unwrap_or(255);
    if ["size_valid", "global_checksum_valid"]
        .iter()
        .any(|k| i[k] != true)
        || !matches!(cart_type, 0x19..=0x1b)
        || (cart_type == 0x19 && ram != 0)
        || (cart_type != 0x19 && !matches!(ram, 2 | 3))
        || data.len() > SPANSION_CAPACITY
    {
        return Err(Error::new("ROM_INCOMPATIBLE",
            "This ROM does not match the S29GL032M R4 / MBC5 board.",
            "Use an MBC5 ROM (type 0x19–0x1B), at most 4 MiB, with no RAM or 8/32 KiB RAM and valid size/header/global checksums. MBC3/RTC, rumble and larger saves are unsupported. Nothing has been erased."));
    }
    Ok(i)
}
pub const LOGO: [u8; 48] = [
    0xce, 0xed, 0x66, 0x66, 0xcc, 0x0d, 0, 0x0b, 3, 0x73, 0, 0x83, 0, 0x0c, 0, 0x0d, 0, 8, 0x11,
    0x1f, 0x88, 0x89, 0, 0x0e, 0xdc, 0xcc, 0x6e, 0xe6, 0xdd, 0xdd, 0xd9, 0x99, 0xbb, 0xbb, 0x67,
    0x63, 0x6e, 0x0e, 0xec, 0xcc, 0xdd, 0xdc, 0x99, 0x9f, 0xbb, 0xb9, 0x33, 0x3e,
];
pub fn sha(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}
pub fn hashes(data: &[u8]) -> Value {
    json!({"sha256":sha(data),"sha1":hex::encode(sha1::Sha1::digest(data)),"crc32":format!("{:08x}",crc32fast::hash(data))})
}
pub fn gb_header(data: &[u8]) -> Result<Value> {
    if data.len() < 0x150 {
        return Err(Error::new(
            "INVALID_GB_ROM",
            "Cartridge header is truncated.",
            "Load a complete, uncompressed .gb or .gbc file.",
        ));
    }
    let size = data[0x148];
    let banks = match size {
        0..=8 => Some(2usize << size),
        0x52 => Some(72),
        0x53 => Some(80),
        0x54 => Some(96),
        _ => None,
    };
    let mapper = match data[0x147] {
        0 | 8 | 9 => Some("ROM"),
        1..=3 => Some("MBC1"),
        5 | 6 => Some("MBC2"),
        0xf..=0x13 => Some("MBC3"),
        0x19..=0x1e => Some("MBC5"),
        _ => None,
    };
    let end = if data[0x143] & 128 != 0 { 0x143 } else { 0x144 };
    let title: String = data[0x134..end]
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| if b.is_ascii() { b as char } else { '�' })
        .collect();
    let checksum = data[0x134..0x14d]
        .iter()
        .fold(0u8, |a, &b| a.wrapping_sub(b).wrapping_sub(1));
    Ok(
        json!({"title":title,"cartridge_type":data[0x147],"mapper":mapper,"cgb_flag":data[0x143],"rom_banks":banks,"rom_bytes":banks.map(|n|n*16384),"ram_size_code":data[0x149],"logo_valid":data[0x104..0x134]==LOGO,"header_checksum_valid":checksum==data[0x14d],"global_checksum_stored":u16::from_be_bytes([data[0x14e],data[0x14f]])}),
    )
}
pub fn validate_gb(info: &Value) -> Result<()> {
    if info["logo_valid"] != true || info["header_checksum_valid"] != true {
        return Err(Error::new("GB_HEADER_UNREADABLE", "No valid Game Boy cartridge header was found.", "Check seating and orientation with USB unplugged, then retry. A blank flash cartridge needs its exact supported board profile; the app will not guess flash wiring.").details(json!({"cartridge":info})));
    }
    let max = match info["mapper"].as_str() {
        Some("ROM") => 2,
        Some("MBC1" | "MBC3") => 128,
        Some("MBC2") => 16,
        Some("MBC5") => 512,
        _ => 0,
    };
    if info["rom_banks"].as_u64().is_none_or(|n| n == 0 || n > max) {
        return Err(Error::new("GB_BOARD_UNSUPPORTED", "The cartridge header describes an unsupported mapper or ROM size.", "Retain the header details when requesting support, or use a reader that supports this cartridge type.").details(json!({"cartridge":info})));
    }
    Ok(())
}
pub fn gb_checks(data: &[u8]) -> Result<Value> {
    let mut info = gb_header(data)?;
    let sum = data
        .iter()
        .fold(0u16, |s, &b| s.wrapping_add(b as u16))
        .wrapping_sub(data[0x14e] as u16)
        .wrapping_sub(data[0x14f] as u16);
    info["actual_bytes"] = json!(data.len());
    info["size_valid"] = json!(info["rom_bytes"] == data.len());
    info["global_checksum_calculated"] = json!(sum);
    info["global_checksum_valid"] = json!(info["global_checksum_stored"] == sum);
    info["sha256"] = json!(sha(data));
    Ok(info)
}
pub fn validate_gb_flash(data: &[u8]) -> Result<Value> {
    let i = gb_checks(data)?;
    validate_gb(&i)?;
    if [
        "logo_valid",
        "header_checksum_valid",
        "size_valid",
        "global_checksum_valid",
    ]
    .iter()
    .any(|k| i[k] != true)
        || i["cartridge_type"] != 0x19
        || i["ram_size_code"] != 0
        || data.len() > CAPACITY
    {
        return Err(Error::new("ROM_INCOMPATIBLE", "This ROM does not match the SST39SF040 AUDIO/MBC5 board.", "Use an MBC5 ROM (type 0x19) without save RAM, up to 512 KiB, with valid header, size and global checksums. Nothing has been erased."));
    }
    Ok(i)
}
#[derive(Clone, Debug)]
pub struct NesRom {
    pub format: &'static str,
    pub mapper: u16,
    pub submapper: u8,
    pub mirroring: &'static str,
    pub battery: bool,
    pub trainer: Vec<u8>,
    pub prg: Vec<u8>,
    pub chr: Vec<u8>,
    pub prg_ram: usize,
    pub prg_nvram: usize,
    pub chr_ram: usize,
    pub chr_nvram: usize,
    pub console: u8,
    pub timing: &'static str,
    pub raw: Vec<u8>,
}
fn invalid(message: impl Into<String>) -> Error {
    Error::new("INVALID_NES_ROM",message,"Use a complete, uncompressed .nes file with an accurate iNES or NES 2.0 header. Renaming the file does not convert it.").exit(4)
}
fn rom_size(low: u8, high: u8, unit: usize) -> Result<usize> {
    if high != 15 {
        Ok((((high as usize) << 8) | low as usize) * unit)
    } else if low >> 2 > 25 {
        Err(invalid("The header declares an unsupported ROM size."))
    } else {
        Ok((1usize << (low >> 2)) * ((low as usize & 3) * 2 + 1))
    }
}
fn ram_size(n: u8) -> usize {
    if n == 0 {
        0
    } else {
        64usize << n
    }
}
impl NesRom {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 16 || &data[..4] != b"NES\x1a" {
            return Err(invalid("This is not a headered NES/Famicom ROM."));
        }
        let h = &data[..16];
        let kind = h[7] & 12;
        if kind != 0 && kind != 8 {
            return Err(invalid("Unsupported or ambiguous legacy NES header."));
        }
        let n = kind == 8;
        if !n && (h[12..16].iter().any(|&v| v != 0) || h[9] & !1 != 0 || h[10] != 0 || h[11] != 0) {
            return Err(invalid("The legacy header has ambiguous nonstandard fields. Re-export with an accurate NES 2.0 header."));
        }
        let mapper = (h[6] >> 4) as u16
            | (h[7] & 0xf0) as u16
            | if n { ((h[8] & 15) as u16) << 8 } else { 0 };
        let p = if n {
            rom_size(h[4], h[9] & 15, 16384)?
        } else {
            h[4] as usize * 16384
        };
        let c = if n {
            rom_size(h[5], h[9] >> 4, 8192)?
        } else {
            h[5] as usize * 8192
        };
        if p == 0 || p > 32 * 1024 * 1024 || c > 32 * 1024 * 1024 {
            return Err(invalid(
                "The ROM declares an empty or unsupported PRG/CHR size.",
            ));
        }
        let t = if h[6] & 4 != 0 { 512 } else { 0 };
        if data.len() != 16 + t + p + c {
            return Err(invalid(format!("The header requires {} bytes, but the file contains {}. It is truncated or has unsupported extra data.",16+t+p+c,data.len())));
        }
        if n && (h[12] & 0xfc != 0
            || h[14] != 0
            || h[15] & 0xc0 != 0
            || (h[7] & 3 == 0 && h[13] != 0))
        {
            return Err(invalid(
                "The NES 2.0 file declares unsupported extra data or reserved fields.",
            ));
        }
        let battery = h[6] & 2 != 0;
        let mirror = if mapper == 30 && h[6] & 9 == 8 {
            "one-screen"
        } else if h[6] & 8 != 0 {
            "four-screen"
        } else if h[6] & 1 != 0 {
            "vertical"
        } else {
            "horizontal"
        };
        Ok(Self {
            format: if n { "NES 2.0" } else { "iNES" },
            mapper,
            submapper: if n { h[8] >> 4 } else { 0 },
            mirroring: mirror,
            battery,
            trainer: data[16..16 + t].to_vec(),
            prg: data[16 + t..16 + t + p].to_vec(),
            chr: data[16 + t + p..].to_vec(),
            prg_ram: if n {
                ram_size(h[10] & 15)
            } else if !battery {
                h[8] as usize * 8192
            } else {
                0
            },
            prg_nvram: if n {
                ram_size(h[10] >> 4)
            } else if battery {
                h[8] as usize * 8192
            } else {
                0
            },
            chr_ram: if n {
                ram_size(h[11] & 15)
            } else if c == 0 {
                if mapper == 30 {
                    32768
                } else {
                    8192
                }
            } else {
                0
            },
            chr_nvram: if n { ram_size(h[11] >> 4) } else { 0 },
            console: h[7] & 3,
            timing: if n {
                ["NTSC", "PAL", "dual", "Dendy"][(h[12] & 3) as usize]
            } else if h[9] & 1 != 0 {
                "PAL"
            } else {
                "NTSC"
            },
            raw: data.to_vec(),
        })
    }
    pub fn summary(&self) -> Value {
        json!({"format":self.format,"mapper":self.mapper,"submapper":self.submapper,"mirroring":self.mirroring,"battery":self.battery,"trainer_bytes":self.trainer.len(),"prg_bytes":self.prg.len(),"chr_bytes":self.chr.len(),"prg_ram":self.prg_ram,"prg_nvram":self.prg_nvram,"chr_ram":self.chr_ram,"chr_nvram":self.chr_nvram,"console":self.console,"timing":self.timing,"sha256":sha(&self.raw)})
    }
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct Board {
    pub id: &'static str,
    pub name: &'static str,
    pub mapper: u16,
    pub prg_capacity: usize,
    pub chr_capacity: usize,
    pub chr_ram: usize,
    pub writable: bool,
    pub flash_id: Option<&'static str>,
}
pub const BOARDS: [Board; 3] = [
    Board {
        id: "broke-unrom512",
        name: "Broke Studio UNROM-512 v2.1 (mapper 30)",
        mapper: 30,
        prg_capacity: CAPACITY,
        chr_capacity: 0,
        chr_ram: 32768,
        writable: true,
        flash_id: Some("bfb7"),
    },
    Board {
        id: "nrom-128",
        name: "NROM-128 (read-only profile)",
        mapper: 0,
        prg_capacity: 16384,
        chr_capacity: 8192,
        chr_ram: 0,
        writable: false,
        flash_id: None,
    },
    Board {
        id: "nrom-256",
        name: "NROM-256 (read-only profile)",
        mapper: 0,
        prg_capacity: 32768,
        chr_capacity: 8192,
        chr_ram: 0,
        writable: false,
        flash_id: None,
    },
];
pub fn board(id: &str) -> Result<Board> {
    BOARDS.iter().find(|b|b.id==id).copied().ok_or_else(||Error::new("BOARD_PROFILE_REQUIRED",format!("Unknown or missing board profile: {id}."),"Detect a supported board, or select its known physical model. A ROM file cannot identify the connected board."))
}
pub fn compatibility(r: &NesRom, b: Board, mirror: Option<&str>, write: bool) -> Result<Value> {
    let mut reasons = Vec::<String>::new();
    let mut warnings = Vec::new();
    if write && !b.writable {
        reasons.push("This is a read-only profile; its flash hardware has not been qualified for erase/write.".into());
    }
    if r.mapper != b.mapper {
        reasons.push(format!(
            "ROM requires mapper {}; this board implements mapper {}.",
            r.mapper, b.mapper
        ));
    }
    if r.submapper > 1 || (r.mapper == 0 && r.submapper != 0) {
        reasons.push(format!(
            "ROM requires unsupported submapper {}.",
            r.submapper
        ));
    }
    if r.mapper == 30 && r.submapper == 0 && !r.battery {
        reasons.push("ROM declares a bus-conflict UNROM-512 variant. Use a build targeting this flashable board (NES 2.0 submapper 1).".into());
    }
    if r.console != 0 {
        reasons.push("ROM targets an extended console rather than standard Famicom/NES.".into());
    }
    if !r.trainer.is_empty() {
        reasons.push("ROM contains a trainer, which this cartridge cannot install.".into());
    }
    if r.prg.len() > b.prg_capacity {
        reasons.push(format!(
            "ROM needs {} KiB of program memory; the cartridge has {} KiB.",
            r.prg.len() / 1024,
            b.prg_capacity / 1024
        ));
    }
    if r.prg.len() < 16384 || !r.prg.len().is_power_of_two() {
        reasons.push("Program size must be a power of two, starting at 16 KiB.".into());
    }
    if r.chr.len() > b.chr_capacity {
        reasons.push(format!("ROM needs {} KiB of CHR ROM; the board has {} KiB CHR ROM and {} KiB CHR RAM. Those memory types are not interchangeable.",r.chr.len()/1024,b.chr_capacity/1024,b.chr_ram/1024));
    }
    if r.chr_ram > b.chr_ram || r.chr_nvram != 0 {
        reasons.push(
            "ROM requires more graphics RAM or persistent graphics RAM than this board provides."
                .into(),
        );
    }
    if r.prg_ram != 0 || r.prg_nvram != 0 {
        reasons.push("ROM requires separate program/save RAM. This board has none; flash-backed saves are different.".into());
    }
    if let Some(m) = mirror {
        if r.mirroring != m {
            reasons.push(format!(
                "ROM requires {} mirroring; the cartridge is wired for {m} mirroring.",
                r.mirroring
            ));
        }
    }
    if r.mirroring == "four-screen" {
        reasons.push("Four-screen nametable memory is unsupported by this profile.".into());
    }
    let reset = u16::from_le_bytes([r.prg[r.prg.len() - 4], r.prg[r.prg.len() - 3]]);
    if !(0x8000..0xffff).contains(&reset) {
        reasons.push(format!(
            "ROM has an invalid reset vector (${reset:04X}); it would not boot after writing."
        ));
    }
    if r.timing != "NTSC" && r.timing != "dual" {
        warnings.push(format!(
            "ROM timing is {}; use a matching console.",
            r.timing
        ));
    }
    if !reasons.is_empty() {
        return Err(Error::new("ROM_INCOMPATIBLE",reasons.join("\n"),"Use a ROM built for this physical board. For a mirroring mismatch use a matching ROM or adjust the solder jumper with USB disconnected. Changing the mapper number does not convert a game. Nothing has been erased.").details(json!({"board":b,"rom":r.summary(),"reasons":reasons})).exit(4));
    }
    Ok(
        json!({"rom_matches_profile":true,"hardware_verified":mirror.is_some(),"warnings":warnings,"prg_repetitions":b.prg_capacity/r.prg.len()}),
    )
}
pub fn nes_build(prg: &[u8], chr: &[u8], b: Board, mirror: &str) -> Result<Vec<u8>> {
    if !prg.len().is_multiple_of(16384) || !chr.len().is_multiple_of(8192) {
        return Err(invalid("PRG/CHR data must use whole banks."));
    }
    let mode = match mirror {
        "horizontal" => 0,
        "vertical" => 1,
        "one-screen" => 8,
        "four-screen" => 9,
        _ => return Err(invalid("Unknown mirroring mode.")),
    };
    let mut h = vec![0; 16];
    h[..4].copy_from_slice(b"NES\x1a");
    let p = prg.len() / 16384;
    let c = chr.len() / 8192;
    h[4] = p as u8;
    h[5] = c as u8;
    h[6] = ((b.mapper as u8 & 15) << 4) | mode | if b.writable { 2 } else { 0 };
    h[7] = (b.mapper as u8 & 0xf0) | 8;
    h[8] = (b.mapper >> 8) as u8 & 15
        | if b.mapper == 30 && b.writable {
            0x10
        } else {
            0
        };
    h[9] = ((p >> 8) | ((c >> 8) << 4)) as u8;
    if b.chr_ram > 0 {
        h[11] = (b.chr_ram.ilog2() - 6) as u8;
    }
    h.extend(prg);
    h.extend(chr);
    Ok(h)
}
pub fn inspect(path: &Path, data: &[u8]) -> Result<Value> {
    let mut r = json!({"path":path,"name":path.file_name().unwrap_or_default().to_string_lossy(),"bytes":data.len(),"hashes":hashes(data),"issues":[]});
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    if data.starts_with(b"NES\x1a") {
        let n = NesRom::parse(data)?;
        r["platform"] = json!("famicom");
        r["title"] = json!(stem);
        r["format"] = json!(n.format);
        r["mapper"] = json!(format!("Mapper {}", n.mapper));
        r["metadata"] = n.summary();
        r["checksum"] =
            json!("NES ROMs have no built-in checksum. Compare a known hash or independent reads.");
    } else if path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("gba"))
        || crate::gba::header(data).is_ok_and(|h| h["logo_valid"] == true)
    {
        let info = crate::gba::header(data)?;
        let mut issues = Vec::new();
        if crate::gba::validate(&info).is_err() {
            issues.push("The GBA header or boot logo is invalid.");
        }
        if data.len() < 0xc0 || data.len() > crate::gba::MAX_SIZE || data.len() & 1 != 0 {
            issues.push("The GBA ROM size is invalid.");
        }
        r["platform"] = json!("gba");
        r["format"] = json!("Game Boy Advance");
        r["title"] = if info["title"] == "" {
            json!(stem)
        } else {
            info["title"].clone()
        };
        r["mapper"] = json!("Linear GBA ROM");
        r["checksum"] = json!(if issues.is_empty() {
            "Header checksum matches. GBA has no built-in whole-ROM checksum; compare file hashes or read twice.".to_string()
        } else {
            issues.join(" ")
        });
        r["issues"] = json!(issues);
        r["metadata"] = info;
    } else if matches!(
        path.extension()
            .map(|s| s.to_string_lossy().to_lowercase())
            .as_deref(),
        Some("gb" | "gbc")
    ) || data.get(0x104..0x134) == Some(&LOGO)
    {
        let i = gb_checks(data)?;
        r["platform"] = json!("gameboy");
        r["title"] = if i["title"] == "" {
            json!(stem)
        } else {
            i["title"].clone()
        };
        r["format"] = json!(if i["cgb_flag"].as_u64().unwrap_or(0) & 128 != 0 {
            "Game Boy Color"
        } else {
            "Game Boy"
        });
        r["mapper"] = i["mapper"]
            .as_str()
            .map(|s| json!(s))
            .unwrap_or(json!(format!(
                "Unsupported type 0x{:02x}",
                i["cartridge_type"].as_u64().unwrap()
            )));
        let mut issues = Vec::new();
        for (k, l) in [
            ("logo_valid", "Boot logo"),
            ("header_checksum_valid", "Header checksum"),
            ("size_valid", "Declared ROM size"),
            ("global_checksum_valid", "Global checksum"),
        ] {
            if i[k] != true {
                issues.push(format!("{l} does not match."));
            }
        }
        r["checksum"] = json!(if issues.is_empty() {
            "Header and global checksums match.".into()
        } else {
            issues.join(" ")
        });
        r["issues"] = json!(issues);
        r["metadata"] = i;
    } else {
        return Err(Error::new("UNRECOGNIZED_ROM","This file is not a supported cartridge ROM.","Extract archives first, then select a .gb, .gbc, .gba, or headered .nes file. Renaming a file does not convert it."));
    }
    Ok(r)
}
pub fn match_hash(info: &Value, expected: &str) -> Result<Value> {
    let e = expected.trim().to_lowercase();
    let a = match e.len() {
        8 => "crc32",
        40 => "sha1",
        64 => "sha256",
        _ => "",
    };
    if a.is_empty() || !e.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::new(
            "INVALID_CHECKSUM",
            "Enter an 8-character CRC32, 40-character SHA-1, or 64-character SHA-256.",
            "Paste only the hexadecimal checksum, without a filename or prefix.",
        ));
    }
    if info["hashes"][a] != e {
        return Err(Error::new(
            "CHECKSUM_MISMATCH",
            format!("The file's {a} does not match the expected checksum."),
            "Check that the checksum is for this exact ROM version and file format.",
        )
        .details(json!({"expected":e,"actual":info["hashes"][a]})));
    }
    Ok(
        json!({"message":format!("{} matches the expected value.",match a{"sha256"=>"SHA-256","sha1"=>"SHA-1",_=>"CRC32"}),"algorithm":a}),
    )
}
