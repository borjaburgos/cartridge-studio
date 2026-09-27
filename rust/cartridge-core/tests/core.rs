use cartridge_core::{
    rom::{self, BOARDS, LOGO},
    service::{self, Request},
    storage::Cancel,
    usb::{Bus, Transport},
    Error, Result,
};
use serde_json::{json, Value};
use std::{cell::RefCell, path::Path, rc::Rc};
fn gb() -> Vec<u8> {
    let mut d = vec![0; 32768];
    d[0x104..0x134].copy_from_slice(&LOGO);
    d[0x134..0x13d].copy_from_slice(b"CART TEST");
    d[0x147] = 0x19;
    d[0x14d] = d[0x134..0x14d]
        .iter()
        .fold(0u8, |a, &b| a.wrapping_sub(b).wrapping_sub(1));
    let sum = d.iter().map(|&b| b as u32).sum::<u32>() as u16;
    d[0x14e..0x150].copy_from_slice(&sum.to_be_bytes());
    d
}
fn nes() -> Vec<u8> {
    let mut p: Vec<u8> = (0..32768).map(|n| n as u8).collect();
    p[32762..].copy_from_slice(&[0, 0xc0, 0, 0xc0, 0, 0xc0]);
    rom::nes_build(&p, &[], BOARDS[0], "vertical").unwrap()
}
#[test]
fn gb_checks_and_write_validation() {
    let d = gb();
    let i = rom::inspect(Path::new("test.gb"), &d).unwrap();
    assert_eq!(i["issues"], json!([]));
    rom::validate_gb_flash(&d).unwrap();
    for a in ["sha1", "sha256", "crc32"] {
        rom::match_hash(&i, &i["hashes"][a].as_str().unwrap().to_uppercase()).unwrap();
    }
    let mut bad = d.clone();
    bad[2000] ^= 1;
    assert_eq!(
        rom::validate_gb_flash(&bad).unwrap_err().code,
        "ROM_INCOMPATIBLE"
    );
    assert_eq!(
        rom::match_hash(&i, "hello").unwrap_err().code,
        "INVALID_CHECKSUM"
    );
    assert_eq!(
        rom::match_hash(&i, &"0".repeat(64)).unwrap_err().code,
        "CHECKSUM_MISMATCH"
    );
}
#[test]
fn nes_strict_formats_and_fixed_bank() {
    for m in ["horizontal", "vertical", "one-screen"] {
        let mut data = nes();
        let r = rom::NesRom::parse(&data).unwrap();
        data = rom::nes_build(&r.prg, &[], BOARDS[0], m).unwrap();
        let r = rom::NesRom::parse(&data).unwrap();
        rom::compatibility(&r, BOARDS[0], Some(m), true).unwrap();
        assert_eq!(
            r.prg.repeat(524288 / r.prg.len())[524288 - 16384..],
            r.prg[r.prg.len() - 16384..]
        );
        data[7] &= !8;
        data[8..16].fill(0);
        assert_eq!(rom::NesRom::parse(&data).unwrap().chr_ram, 32768);
    }
}
#[test]
fn both_nes_connectors_check_the_same_rom_offline() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("test.nes");
    std::fs::write(&path, nes()).unwrap();
    for slot in ["nes", "famicom"] {
        let mut request = Request::new("check");
        request.platform = slot.into();
        request.profile = BOARDS[0].id.into();
        request.source = Some(path.clone());
        let result = service::run_with(&request, Cancel::default(), &mut |_| {}, &mut |_| {
            panic!("Offline check must not open USB")
        })
        .unwrap();
        assert_eq!(result["slot"], slot);
        assert_eq!(result["rom"]["platform"], "famicom");
    }
}
#[test]
fn invalid_headers_never_pass() {
    let d = nes();
    for n in [0, 1, 4, 15, 16, d.len() - 1] {
        assert!(rom::NesRom::parse(&d[..n]).is_err());
    }
    for (pos, value) in [(7, 4), (12, 0xfc), (14, 1), (15, 0xc0)] {
        let mut b = d.clone();
        b[pos] = value;
        assert!(rom::NesRom::parse(&b).is_err());
    }
    let mut b = d.clone();
    b.push(0);
    assert!(rom::NesRom::parse(&b).is_err());
    let mut b = d;
    b[8] = 0x20;
    assert_eq!(
        rom::compatibility(&rom::NesRom::parse(&b).unwrap(), BOARDS[0], None, true)
            .unwrap_err()
            .code,
        "ROM_INCOMPATIBLE"
    );
}
#[test]
fn preflight_rejects_before_usb() {
    let temp = tempfile::tempdir().unwrap();
    let p = temp.path().join("test.nes");
    std::fs::write(&p, nes()).unwrap();
    let mut r = Request::new("write");
    r.platform = "famicom".into();
    r.profile = BOARDS[0].id.into();
    r.source = Some(p);
    r.source_sha256 = Some("0".repeat(64));
    r.data_directory = Some(temp.path().into());
    let mut open = |_| panic!("USB must not be opened");
    assert_eq!(
        service::run_with(&r, Cancel::default(), &mut |_| {}, &mut open)
            .unwrap_err()
            .code,
        "CONFIRMATION_REQUIRED"
    );
    r.confirmed = true;
    assert_eq!(
        service::run_with(&r, Cancel::default(), &mut |_| {}, &mut open)
            .unwrap_err()
            .code,
        "SOURCE_CHANGED"
    );
    r.source_sha256 = Some(rom::sha(&nes()));
    r.profile = "nrom-128".into();
    assert_eq!(
        service::run_with(&r, Cancel::default(), &mut |_| {}, &mut open)
            .unwrap_err()
            .code,
        "BOARD_READ_ONLY"
    );
    r.action = "verify".into();
    r.source = None;
    assert_eq!(
        service::run_with(&r, Cancel::default(), &mut |_| {}, &mut open)
            .unwrap_err()
            .code,
        "SOURCE_REQUIRED"
    );
}
#[test]
fn gbxcart_write_checks_source_profile_and_confirmation_before_open() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("game.gbc");
    let original = gb();
    std::fs::write(&path, &original).unwrap();
    let mut req = Request::new("write");
    req.reader = cartridge_core::readers::Kind::Gbxcart;
    req.platform = "gameboy".into();
    req.profile = rom::GB_PROFILE.into();
    req.source = Some(path.clone());
    req.source_sha256 = Some(rom::sha(&original));
    req.data_directory = Some(temp.path().into());
    let mut unopened = |_| panic!("No reader may be opened before preflight passes");
    assert_eq!(
        service::run_with(&req, Cancel::default(), &mut |_| {}, &mut unopened)
            .unwrap_err()
            .code,
        "CONFIRMATION_REQUIRED"
    );
    req.confirmed = true;
    req.source_sha256 = Some("0".repeat(64));
    assert_eq!(
        service::run_with(&req, Cancel::default(), &mut |_| {}, &mut unopened)
            .unwrap_err()
            .code,
        "SOURCE_CHANGED"
    );
    req.source_sha256 = Some(rom::sha(&original));
    req.profile = "auto".into();
    assert_eq!(
        service::run_with(&req, Cancel::default(), &mut |_| {}, &mut unopened)
            .unwrap_err()
            .code,
        "READER_OPERATION_UNSUPPORTED"
    );
    req.profile = rom::GB_PROFILE.into();
    let mut bad = original.clone();
    bad[2000] ^= 1;
    std::fs::write(&path, &bad).unwrap();
    req.source_sha256 = Some(rom::sha(&bad));
    assert_eq!(
        service::run_with(&req, Cancel::default(), &mut |_| {}, &mut unopened)
            .unwrap_err()
            .code,
        "ROM_INCOMPATIBLE"
    );
    std::fs::write(&path, &original).unwrap();
    req.source_sha256 = Some(rom::sha(&original));
    req.directory = Some(temp.path().join("pinned"));
    let error = service::run_with(&req, Cancel::default(), &mut |_| {}, &mut |_| {
        assert_eq!(
            std::fs::read(temp.path().join("pinned/reviewed-source.gb")).unwrap(),
            original
        );
        Err(Error::new(
            "SERIAL_OPEN_FAILED",
            "Busy",
            "Close the other application",
        ))
    })
    .unwrap_err();
    assert_eq!(error.code, "SERIAL_OPEN_FAILED");
    assert!(error.details["backup_directory"].is_string());
}
#[derive(Default)]
struct Memory {
    data: Vec<u8>,
    bank: usize,
    base: usize,
    offset: usize,
    trace: Vec<(u8, u16, u16)>,
    payloads: usize,
    fault: bool,
}
struct Fake(Rc<RefCell<Memory>>);
impl Transport for Fake {
    fn identity(&self) -> Value {
        json!({"product":"TEST"})
    }
    fn output(&mut self, _: u8, _: u16, _: u16, _: &[u8]) -> Result<()> {
        panic!("Read cannot program")
    }
    fn input(&mut self, d: u8, v: u16, a: u16, len: u16) -> Result<Vec<u8>> {
        let mut s = self.0.borrow_mut();
        s.trace.push((d, v, a));
        let o = v as u8;
        let misc = (v >> 8) as u8;
        let mut r = vec![0; len as usize];
        if len > 1 {
            r[1] = (len - 2) as u8;
        }
        match (d, o) {
            (12, 0) => {
                r[2] = s.data[a as usize];
            }
            (12, 1) => {
                assert!([0x2000, 0x2100, 0x3000, 0x4000, 0x6000].contains(&a));
                if a == 0x2000 {
                    s.bank = misc as usize;
                }
            }
            (5, 0x32) => s.base = if a == 0 { 0 } else { s.bank * 16384 },
            (7, 0) if a == 0xd2 => s.offset = 0,
            (5, 0x61) => r[2] = 0xd8,
            (5, 0x70) => {
                s.payloads += 1;
                if s.fault && s.payloads == 257 {
                    return Err(Error::new(
                        "READER_DISCONNECTED",
                        "Disconnected",
                        "Reconnect",
                    ));
                }
                r = s.data[s.base + s.offset..s.base + s.offset + len as usize].to_vec();
                s.offset += len as usize;
            }
            (2, _) | (5, _) | (7, _) => (),
            _ => panic!("unexpected command {d} {o} {a}"),
        };
        Ok(r)
    }
}
#[test]
fn buffered_gb_read_is_verified_and_read_only() {
    let temp = tempfile::tempdir().unwrap();
    let state = Rc::new(RefCell::new(Memory {
        data: gb(),
        ..Default::default()
    }));
    let mut req = Request::new("read");
    req.platform = "gameboy".into();
    req.data_directory = Some(temp.path().into());
    req.download_artwork = false;
    let mut open = |c| {
        Ok(Bus {
            transport: Box::new(Fake(state.clone())),
            cancel: c,
        })
    };
    let report = service::run_with(&req, Cancel::default(), &mut |_| {}, &mut open).unwrap();
    assert_eq!(report["status"], "complete");
    assert_eq!(report["identical_reads"], true);
    assert_eq!(
        std::fs::read(report["output"].as_str().unwrap()).unwrap(),
        gb()
    );
    assert_eq!(state.borrow().payloads, 512);
    assert_eq!(state.borrow().trace.last().unwrap(), &(2, 0, 0));
}
#[test]
fn disconnected_read_retains_raw_bytes_and_journal() {
    let temp = tempfile::tempdir().unwrap();
    let state = Rc::new(RefCell::new(Memory {
        data: gb(),
        fault: true,
        ..Default::default()
    }));
    let mut req = Request::new("read");
    req.platform = "gameboy".into();
    req.directory = Some(temp.path().join("backup"));
    req.download_artwork = false;
    let mut open = |c| {
        Ok(Bus {
            transport: Box::new(Fake(state.clone())),
            cancel: c,
        })
    };
    let e = service::run_with(&req, Cancel::default(), &mut |_| {}, &mut open).unwrap_err();
    assert_eq!(e.code, "READER_DISCONNECTED");
    assert_eq!(
        std::fs::read(temp.path().join("backup/read1.bin")).unwrap(),
        gb()
    );
    let r: Value =
        serde_json::from_slice(&std::fs::read(temp.path().join("backup/report.json")).unwrap())
            .unwrap();
    assert_eq!(r["status"], "failed");
    assert_eq!(r["failed_during"], "reading");
    assert!(!temp.path().join("backup/cartridge.gb").exists());
    assert_eq!(state.borrow().trace.last().unwrap(), &(2, 0, 0));
}
#[test]
fn cancellation_prevents_open() {
    let c = Cancel::default();
    c.0.store(true, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        service::run_with(&Request::new("doctor"), c, &mut |_| {}, &mut |_| panic!(
            "must not open"
        ))
        .unwrap_err()
        .code,
        "INTERRUPTED"
    );
}
#[test]
fn unknown_json_options_are_rejected() {
    assert!(serde_json::from_value::<Request>(json!({"action":"write","confirmd":true})).is_err());
}

