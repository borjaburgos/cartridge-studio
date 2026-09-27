//! Read-only, linear GBA ROM access. Save memory and flash commands are excluded.
use crate::{rom, Error, Result};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};

pub const MAX_SIZE: usize = 32 * 1024 * 1024;
pub trait RomReader {
    fn initialize(&mut self) -> Result<()>;
    fn prepare(&mut self, _rom_bytes: usize) -> Result<()> {
        Ok(())
    }
    fn read(&mut self, address: usize, length: usize) -> Result<Vec<u8>>;
    fn identity(&self) -> Value;
    fn close(&mut self) -> Result<()>;
}
pub fn header(data: &[u8]) -> Result<Value> {
    if data.len() < 0xc0 {
        return Err(Error::new(
            "INVALID_GBA_ROM",
            "The Game Boy Advance header is incomplete.",
            "Choose a complete, uncompressed .gba file.",
        ));
    }
    let text = |range: std::ops::Range<usize>| -> String {
        data[range]
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| {
                if b.is_ascii_graphic() || b == b' ' {
                    b as char
                } else {
                    '�'
                }
            })
            .collect::<String>()
            .trim_end()
            .to_owned()
    };
    let sum = data[0xa0..0xbd]
        .iter()
        .fold(0u8, |sum, b| sum.wrapping_sub(*b))
        .wrapping_sub(0x19);
    Ok(
        json!({"title":text(0xa0..0xac),"game_code":text(0xac..0xb0),"maker_code":text(0xb0..0xb2),"version":data[0xbc],"logo_valid":hex::encode(Sha1::digest(&data[4..0xa0])) == "17daa0fec02fc33c0f6abb549a8b80b6613b48ee","fixed_byte_valid":data[0xb2]==0x96,"unit_code":data[0xb3],"header_checksum_stored":data[0xbd],"header_checksum_calculated":sum,"header_checksum_valid":sum==data[0xbd]}),
    )
}
pub fn validate(info: &Value) -> Result<()> {
    if info["logo_valid"] != true
        || info["fixed_byte_valid"] != true
        || info["header_checksum_valid"] != true
        || info["unit_code"] != 0
    {
        return Err(Error::new("GBA_HEADER_UNREADABLE", "No valid retail Game Boy Advance header was found.", "Unplug USB, check that a GBA cartridge is seated in the GBA connector, then reconnect and retry. SD-card flash cartridges and special banked boards are not supported.").details(json!({"cartridge":info})));
    }
    Ok(())
}
pub fn detect(r: &mut dyn RomReader) -> Result<Value> {
    r.initialize()?;
    let bytes = r.read(0, 0x100)?;
    if bytes.iter().all(|&byte| byte == 0xff) || bytes.iter().all(|&byte| byte == 0) {
        return Err(Error::new(
            "GBA_CARTRIDGE_NOT_RESPONDING",
            "The GBA cartridge bus returned only idle bytes.",
            "Unplug USB, remove and firmly reseat the GBA cartridge in the reader's 32-pin GB/GBA connector, then reconnect and retry. Check the cartridge orientation and clean its contacts if needed. Do not press either programmer button.",
        ).details(json!({"sample":hex::encode(&bytes[..16]),"device":r.identity()})));
    }
    let info = header(&bytes)?;
    validate(&info)?;
    Ok(
        json!({"device":r.identity(),"cartridge":info,"writable":false,"message":format!("{} · {} · Game Boy Advance",info["title"].as_str().unwrap(),info["game_code"].as_str().unwrap()),"summary":format!("{} · {}",info["title"].as_str().unwrap(),info["game_code"].as_str().unwrap()),"limitation":"Read-only GBA ROM · 3.3 V · saved games excluded","header_sha256":rom::sha(&bytes)}),
    )
}

