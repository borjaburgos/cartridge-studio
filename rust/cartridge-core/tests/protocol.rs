//! Golden wire transcripts recorded from the qualified v0.6 host drivers.
//! Payloads are synthetic test patterns. These tests never open a real USB device.
use cartridge_core::{
    rom::BOARDS,
    storage::Cancel,
    usb::{Bus, Transport},
    Result,
};
use serde_json::{json, Value};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
struct Replay(Rc<RefCell<VecDeque<Value>>>);
impl Transport for Replay {
    fn identity(&self) -> Value {
        json!({"product":"REPLAY"})
    }
    fn input(&mut self, d: u8, v: u16, a: u16, n: u16) -> Result<Vec<u8>> {
        let e = self
            .0
            .borrow_mut()
            .pop_front()
            .expect("unexpected USB command");
        assert_eq!(e["direction"], "in");
        assert_eq!(e["request"], d);
        assert_eq!(e["value"], v);
        assert_eq!(e["index"], a);
        assert_eq!(e["length"], n);
        Ok(hex::decode(e["response"].as_str().unwrap()).unwrap())
    }
    fn output(&mut self, d: u8, v: u16, a: u16, data: &[u8]) -> Result<()> {
        let e = self
            .0
            .borrow_mut()
            .pop_front()
            .expect("unexpected USB output");
        assert_eq!(e["direction"], "out");
        assert_eq!(e["request"], d);
        assert_eq!(e["value"], v);
        assert_eq!(e["index"], a);
        assert_eq!(e["data"], hex::encode(data));
        Ok(())
    }
}
fn fixture(name: &str) -> (Bus, Rc<RefCell<VecDeque<Value>>>) {
    let data = std::fs::read(format!(
        "{}/tests/fixtures/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let t = Rc::new(RefCell::new(serde_json::from_slice(&data).unwrap()));
    (
        Bus {
            transport: Box::new(Replay(t.clone())),
            cancel: Cancel::default(),
        },
        t,
    )
}
#[test]
fn all_mapper_bank_sequences_match_the_previous_driver() {
    for (m, banks) in [
        ("ROM", vec![0, 1]),
        ("MBC1", vec![0, 1, 31, 32, 33, 64, 127]),
        ("MBC2", vec![0, 1, 15]),
        ("MBC3", vec![0, 1, 127]),
        ("MBC5", vec![0, 1, 255, 256, 511]),
    ] {
        let (bus, t) = fixture(&format!("bank-{m}"));
        let mut r = cartridge_core::gb::Reader::new(bus);
        r.mapper = Some(m.into());
        for n in banks {
            r.select_bank(n).unwrap();
        }
        assert!(t.borrow().is_empty());
        std::mem::forget(r);
    }
}
#[test]
fn gb_buffered_read_protocol_matches() {
    let (bus, t) = fixture("read-gb");
    let mut bus = bus;
    bus.block(0x26dd, 0x4000, 16384).unwrap();
    assert!(t.borrow().is_empty());
}
#[test]
fn famicom_buffered_read_protocol_matches() {
    let (bus, t) = fixture("read-famicom");
    let mut r = cartridge_core::famicom::Reader::new(bus, BOARDS[0]);
    r.block(true, 0x8000, 16384).unwrap();
    assert!(t.borrow().is_empty());
    std::mem::forget(r);
}
#[test]
fn famicom_programming_protocol_matches() {
    let (bus, t) = fixture("program-famicom");
    let mut r = cartridge_core::famicom::Reader::new(bus, BOARDS[0]);
    r.flash_verified = true;
    let data: Vec<u8> = (0..16384).map(|n| n as u8).collect();
    r.program_bank(7, &data).unwrap();
    assert!(t.borrow().is_empty());
    std::mem::forget(r);
}
#[test]
fn bounded_identity_descriptors() {
    assert_eq!(
        cartridge_core::usb::decode_string(&[6, 3, b'O', 0, b'K', 0]).unwrap(),
        "OK"
    );
    for bad in [&[][..], &[3, 3, 0][..], &[8, 3, 0, 0][..], &[2, 1][..]] {
        assert!(cartridge_core::usb::decode_string(bad).is_err());
    }
}

#[test]
fn gb_erase_uses_the_qualified_unlock_sequence() {
    let (bus, t) = fixture("erase-gb-sequence");
    let prefix = [(7, 0, 1), (5, 0, 0), (12, 1, 0x3000), (12, 0x101, 0x2000)];
    for (d, v, a) in prefix.into_iter().rev() {
        t.borrow_mut().push_front(
            json!({"direction":"in","request":d,"value":v,"index":a,"length":1,"response":"00"}),
        );
    }
    t.borrow_mut().push_back(
        json!({"direction":"in","request":12,"value":0,"index":0,"length":3,"response":"0001ff"}),
    );
    let mut r = cartridge_core::gb::Reader::new(bus);
    r.mapper = Some("MBC5".into());
    r.flash_verified = true;
    r.erase().unwrap();
    assert!(t.borrow().is_empty());
    std::mem::forget(r);
}
