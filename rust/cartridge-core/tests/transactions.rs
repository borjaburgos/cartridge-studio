//! Firmware/flash model for fault injection. It accepts the real USB protocol;
//! no driver method is mocked, so transactions exercise banking, buffering and RAM helpers.
use cartridge_core::{
    rom::{self, BOARDS, CAPACITY, LOGO},
    service::{self, Request},
    storage::Cancel,
    usb::{Bus, Transport},
    Error, Result,
};
use serde_json::{json, Value};
use std::{cell::RefCell, path::Path, rc::Rc};
const BASE: u32 = 0x20000500;
#[derive(Default)]
struct Model {
    gb: bool,
    flash: Vec<u8>,
    bank: usize,
    id: bool,
    sequence: Vec<(usize, u8)>,
    ram: Vec<u8>,
    chr: [u8; 4],
    chr_banks: usize,
    latch: u16,
    data: u8,
    mcu: u32,
    buffer_mem: u16,
    buffer_address: u16,
    offset: usize,
    program_bank: usize,
    erases: usize,
    programs: usize,
    reads: usize,
    fault: String,
    trace: Vec<(u8, u16, u16)>,
    cancel: Cancel,
}
impl Model {
    fn new(gb: bool, fault: &str) -> Self {
        let mut m = Self {
            gb,
            flash: (0..CAPACITY)
                .map(|n| ((n / 16384 + n) & 255) as u8)
                .collect(),
            ram: vec![0; 0x1800],
            chr: [11, 12, 13, 14],
            chr_banks: 4,
            fault: fault.into(),
            ..Default::default()
        };
        if fault == "aliased" {
            m.chr_banks = 1;
        }
        m
    }
    fn command(&mut self, address: usize, value: u8) {
        if value == 0xf0 {
            self.id = false;
            self.sequence.clear();
            return;
        }
        self.sequence.push((address, value));
        let s = self.sequence.as_slice();
        if s == [(0x5555, 0xaa), (0x2aaa, 0x55), (0x5555, 0x90)] {
            self.id = self.fault != "no-id";
            self.sequence.clear();
        } else if s
            == [
                (0x5555, 0xaa),
                (0x2aaa, 0x55),
                (0x5555, 0x80),
                (0x5555, 0xaa),
                (0x2aaa, 0x55),
                (0x5555, 0x10),
            ]
        {
            self.erases += 1;
            assert!(
                self.reads >= 2 * 4096,
                "erase requires two complete physical backups"
            );
            self.flash.fill(255);
            if self.fault == "blank" {
                self.flash[88] = 0;
            }
            self.sequence.clear();
        }
    }
    fn rom_byte(&self, a: u16) -> u8 {
        if self.id {
            return [0xbf, 0xb7][a as usize & 1];
        }
        let bank = if self.gb {
            if a < 0x4000 {
                0
            } else {
                self.bank
            }
        } else if a >= 0xc000 {
            if self.fault == "layout" {
                30
            } else {
                31
            }
        } else {
            self.bank & 31
        };
        self.flash[bank * 16384 + (a as usize & 0x3fff)]
    }
    fn mcu_read(&self, a: u32) -> u16 {
        let data = match a {
            0x08000000..0x08000008 => {
                let v = [0x20002000u32.to_le_bytes(), 0x08000101u32.to_le_bytes()].concat();
                let i = (a - 0x08000000) as usize;
                [v[i], v[i + 1]]
            }
            0x48000814 => self.latch.to_le_bytes(),
            0x48000816 => [0, 0],
            0x48000414 => [0, self.data],
            0x48000416 => [0, 0],
            0x20000000..0x20001800 => {
                let i = (a - 0x20000000) as usize;
                [self.ram[i], self.ram[i + 1]]
            }
            _ => panic!("unqualified MCU read {a:x}"),
        };
        u16::from_le_bytes(data)
    }
    fn helper(&mut self) {
        let p = (BASE + 384 - 0x20000000) as usize;
        let magic = u32::from_le_bytes(self.ram[p..p + 4].try_into().unwrap());
        let count = self.ram[p + 7] as usize;
        let status = if magic == 0x4543484f {
            0xec000000
                | self.ram[p + 12..p + 12 + count]
                    .iter()
                    .map(|&b| b as u32)
                    .sum::<u32>()
        } else {
            assert_eq!(magic, 0x53465354);
            self.programs += 1;
            let bank = self.ram[p + 6] as usize;
            let offset = u16::from_le_bytes(self.ram[p + 4..p + 6].try_into().unwrap()) as usize;
            assert!(count <= 116 && offset + count <= 16384 && bank < 32);
            if self.fault != "program" {
                for n in 0..count {
                    self.flash[bank * 16384 + offset + n] &= self.ram[p + 12 + n];
                }
            }
            0x600d0000 | count as u32
        };
        let status = if self.fault == "helper" { 0 } else { status };
        self.ram[p + 8..p + 12].copy_from_slice(&status.to_le_bytes());
    }
}
struct Wire(Rc<RefCell<Model>>);
impl Transport for Wire {
    fn identity(&self) -> Value {
        json!({"product":"FIRMWARE MODEL","firmware_usb":"0203"})
    }
    fn input(&mut self, d: u8, v: u16, a: u16, len: u16) -> Result<Vec<u8>> {
        let mut m = self.0.borrow_mut();
        let o = v as u8;
        let misc = (v >> 8) as u8;
        m.trace.push((d, v, a));
        let mut r = vec![0; len as usize];
        if len > 1 {
            r[1] = (len - 2) as u8;
        }
        match (d, o) {
            (2, 0 | 1 | 5 | 9) => (),
            (12, 0) => r[2] = m.rom_byte(a),
            (12, 1) => {
                if a == 0x2000 {
                    m.bank = (m.bank & 0x100) | misc as usize;
                } else if a == 0x3000 {
                    m.bank = (m.bank & 255) | ((misc as usize) << 8);
                } else {
                    panic!("unexpected GB mapper")
                }
            }
            (3, 0x81) => {
                if m.fault == "unstable"
                    && m.trace
                        .iter()
                        .filter(|&&(d, v, _)| d == 3 && v as u8 == 0x81)
                        .count()
                        == 129
                {
                    r[2] = 0xea;
                } else {
                    r[2] = m.rom_byte(a);
                }
            }
            (3, 2) => {
                if a >= 0xc000 {
                    m.bank = misc as usize;
                } else {
                    let address = (m.bank & 31) * 16384 + (a as usize & 0x3fff);
                    m.command(address, misc);
                }
            }
            (3, 0x82) => r[2] = m.chr[((m.bank >> 5) & 3) % m.chr_banks],
            (3, 1) => {
                let bank = ((m.bank >> 5) & 3) % m.chr_banks;
                m.chr[bank] = misc;
            }
            (3, 0x20) => {
                m.program_bank = a as usize;
                m.offset = 0;
            }
            (1, 17) => m.latch = a,
            (1, 11) => m.data = a as u8,
            (1, 6) => {
                r[2] = u8::from(m.latch == 0x400);
            }
            (1, 4) if a == 15 => {
                let address = m.latch as usize;
                let data = m.data;
                m.command(address, data);
            }
            (1, 1 | 3 | 5 | 8 | 10) => (),
            (5, 0x80..=0x83) if m.gb => {
                let n = (o - 0x80) as usize;
                let descriptor = 0x100 + n * 24;
                let pointer = BASE + (a & 255) as u32 * 32;
                m.ram[descriptor..descriptor + 20].fill(0);
                m.ram[descriptor + 2] = (a >> 8) as u8;
                m.ram[descriptor + 4..descriptor + 8].copy_from_slice(&pointer.to_le_bytes());
                m.ram[descriptor + 8] = 0x7f;
            }
            (5, 0x30) => m.buffer_mem = a,
            (5, 0x32) => m.buffer_address = a,
            (7, 0) if a == 0xd2 => m.offset = 0,
            (5, 0x61) => {
                r[2] = if m.fault == "buffer" {
                    0xee
                } else if m.buffer_mem == 0x10dd {
                    0
                } else {
                    0xd8
                }
            }
            (5, 0x50) => r[3] = 0,
            (5, 0x70) => {
                m.reads += 1;
                if m.fault == "disconnect" && m.erases > 0 {
                    return Err(Error::new(
                        "READER_DISCONNECTED",
                        "Disconnected during verify.",
                        "Restore the retained source.",
                    ));
                }
                if m.fault == "cancel" && m.reads == 4100 {
                    m.cancel.0.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                let bank = if m.gb && m.buffer_address == 0 {
                    0
                } else {
                    m.bank & 31
                };
                let start = bank * 16384 + m.offset;
                r = m.flash[start..start + len as usize].to_vec();
                if m.fault == "backup" && m.reads == 4097 {
                    r[0] ^= 1;
                }
                if m.fault == "final"
                    && m.programs > 0
                    && m.reads > 8192 + 4096 + if m.gb { 256 } else { 4096 }
                {
                    r[0] ^= 1;
                }
                m.offset += len as usize;
            }
            (5, _) | (7, _) => (),
            (10, 5) => m.mcu = (m.mcu & 0xffff) | ((a as u32) << 16),
            (10, 6) => m.mcu = (m.mcu & 0xffff0000) | a as u32,
            (10, 8) => {
                let word = m.mcu_read(m.mcu + a as u32 * 2);
                r[2..4].copy_from_slice(&word.to_le_bytes());
            }
            (10, 9) => {
                let address = m.mcu + misc as u32 * 2;
                assert!((BASE..BASE + 512).contains(&address));
                let i = (address - 0x20000000) as usize;
                m.ram[i..i + 2].copy_from_slice(&a.to_le_bytes());
            }
            (10, 2) => (),
            (10, 3) => {
                assert_eq!(a, (BASE as u16) | 1);
                m.helper();
            }
            _ => panic!("unexpected firmware command {d} {o:x} {a:x} {misc:x}"),
        };
        Ok(r)
    }
    fn output(&mut self, d: u8, v: u16, a: u16, data: &[u8]) -> Result<()> {
        let mut m = self.0.borrow_mut();
        assert!(!m.gb);
        assert_eq!((d, v, a), (5, 0x70, 0));
        assert_eq!(data.len(), 256);
        m.programs += 1;
        let base = m.program_bank * 16384 + m.offset;
        if m.fault != "program" {
            for (n, &b) in data.iter().enumerate() {
                m.flash[base + n] &= b;
            }
        }
        m.offset += 256;
        Ok(())
    }
}
fn source(gb: bool) -> Vec<u8> {
    if gb {
        let mut d = vec![0x42; 32768];
        d[0x104..0x134].copy_from_slice(&LOGO);
        d[0x134..0x150].fill(0);
        d[0x147] = 0x19;
        d[0x14d] = d[0x134..0x14d]
            .iter()
            .fold(0u8, |a, &b| a.wrapping_sub(b).wrapping_sub(1));
        let sum = d.iter().map(|&b| b as u32).sum::<u32>() as u16;
        d[0x14e..0x150].copy_from_slice(&sum.to_be_bytes());
        d
    } else {
        let mut p = vec![0x42; 32768];
        p[32764..32766].copy_from_slice(&0x8000u16.to_le_bytes());
        rom::nes_build(&p, &[], BOARDS[0], "vertical").unwrap()
    }
}
fn run(
    gb: bool,
    action: &str,
    fault: &str,
) -> (Result<Value>, Rc<RefCell<Model>>, tempfile::TempDir) {
    run_slot(if gb { "gameboy" } else { "famicom" }, action, fault)
}
fn run_slot(
    slot: &str,
    action: &str,
    fault: &str,
) -> (Result<Value>, Rc<RefCell<Model>>, tempfile::TempDir) {
    let gb = slot == "gameboy";
    let temp = tempfile::tempdir().unwrap();
    let model = Rc::new(RefCell::new(Model::new(gb, fault)));
    let mut request = Request::new(action);
    request.platform = slot.into();
    request.profile = if gb { rom::GB_PROFILE } else { BOARDS[0].id }.into();
    request.directory = Some(temp.path().join("operation"));
    request.data_directory = Some(temp.path().into());
    request.confirmed = true;
    request.download_artwork = false;
    if action == "write" {
        let p = temp.path().join(if gb { "input.gb" } else { "input.nes" });
        let bytes = source(gb);
        std::fs::write(&p, &bytes).unwrap();
        request.source = Some(p);
        request.source_sha256 = Some(rom::sha(&bytes));
    }
    let cancel = Cancel::default();
    model.borrow_mut().cancel = cancel.clone();
    let result = service::run_with(&request, cancel, &mut |_| {}, &mut |c| {
        Ok(Bus {
            transport: Box::new(Wire(model.clone())),
            cancel: c,
        })
    });
    (result, model, temp)
}
fn report(dir: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(dir.join("operation/report.json")).unwrap()).unwrap()
}
#[test]
fn full_writes_keep_two_backups_and_verify_every_byte() {
    for gb in [false, true] {
        let (result, m, temp) = run(gb, "write", "");
        let r = result.unwrap();
        assert_eq!(r["status"], "complete");
        assert_eq!(r["identical_final_reads"], true);
        assert_eq!(m.borrow().erases, 1);
        assert!(m.borrow().programs > 0);
        let saved = std::fs::read(r["output"].as_str().unwrap()).unwrap();
        assert_eq!(saved, source(gb));
        assert_eq!(report(temp.path())["status"], "complete");
        if gb {
            assert!(m.borrow().flash[32768..].iter().all(|&b| b == 255));
        } else {
            assert_eq!(m.borrow().chr, [11, 12, 13, 14]);
        }
        assert_eq!(m.borrow().trace.last(), Some(&(2, 0, 0)));
    }
}
#[test]
fn nes_and_famicom_share_commands_but_retain_the_selected_slot() {
    let (nes, nes_model, nes_temp) = run_slot("nes", "write", "");
    let (famicom, famicom_model, _) = run_slot("famicom", "write", "");
    let nes = nes.unwrap();
    let famicom = famicom.unwrap();
    assert_eq!(nes["slot"], "nes");
    assert_eq!(famicom["slot"], "famicom");
    assert_eq!(nes["platform"], "famicom"); // ROM family remains compatible with the catalog.
    assert_eq!(nes["slot_source"], "user_selection");
    assert_eq!(report(nes_temp.path())["slot"], "nes");
    assert_eq!(nes_model.borrow().trace, famicom_model.borrow().trace);
    let (failure, failed_model, failed_temp) = run_slot("nes", "write", "backup");
    assert_eq!(failure.unwrap_err().code, "BACKUP_MISMATCH");
    assert_eq!(failed_model.borrow().erases, 0);
    assert_eq!(report(failed_temp.path())["slot"], "nes");
}
#[test]
fn wipes_back_up_twice_and_verify_blank_without_programming() {
    for gb in [false, true] {
        let (r, m, _) = run(gb, "wipe", "");
        assert_eq!(r.unwrap()["blank_verified_bytes"], CAPACITY);
        assert_eq!(m.borrow().erases, 1);
        assert_eq!(m.borrow().programs, 0);
        assert!(m.borrow().flash.iter().all(|&b| b == 255));
    }
}
#[test]
fn mismatched_backup_never_erases() {
    for gb in [false, true] {
        let (r, m, temp) = run(gb, "write", "backup");
        assert_eq!(r.unwrap_err().code, "BACKUP_MISMATCH");
        assert_eq!(m.borrow().erases, 0);
        assert_eq!(m.borrow().programs, 0);
        assert_eq!(report(temp.path())["status"], "failed");
    }
}
#[test]
fn blank_check_failure_never_programs() {
    for gb in [false, true] {
        let (r, m, temp) = run(gb, "write", "blank");
        assert_eq!(r.unwrap_err().code, "BLANK_CHECK_FAILED");
        assert_eq!(m.borrow().erases, 1);
        assert_eq!(m.borrow().programs, 0);
        assert_eq!(report(temp.path())["failed_during"], "erasing");
    }
}
#[test]
fn bad_program_is_reported_with_retained_failed_bank() {
    for gb in [false, true] {
        let (r, _, temp) = run(gb, "write", "program");
        assert_eq!(r.unwrap_err().code, "BANK_VERIFY_FAILED");
        assert!(temp.path().join("operation/failed-bank-00.bin").is_file());
        assert_eq!(report(temp.path())["status"], "failed");
    }
}
#[test]
fn disconnect_after_erase_retains_recovery_information() {
    for gb in [false, true] {
        let (r, _, temp) = run(gb, "write", "disconnect");
        let e = r.unwrap_err();
        assert_eq!(e.code, "READER_DISCONNECTED");
        assert!(e.details["backup_directory"].is_string());
        assert_eq!(report(temp.path())["failed_during"], "erasing");
    }
}
#[test]
fn cancellation_journals_and_resets_pins() {
    for gb in [false, true] {
        let (r, m, temp) = run(gb, "write", "cancel");
        assert_eq!(r.unwrap_err().code, "INTERRUPTED");
        assert_eq!(m.borrow().erases, 0);
        assert_eq!(report(temp.path())["status"], "interrupted");
        assert_eq!(m.borrow().trace.last(), Some(&(2, 0, 0)));
    }
}
#[test]
fn automatic_famicom_detection_rejects_unknown_layouts() {
    for fault in ["", "aliased", "no-id", "layout", "unstable"] {
        let m = Rc::new(RefCell::new(Model::new(false, fault)));
        let bus = Bus {
            transport: Box::new(Wire(m.clone())),
            cancel: Cancel::default(),
        };
        let mut r = cartridge_core::famicom::Reader::new(bus, BOARDS[0]);
        let result = r.detect(&mut |_| {});
        r.close().unwrap();
        if fault.is_empty() {
            assert_eq!(result.unwrap()["profile"], BOARDS[0].id);
        } else {
            assert!(result.is_err(), "{fault}");
        }
        assert_eq!(m.borrow().erases, 0);
        assert_eq!(m.borrow().programs, 0);
        assert_eq!(m.borrow().chr, [11, 12, 13, 14]);
        assert!(!m.borrow().id);
    }
}

#[test]
fn final_full_readback_failure_is_not_reported_as_success() {
    for gb in [false, true] {
        let (r, _, temp) = run(gb, "write", "final");
        assert_eq!(r.unwrap_err().code, "FINAL_VERIFY_FAILED");
        assert_eq!(report(temp.path())["failed_during"], "verifying");
        assert!(!temp
            .path()
            .join(if gb {
                "operation/readback.gb"
            } else {
                "operation/readback.nes"
            })
            .exists());
    }
}
#[test]
fn helper_self_test_failure_never_erases() {
    let (r, m, temp) = run(true, "write", "helper");
    assert!(r.is_err());
    assert_eq!(m.borrow().erases, 0);
    assert_eq!(m.borrow().programs, 0);
    assert_eq!(report(temp.path())["failed_during"], "backed_up");
}
#[test]
fn short_or_invalid_buffer_responses_never_erase() {
    for gb in [false, true] {
        let (r, m, _) = run(gb, "write", "buffer");
        assert_eq!(r.unwrap_err().code, "BUFFER_FAILURE");
        assert_eq!(m.borrow().erases, 0);
    }
}