/// GBA has no header size field. Mirroring supplies an estimate, never a game identity.
pub fn size(r: &mut dyn RomReader, override_bytes: Option<usize>) -> Result<(usize, &'static str)> {
    if let Some(n) = override_bytes {
        validate_size(n)?;
        return Ok((n, "user_selected"));
    }
    for n in (18..25).map(|shift| 1usize << shift) {
        let mut mirrored = true;
        for offset in [0, 0x1000, (n / 3) & !1, n / 2, n - 256] {
            let low = r.read(offset, 256)?;
            let high = r.read(n + offset, 256)?;
            if low != high {
                mirrored = false;
                break;
            }
        }
        if mirrored {
            return Ok((n, "sampled_mirroring_estimate"));
        }
    }
    Ok((MAX_SIZE, "full_address_window"))
}
pub fn validate_size(n: usize) -> Result<()> {
    if !(256 * 1024..=MAX_SIZE).contains(&n) || !n.is_power_of_two() {
        return Err(Error::new("GBA_SIZE_INVALID", "GBA read size must be a power of two between 256 KiB and 32 MiB.", "Use automatic sizing or select a supported ROM size. GBA headers do not declare their ROM size."));
    }
    Ok(())
}

pub fn read_rom(
    mut r: Box<dyn RomReader>,
    j: &mut crate::operations::Journal,
    action: &str,
    source: Option<&[u8]>,
    passes: usize,
    override_bytes: Option<usize>,
    progress: &mut dyn FnMut(String),
) -> Result<Value> {
    use std::{fs::OpenOptions, io::Write};
    let result = (|| {
        // 2 full raw passes, output and optional source snapshot, plus headroom.
        if fs2::available_space(&j.directory)? < 160 * 1024 * 1024 {
            return Err(Error::new(
                "INSUFFICIENT_SPACE",
                "GBA reads need at least 160 MiB of free space for raw reads and verification.",
                "Free space or select another library folder, then retry.",
            ));
        }
        let detection = detect(r.as_mut())?;
        j.report["cartridge"] = detection["cartridge"].clone();
        let hint =
            crate::games::gba_size_hint(detection["cartridge"]["game_code"].as_str().unwrap_or(""));
        let (size, method) =
            if override_bytes.is_none() && hint.is_some_and(|n| validate_size(n).is_ok()) {
                (hint.unwrap(), "catalog_header_hint_pending_hash")
            } else {
                size(r.as_mut(), override_bytes)?
            };
        // DACS exposes its marker at the end of the 32 MiB address window. Smaller
        // known ROMs do not need a second full-window transfer just to test it.
        if size == MAX_SIZE {
            r.prepare(size)?;
            if r.read(0x1ffe000, 12)? == b"AGBFLASHDACS" {
                return Err(Error::new("GBA_BOARD_UNSUPPORTED", "This GBA cartridge uses DACS storage.", "This release reads standard linear GBA ROMs only. Keep this report when requesting support; no unlock or save-memory commands were sent."));
            }
        }
        if source.is_some_and(|s| s.len() != size) {
            return Err(Error::new("GBA_SIZE_MISMATCH", "The selected ROM and cartridge read size differ.", "Check the loaded ROM. If you know its physical size, select the matching GBA read size and retry.").details(json!({"read_bytes":size,"source_bytes":source.map(<[u8]>::len)})));
        }
        j.report["rom_bytes"] = json!(size);
        j.report["size_method"] = json!(method);
        j.report["size_confirmed_by_catalog"] = json!(false);
        j.report["voltage"] = json!(3.3);
        j.stage("reading")?;
        let passes = if action == "read" {
            passes.clamp(1, 2)
        } else {
            2
        };
        let mut first = Vec::with_capacity(size);
        for pass in 1..=passes {
            r.initialize()?;
            r.prepare(size)?;
            let path = j.directory.join(format!("read{pass}.bin"));
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)?;
            let mut differs = false;
            for address in (0..size).step_by(65536) {
                let chunk = r.read(address, (size - address).min(65536))?;
                if chunk.len() != (size - address).min(65536) {
                    return Err(Error::new(
                        "GBA_SHORT_READ",
                        "The reader returned an incomplete GBA ROM block.",
                        "Keep the partial read, reconnect the reader and retry.",
                    ));
                }
                file.write_all(&chunk)?;
                if pass == 1 {
                    first.extend_from_slice(&chunk);
                } else if first[address..address + chunk.len()] != chunk {
                    differs = true;
                }
                if (address + chunk.len()) % (256 * 1024) == 0 || address + chunk.len() == size {
                    progress(format!(
                        "GBA read {pass}/{passes}: {} / {} KiB",
                        (address + chunk.len()) / 1024,
                        size / 1024
                    ));
                }
            }
            file.sync_all()?;
            crate::storage::sync_parent(&path)?;
            if differs {
                return Err(Error::new("BACKUP_MISMATCH", "The two GBA ROM reads differ. Both raw reads are preserved.", "Unplug USB, clean and reseat the cartridge, reconnect and retry. Special cartridges and the EEPROM-overlap area of some 32 MiB games need additional support; bytes are never silently repaired."));
            }
        }
        let info = header(&first)?;
        validate(&info)?;
        if info != detection["cartridge"] {
            return Err(Error::new(
                "GBA_HEADER_CHANGED",
                "The cartridge header changed during the read.",
                "Keep the raw reads, reseat the cartridge with USB unplugged and retry.",
            ));
        }
        if source.is_some_and(|s| s != first) {
            return Err(Error::new("CARTRIDGE_MISMATCH", "The GBA cartridge differs from the loaded ROM.", "Keep the raw reads and report. Load the correct ROM or clean and reseat the cartridge with USB unplugged before retrying."));
        }
        j.report["cartridge"] = info;
        j.report["device"] = r.identity();
        j.report["read_passes"] = json!(passes);
        j.report["identical_reads"] = if passes == 2 {
            json!(true)
        } else {
            Value::Null
        };
        j.report["hashes"] = rom::hashes(&first);
        let identity = crate::games::identify(&first, "gba");
        j.report["size_confirmed_by_catalog"] =
            json!(identity["status"] == "identified" || identity["status"] == "ambiguous");
        if method == "catalog_header_hint_pending_hash" {
            if j.report["size_confirmed_by_catalog"] != true {
                return Err(Error::new("GBA_SIZE_UNCONFIRMED", "The header suggested a catalogued size, but the full ROM checksum did not match.", "Raw reads are preserved. Clean and reseat the cartridge with USB unplugged, then retry. For a modified or uncatalogued cartridge, select 32 MiB to preserve the full address window, or its known physical ROM size."));
            }
            j.report["size_method"] = json!("exact_catalog_hash_and_size");
        }
        j.report["output"] = json!(j.put(
            if action == "verify" {
                "readback.gba"
            } else {
                "cartridge.gba"
            },
            &first
        )?);
        if j.report["size_confirmed_by_catalog"] != true {
            j.report["warning"] = json!("No exact catalog match confirms this GBA image or its size. Automatic sizing uses sampled mirroring; choose 32 MiB to preserve the full address window if the estimate is uncertain. Matching reads establish consistency, not playability. Save memory is not included.");
        }
        if let Some(n) = j.report["device"]["rom_read_retries"]
            .as_u64()
            .filter(|&n| n > 0)
        {
            let note = format!("{n} incomplete USB responses were retried from their original ROM addresses. Final read comparison and catalog results are recorded separately.");
            j.report["transport_note"] = json!(note);
            progress(note);
        }
        j.report["game"] = identity;
        Ok(())
    })();
    let close = r.close();
    j.finish(crate::operations::cleanup(result, close))
}

