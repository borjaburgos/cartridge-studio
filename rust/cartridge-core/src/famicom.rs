use crate::{
    rom::{Board, BOARDS},
    usb::Bus,
    Error, Result,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
pub struct Reader {
    pub bus: Bus,
    pub board: Board,
    pub flash_verified: bool,
    closed: bool,
    bank_touched: bool,
}
impl Reader {
    pub fn new(bus: Bus, board: Board) -> Self {
        Self {
            bus,
            board,
            flash_verified: false,
            closed: false,
            bank_touched: false,
        }
    }
    pub fn initialize(&mut self) -> Result<()> {
        self.bus.cancel.check()?;
        self.bus.cmd(2, 0, 0, 0)?;
        self.bus.cmd(2, 1, 0, 0)?;
        std::thread::sleep(Duration::from_millis(100));
        Ok(())
    }
    pub fn cpu_read(&mut self, a: u16) -> Result<u8> {
        self.bus.byte(3, 0x81, a)
    }
    pub fn cpu_write(&mut self, a: u16, v: u8) -> Result<()> {
        self.bus.cmd(3, 2, a, v)
    }
    fn ppu_read(&mut self, a: u16) -> Result<u8> {
        self.bus.byte(3, 0x82, a)
    }
    fn ppu_write(&mut self, a: u16, v: u8) -> Result<()> {
        self.bus.cmd(3, 1, a, v)
    }
    pub fn select_bank(&mut self, n: u8) -> Result<()> {
        if self.board.mapper != 30 {
            return Err(Error::check("Invalid UNROM-512 bank register write."));
        }
        self.bank_touched = true;
        self.cpu_write(0xc000, n)
    }
    pub fn block(&mut self, cpu: bool, a: u16, size: usize) -> Result<Vec<u8>> {
        if !(256..=16384).contains(&size)
            || !size.is_power_of_two()
            || !(a as usize).is_multiple_of(size)
            || (cpu && (a < 0x8000 || a as usize + size > 0x10000))
            || (!cpu && a as usize + size > 0x2000)
        {
            return Err(Error::check("Read outside aligned cartridge ROM windows."));
        }
        self.bus.block(if cpu { 0x22dd } else { 0x23dd }, a, size)
    }
    pub fn read_bank(&mut self, n: usize) -> Result<Vec<u8>> {
        self.bus.cancel.check()?;
        if n >= self.board.prg_capacity / 16384 {
            return Err(Error::check("Bank exceeds physical capacity."));
        }
        let a = if self.board.mapper == 30 {
            self.select_bank(n as u8)?;
            0x8000
        } else {
            (0x8000 + n * 16384) as u16
        };
        self.block(true, a, 16384)
    }
    fn mirror_sample(&mut self) -> Result<&'static str> {
        self.bus.cmd(1, 17, 0x800, 0)?;
        let h = self.bus.transfer(1, 6, 11, 0, 4, false)?[2..]
            .iter()
            .any(|&v| v != 0);
        self.bus.cmd(1, 17, 0x400, 0)?;
        let v = self.bus.transfer(1, 6, 11, 0, 4, false)?[2..]
            .iter()
            .any(|&v| v != 0);
        Ok(match (h, v) {
            (false, false) => "one-a",
            (true, true) => "one-b",
            (true, false) => "horizontal",
            (false, true) => "vertical",
        })
    }
    pub fn mirroring(&mut self) -> Result<&'static str> {
        let result = if self.board.mapper != 30 {
            let s = self.mirror_sample()?;
            if s == "horizontal" || s == "vertical" {
                Some(s)
            } else {
                None
            }
        } else {
            let r = (|| {
                self.select_bank(0)?;
                let first = self.mirror_sample()?;
                self.select_bank(0x80)?;
                let second = self.mirror_sample()?;
                Ok((first, second))
            })();
            let cleanup = self.select_bank(0);
            let (first, second) = r.and_then(|v| cleanup.map(|_| v))?;
            if (first, second) == ("one-a", "one-b") {
                Some("one-screen")
            } else if first == second && (first == "horizontal" || first == "vertical") {
                Some(first)
            } else {
                None
            }
        };
        result.ok_or_else(||Error::new("MIRRORING_UNDETECTED","The cartridge mirroring jumper could not be read reliably.","Unplug USB, check seating and orientation, and inspect the mirroring jumper. Do not choose a value just to bypass this check.").exit(3))
    }
    fn flash_command(&mut self, v: u8) -> Result<()> {
        self.select_bank(1)?;
        self.cpu_write(0x9555, 0xaa)?;
        self.select_bank(0)?;
        self.cpu_write(0xaaaa, 0x55)?;
        self.select_bank(1)?;
        self.cpu_write(0x9555, v)
    }
    fn sample(&mut self, start: u16, len: usize) -> Result<Vec<u8>> {
        let mut v = Vec::new();
        for i in 0..len {
            self.bus.cancel.check()?;
            v.push(self.cpu_read(start + i as u16)?);
        }
        Ok(v)
    }
    pub fn identify_flash(&mut self, transition: bool) -> Result<String> {
        self.flash_verified = false;
        if !self.board.writable || self.board.mapper != 30 {
            return Err(Error::new(
                "BOARD_READ_ONLY",
                "Flash identification is not qualified for this board profile.",
                "Use read/backup, or select a qualified flash cartridge.",
            ));
        }
        self.cpu_write(0x8000, 0xf0)?;
        self.select_bank(0)?;
        let before = self.sample(0x8000, 64)?;
        let rom_id = if transition {
            self.select_bank(1)?;
            let id = self.sample(0x8000, 2)?;
            self.select_bank(0)?;
            id
        } else {
            vec![]
        };
        let result = (|| {
            self.flash_command(0x90)?;
            self.sample(0x8000, 2)
        })();
        let c1 = self.cpu_write(0x8000, 0xf0);
        let c2 = self.select_bank(0);
        let id = result.and_then(|v| c1.and(c2).map(|_| v))?;
        let after = self.sample(0x8000, 64)?;
        if before != after {
            return Err(Error::new(
                "FLASH_READ_MODE",
                "The cartridge did not return to the same ROM bytes after identification.",
                "Reconnect and reseat the cartridge. Erase/write is blocked.",
            )
            .exit(3));
        }
        if transition && id == rom_id {
            return Err(Error::new("FLASH_ID_AMBIGUOUS","The cartridge did not show a distinct flash identification response.","Automatic detection cannot confirm this board. Check the PCB or manufacturer documentation.").exit(3));
        }
        let received = hex::encode(id);
        if self.board.flash_id != Some(received.as_str()) {
            return Err(Error::new("FLASH_NOT_IDENTIFIED",format!("Expected SST39SF040 flash ID BF B7; received {received}."),"Unplug USB, reseat the cartridge, and check its orientation and board model. Nothing has been erased.").details(json!({"expected":self.board.flash_id,"received":received,"sample":hex::encode(before)})).exit(3));
        }
        self.flash_verified = true;
        Ok(received)
    }
    pub fn verify_chr_ram(&mut self) -> Result<usize> {
        if !self.flash_verified {
            return Err(Error::check("Identify flash before checking graphics RAM."));
        }
        let mut originals = Vec::new();
        let result = (|| {
            for n in 0..4 {
                self.bus.cancel.check()?;
                self.select_bank(n << 5)?;
                originals.push(self.ppu_read(0)?);
            }
            for (n, &v) in [0x31, 0x72, 0xa4, 0xe8].iter().enumerate() {
                self.bus.cancel.check()?;
                self.select_bank((n as u8) << 5)?;
                self.ppu_write(0, v)?;
            }
            let mut actual = Vec::new();
            for n in 0..4 {
                self.bus.cancel.check()?;
                self.select_bank(n << 5)?;
                actual.push(self.ppu_read(0)?);
            }
            if actual != [0x31, 0x72, 0xa4, 0xe8] {
                return Err(Error::new("CHR_RAM_MISMATCH","The four independent graphics RAM banks did not respond.","Check that the board has 32 KiB of CHR RAM and is seated correctly. Erase/write is blocked.").details(json!({"bank_reads":actual})).exit(3));
            }
            Ok(32768)
        })();
        // Restore all sampled banks, even if one restore fails. Cancellation never skips this.
        let mut failure = None;
        for (n, &v) in originals.iter().enumerate() {
            if self
                .select_bank((n as u8) << 5)
                .and_then(|_| self.ppu_write(0, v))
                .is_err()
            {
                failure = Some(());
            }
        }
        for (n, &v) in originals.iter().enumerate() {
            if self
                .select_bank((n as u8) << 5)
                .and_then(|_| self.ppu_read(0))
                .ok()
                != Some(v)
            {
                failure = Some(());
            }
        }
        let bank = self.select_bank(0);
        if failure.is_some() {
            return Err(Error::new("CHR_RAM_RESTORE_FAILED","A graphics RAM test byte could not be restored.","Unplug USB and reseat the cartridge. ROM erase/write is blocked; temporary graphics RAM resets when power is removed.").exit(3));
        }
        result.and_then(|v| bank.map(|_| v))
    }
    pub fn inspect(&mut self) -> Result<Value> {
        self.flash_verified = false;
        self.initialize()?;
        let mut v = json!({"device":self.bus.identity(),"board":self.board});
        if self.board.writable {
            v["flash_id"] = json!(self.identify_flash(false)?);
        }
        v["mirroring"] = json!(self.mirroring()?);
        if self.board.writable {
            v["chr_ram_verified"] = json!(self.verify_chr_ram()?);
        }
        Ok(v)
    }
    pub fn detect(&mut self, progress: &mut dyn FnMut(String)) -> Result<Value> {
        let mut evidence = vec![];
        let result = (|| {
            self.initialize()?;
            progress("Detecting cartridge: checking stable bus reads…".into());
            let mut before = self.sample(0x8000, 64)?;
            before.extend(self.sample(0xc000, 64)?);
            let mut again = self.sample(0x8000, 64)?;
            again.extend(self.sample(0xc000, 64)?);
            if before != again {
                return Err(Error::new("CARTRIDGE_UNSTABLE","The cartridge returned different bytes on repeated reads.","Unplug USB, clean and reseat the cartridge, then reconnect. No flash identification was attempted.").exit(3));
            }
            evidence.push("Repeated bus samples match.".to_string());
            progress("Detecting cartridge: reading the UNROM-512 flash signature…".into());
            let first = self.identify_flash(true)?;
            if first != self.identify_flash(true)? {
                return Err(Error::check("Flash identification changed between checks."));
            }
            evidence.push("SST39SF040 flash ID BF B7 confirmed twice, with distinct ID/read modes and restored ROM samples.".into());
            progress("Detecting cartridge: checking graphics banks and mirroring…".into());
            let ram = self.verify_chr_ram()?;
            evidence.push(
                "Four independent CHR RAM banks respond; original test bytes restored.".into(),
            );
            let mirror = self.mirroring()?;
            let layout = (|| {
                self.select_bank(31)?;
                let mut fixed = Vec::new();
                let mut last = Vec::new();
                for o in (0..32).chain(0x3ffa..0x4000) {
                    self.bus.cancel.check()?;
                    fixed.push(self.cpu_read(0xc000 + o)?);
                    last.push(self.cpu_read(0x8000 + o)?);
                }
                self.select_bank(0)?;
                let mut stable = Vec::new();
                for o in (0..32).chain(0x3ffa..0x4000) {
                    self.bus.cancel.check()?;
                    stable.push(self.cpu_read(0xc000 + o)?);
                }
                if fixed != last || fixed != stable {
                    return Err(Error::new("PRG_LAYOUT_MISMATCH","Program memory does not match the supported UNROM-512 layout.","Check the board and memory chips. Automatic detection cannot select a safe profile."));
                }
                Ok(())
            })();
            let cleanup = self.select_bank(0);
            layout.and(cleanup)?;
            evidence.push(format!(
                "Fixed final PRG window matches bank 31; {mirror} mirroring detected."
            ));
            let name = "UNROM-512 · SST39SF040 · 32 KiB CHR RAM";
            Ok(
                json!({"device":self.bus.identity(),"board":BOARDS[0],"profile":BOARDS[0].id,"flash_id":first,"chr_ram_verified":ram,"mirroring":mirror,"detection":{"status":"identified","platform":"famicom","profile":BOARDS[0].id,"name":name,"basis":"hardware","writable":true,"summary":format!("{name} · 512 KiB flash · {mirror} mirroring"),"evidence":evidence,"limitation":"Matches the supported hardware interface; PCB manufacturer and revision cannot be read electronically."},"message":format!("Detected {name} · 512 KiB flash · {mirror} mirroring. Read/write profile available.")}),
            )
        })();
        self.flash_verified = false;
        result.map_err(|e|if ["FLASH_NOT_IDENTIFIED","FLASH_ID_AMBIGUOUS","CHR_RAM_MISMATCH","PRG_LAYOUT_MISMATCH","MIRRORING_UNDETECTED"].contains(&e.code.as_str()){Error::new("BOARD_NOT_DETECTED",format!("Automatic detection could not identify this board. {}",e.message),"Unplug USB and check seating and orientation. If detection still fails, look up the PCB model or chip markings and select a supported profile. NROM and other NES/Famicom boards cannot yet be identified automatically. No erase or program command was sent.").details(json!({"platform":"famicom","evidence":evidence,"reason":e})).exit(3)}else{e})
    }
    pub fn erase(&mut self) -> Result<()> {
        self.bus.cancel.check()?;
        if !self.flash_verified {
            return Err(Error::new(
                "FLASH_UNVERIFIED",
                "Erase requires a matching physical flash identification.",
                "Run Detect with the correct board profile first.",
            ));
        }
        self.bus.stop()?;
        self.flash_command(0x80)?;
        self.flash_command(0x10)?;
        let end = Instant::now() + Duration::from_secs(30);
        while self.cpu_read(0x8000)? != 255 {
            self.bus.cancel.check()?;
            if Instant::now() > end {
                return Err(Error::new("ERASE_TIMEOUT","The flash chip did not finish erasing.","Keep the complete backup and source. Reconnect and retry the write using the saved source.").exit(5));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }
    pub fn program_bank(&mut self, n: usize, data: &[u8]) -> Result<()> {
        self.bus.cancel.check()?;
        if !self.flash_verified || self.board.mapper != 30 || n >= 32 || data.len() != 16384 {
            return Err(Error::check(
                "Unverified flash or invalid programming bank.",
            ));
        }
        self.bus.stop()?;
        let result = (|| {
            self.bus.cmd(3, 0x20, n as u16, 0)?;
            for n in 0..2 {
                self.bus.cmd(5, 0x80 + n, n as u16 * 8, 8)?;
                self.bus.cmd(5, 0x90 + n, n as u16, 2)?;
                self.bus.cmd(5, 0x30, 0x10dd, n)?;
                self.bus.cmd(5, 0x32, 30 << 8, n)?;
            }
            self.bus.cmd(7, 0, 0xf2, 0)?;
            for chunk in data.as_chunks::<256>().0.iter() {
                self.bus.wait(&[0], None)?;
                self.bus.transport.output(5, 0x70, 0, chunk)?;
            }
            for n in 0..2 {
                self.bus.wait(&[0, 0xf4], Some(n))?;
            }
            Ok(())
        })();
        let cleanup = self.bus.stop();
        result.and(cleanup)
    }
    pub fn close(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        let bank = if self.board.mapper == 30 && self.bank_touched {
            self.select_bank(0)
        } else {
            Ok(())
        };
        let buffers = self.bus.stop();
        let reset = self.bus.cmd(2, 0, 0, 0);
        bank.and(buffers).and(reset)
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            eprintln!("Reader cleanup: {e}");
        }
    }
}