#[test]
fn single_read_does_not_claim_two_reads_but_backup_always_uses_two() {
    for action in ["read", "backup"] {
        let temp = tempfile::tempdir().unwrap();
        let state = Rc::new(RefCell::new(Memory {
            data: gb(),
            ..Default::default()
        }));
        let mut req = Request::new(action);
        req.platform = "gameboy".into();
        req.data_directory = Some(temp.path().into());
        req.double_read = false;
        req.download_artwork = false;
        let report = service::run_with(&req, Cancel::default(), &mut |_| {}, &mut |c| {
            Ok(Bus {
                transport: Box::new(Fake(state.clone())),
                cancel: c,
            })
        })
        .unwrap();
        assert_eq!(report["read_passes"], if action == "read" { 1 } else { 2 });
        assert_eq!(
            report["identical_reads"],
            if action == "read" {
                Value::Null
            } else {
                json!(true)
            }
        );
    }
}
#[test]
fn source_snapshot_is_pinned_before_open_and_usb_failure_is_journaled() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("input.nes");
    let original = nes();
    std::fs::write(&path, &original).unwrap();
    let mut req = Request::new("write");
    req.platform = "famicom".into();
    req.profile = BOARDS[0].id.into();
    req.source = Some(path.clone());
    req.source_sha256 = Some(rom::sha(&original));
    req.confirmed = true;
    req.directory = Some(temp.path().join("op"));
    let e = service::run_with(&req, Cancel::default(), &mut |_| {}, &mut |_| {
        std::fs::write(&path, b"replaced after review").unwrap();
        assert_eq!(
            std::fs::read(temp.path().join("op/reviewed-source.nes")).unwrap(),
            original
        );
        Err(Error::new(
            "USB_PERMISSION_DENIED",
            "Denied",
            "Install the access rule",
        ))
    })
    .unwrap_err();
    assert_eq!(e.code, "USB_PERMISSION_DENIED");
    assert!(e.details["backup_directory"].is_string());
    let r: Value =
        serde_json::from_slice(&std::fs::read(temp.path().join("op/report.json")).unwrap())
            .unwrap();
    assert_eq!(r["status"], "failed");
}