#[cfg(test)]
pub(crate) fn fixture() -> Vec<u8> {
    let mut data = (0..256 * 1024)
        .map(|n| (((n * 37) ^ ((n >> 8) * 73)) & 255) as u8)
        .collect::<Vec<_>>();
    data[..0xc0].fill(0);
    // Standard boot-logo identification bytes, shared by GBA cartridge headers.
    data[4..0xa0].copy_from_slice(&hex::decode("24ffae51699aa2213d84820a84e409ad11248b98c0817f21a352be199309ce2010464a4af82731ec58c7e83382e3cebf85f4df94ce4b09c194568ac01372a7fc9f844d73a3ca9a615897a327fc039876231dc7610304ae56bf38840040a70efdff52fe036f9530f197fbc08560d68025a963be03014e38e2f9a234ffbb3e0344780090cb88113a9465c07c6387f03cafd625e48b380aac7221d4f807").unwrap());
    data[0xa0..0xac].copy_from_slice(b"STUDIO TEST ");
    data[0xac..0xb0].copy_from_slice(b"TST0");
    data[0xb2] = 0x96;
    data[0xbd] = data[0xa0..0xbd]
        .iter()
        .fold(0u8, |sum, &b| sum.wrapping_sub(b))
        .wrapping_sub(0x19);
    data
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Idle;
    impl RomReader for Idle {
        fn initialize(&mut self) -> Result<()> {
            Ok(())
        }
        fn read(&mut self, _address: usize, length: usize) -> Result<Vec<u8>> {
            Ok(vec![0xff; length])
        }
        fn identity(&self) -> Value {
            json!({"driver":"test"})
        }
        fn close(&mut self) -> Result<()> {
            Ok(())
        }
    }
    #[test]
    fn header_validation_and_offline_inspection_are_gba_specific() {
        let mut bytes = fixture();
        let h = header(&bytes).unwrap();
        validate(&h).unwrap();
        assert_eq!(h["game_code"], "TST0");
        let i = rom::inspect(std::path::Path::new("fixture.GBA"), &bytes).unwrap();
        assert_eq!(i["platform"], "gba");
        assert_eq!(i["issues"], json!([]));
        assert!(header(&bytes[..0xbf]).is_err());
        for offset in [4, 0xa0, 0xb2, 0xbd] {
            bytes[offset] ^= 1;
            assert!(validate(&header(&bytes).unwrap()).is_err());
            bytes[offset] ^= 1;
        }
        assert!(validate_size(3 * 1024 * 1024).is_err());
        assert!(validate_size(MAX_SIZE + 2).is_err());
    }
    #[test]
    fn idle_bus_has_an_actionable_connection_error() {
        let error = detect(&mut Idle).unwrap_err();
        assert_eq!(error.code, "GBA_CARTRIDGE_NOT_RESPONDING");
        assert!(error.action.contains("reseat"));
        assert!(error.action.contains("Do not press"));
    }
    #[test]
    fn full_window_prepares_the_reader_before_rejecting_dacs() {
        struct Dacs {
            prepared: bool,
            header: Vec<u8>,
        }
        impl RomReader for Dacs {
            fn initialize(&mut self) -> Result<()> {
                Ok(())
            }
            fn prepare(&mut self, rom_bytes: usize) -> Result<()> {
                assert_eq!(rom_bytes, MAX_SIZE);
                self.prepared = true;
                Ok(())
            }
            fn read(&mut self, address: usize, length: usize) -> Result<Vec<u8>> {
                if address == 0 {
                    return Ok(self.header[..length].to_vec());
                }
                assert!(self.prepared, "DACS marker read happened before prepare");
                assert_eq!((address, length), (0x1ffe000, 12));
                Ok(b"AGBFLASHDACS".to_vec())
            }
            fn identity(&self) -> Value {
                json!({"driver":"test"})
            }
            fn close(&mut self) -> Result<()> {
                Ok(())
            }
        }
        let root = tempfile::tempdir().unwrap();
        let mut journal =
            crate::operations::Journal::new(&root.path().join("read"), "gba", "backup").unwrap();
        let error = read_rom(
            Box::new(Dacs {
                prepared: false,
                header: fixture(),
            }),
            &mut journal,
            "backup",
            None,
            2,
            Some(MAX_SIZE),
            &mut |_| {},
        )
        .unwrap_err();
        assert_eq!(error.code, "GBA_BOARD_UNSUPPORTED");
        assert_eq!(journal.report["status"], "failed");
    }
    #[test]
    fn unsupported_gba_operations_never_open_hardware() {
        for reader in crate::readers::Kind::ALL {
            for action in ["write", "wipe", "check"] {
                let mut r = crate::service::Request::new(action);
                r.reader = reader;
                r.platform = "gba".into();
                r.confirmed = true;
                let result = crate::service::run_with(
                    &r,
                    crate::storage::Cancel::default(),
                    &mut |_| {},
                    &mut |_| panic!("opened hardware"),
                );
                assert_eq!(result.unwrap_err().code, "GBA_OPERATION_UNSUPPORTED");
            }
        }
    }
}
