//! Board-specific SST flash access. The qualified on-device Thumb helper is unchanged.
use crate::{
    gb::Reader,
    rom::{self, CAPACITY, GB_PROFILE},
    Error, Result,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
impl Reader {
    pub fn enable_audio(&mut self) -> Result<()> {
        self.audio = true;
        self.bus.cmd(1, 5, 15, 0)?;
        self.bus.cmd(1, 3, 15, 0)
    }
    fn flash_cycle(&mut self, a: u16, v: u8) -> Result<()> {
        self.bus.cmd(1, 17, a, 0)?;
        self.bus.cmd(1, 11, v as u16, 0)?;
        self.bus.cmd(1, 4, 15, 0)?;
        self.bus.cmd(1, 5, 15, 0)
    }
    pub(crate) fn command(&mut self, cycles: &[(u16, u8)]) -> Result<()> {
        self.bus.cmd(1, 10, 0, 0)?;
        let r = (|| {
            for &(a, v) in cycles {
                self.flash_cycle(a, v)?;
            }
            Ok(())
        })();
        let high = self.bus.cmd(1, 5, 15, 0);
        let input = self.bus.cmd(1, 8, 0, 0);
        r.and(high).and(input)
    }
    pub fn identify_flash(&mut self) -> Result<Value> {
        self.flash_verified = false;
        self.mapper = Some("MBC5".into());
        self.initialize()?;
        self.select_bank(1)?;
        let mut before = Vec::new();
        for a in 0..0x150 {
            self.bus.cancel.check()?;
            before.push(self.read_byte(a)?);
        }
        let info = rom::gb_header(&before)?;
        if info["logo_valid"] == true
            && (info["cartridge_type"] != 0x19 || info["ram_size_code"] != 0)
        {
            return Err(Error::new("ROM_INCOMPATIBLE","The connected cartridge header does not match the AUDIO/MBC5 flash board.","Use Automatic for retail cartridges, or select the exact physical flash board. Nothing has been erased."));
        }
        self.enable_audio()?;
        self.command(&[(0, 0xf0)])?;
        let result = (|| {
            self.command(&[(0x5555, 0xaa), (0x2aaa, 0x55), (0x5555, 0x90)])?;
            std::thread::sleep(Duration::from_millis(1));
            Ok(vec![self.read_byte(0)?, self.read_byte(1)?])
        })();
        let cleanup = self.command(&[(0, 0xf0)]);
        std::thread::sleep(Duration::from_millis(1));
        let id: Vec<u8> = result.and_then(|v| cleanup.map(|_| v))?;
        let mut after = Vec::new();
        for a in 0..0x150 {
            self.bus.cancel.check()?;
            after.push(self.read_byte(a)?);
        }
        if after != before {
            return Err(Error::check(
                "Cartridge bytes changed after flash identification. Reconnect before retrying.",
            ));
        }
        if id != [0xbf, 0xb7] {
            return Err(Error::new("FLASH_NOT_IDENTIFIED",format!("Expected SST39SF040 flash ID BF B7; received {}.",hex::encode(&id)),"Check the board model and AUDIO write-enable wiring with USB unplugged. Nothing has been erased.").details(json!({"received":hex::encode(id)})).exit(3));
        }
        self.flash_verified = true;
        Ok(
            json!({"profile":GB_PROFILE,"flash_id":"bfb7","capacity":CAPACITY,"header":info,"header_unchanged_after_id":true}),
        )
    }
    fn set_ram_address(&mut self, a: u32) -> Result<()> {
        self.bus.cmd(10, 5, (a >> 16) as u16, 0)?;
        self.bus.cmd(10, 6, a as u16, 0)
    }
    pub fn read_mcu(&mut self, a: u32, len: usize) -> Result<Vec<u8>> {
        let end = (a as u64) + (len as u64);
        let valid = [
            (0x20000000u64, 0x20001800u64),
            (0x08000000, 0x08000008),
            (0x48000814, 0x48000818),
            (0x48000414, 0x48000418),
        ]
        .iter()
        .any(|&(l, h)| a as u64 >= l && end <= h);
        if !a.is_multiple_of(2) || !len.is_multiple_of(2) || !valid {
            return Err(Error::check(
                "MCU read is outside the qualified memory ranges.",
            ));
        }
        self.set_ram_address(a)?;
        let mut data = Vec::with_capacity(len);
        for i in 0..len / 2 {
            self.bus.cancel.check()?;
            data.extend_from_slice(&self.bus.transfer(10, 8, i as u16, 0, 4, false)?[2..]);
        }
        Ok(data)
    }
    fn write_buffer(&mut self, n: usize, data: &[u8]) -> Result<()> {
        let base = self
            .helper_base
            .ok_or_else(|| Error::check("Helper RAM has not been allocated."))?;
        if n >= 4 || data.len() != 128 {
            return Err(Error::check("Invalid helper buffer write."));
        }
        self.set_ram_address(base + n as u32 * 128)?;
        for i in (0..128).step_by(2) {
            self.bus.cancel.check()?;
            if n == 3
                && self
                    .mailbox
                    .as_ref()
                    .is_some_and(|p| p[i..i + 2] == data[i..i + 2])
            {
                continue;
            }
            self.bus.cmd(
                10,
                9,
                u16::from_le_bytes([data[i], data[i + 1]]),
                (i / 2) as u8,
            )?;
        }
        if n == 3 {
            self.mailbox = Some(data.to_vec());
        }
        Ok(())
    }
    fn allocate_helper(&mut self) -> Result<()> {
        self.bus.stop()?;
        self.mailbox = None;
        for n in 0..4 {
            self.bus
                .cmd(5, 0x80 + n, (((0xb0 + n) as u16) << 8) | (n as u16 * 4), 4)?;
        }
        Ok(())
    }
    fn call_helper(&mut self) -> Result<()> {
        self.bus.cancel.check()?;
        let a = self
            .helper_base
            .ok_or_else(|| Error::check("Helper RAM unavailable."))?;
        self.bus.cmd(10, 2, (a >> 16) as u16, 0)?;
        self.bus.cmd(10, 3, a as u16 | 1, 0)
    }
    pub fn load_helper(&mut self) -> Result<()> {
        self.allocate_helper()?;
        let code = self.helper_code.clone();
        if code.len() != 384 {
            return Err(Error::check("The qualified flash helper is unavailable."));
        }
        for n in 0..3 {
            self.write_buffer(n, &code[n * 128..(n + 1) * 128])?;
        }
        if self.read_mcu(self.helper_base.unwrap(), 384)? != code {
            return Err(Error::check(
                "The RAM helper did not verify before execution.",
            ));
        }
        self.write_buffer(3, &[0; 128])
    }
    pub fn prepare_helper(&mut self) -> Result<()> {
        if !self.flash_verified {
            return Err(Error::check("Identify flash before loading its helper."));
        }
        let v = self.read_mcu(0x08000000, 8)?;
        let stack = u32::from_le_bytes(v[..4].try_into().unwrap());
        let reset = u32::from_le_bytes(v[4..].try_into().unwrap());
        if !(0x20001800..=0x20010000).contains(&stack)
            || !(0x08000001..0x08020000).contains(&reset)
            || reset & 1 == 0
        {
            return Err(Error::check(
                "The MCU memory layout differs from the qualified programmer.",
            ));
        }
        self.bus.cmd(1, 17, 0x1357, 0)?;
        self.bus.cmd(1, 11, 0xa6, 0)?;
        let address = self.read_mcu(0x48000814, 4)?;
        let data = self.read_mcu(0x48000414, 4)?;
        if u32::from_le_bytes(address.try_into().unwrap()) & 0xffff != 0x1357
            || (u32::from_le_bytes(data.try_into().unwrap()) >> 8) & 255 != 0xa6
        {
            return Err(Error::check(
                "Programmer GPIO layout is not qualified for this helper.",
            ));
        }
        self.allocate_helper()?;
        let ram = self.read_mcu(0x20000000, 0x1800)?;
        let mut ptrs = Vec::new();
        let mut descriptors = Vec::new();
        for n in 0..4 {
            let found: Vec<usize> = (0..ram.len() - 20)
                .step_by(4)
                .filter(|&i| {
                    ram[i..i + 4] == [0, 0, 0xb0 + n, 0]
                        && ram[i + 8..i + 18] == [0x7f, 0, 0, 0, 0, 0, 0, 0, 0, 0]
                })
                .collect();
            if found.len() != 1 {
                return Err(Error::check(
                    "Could not prove unique firmware buffer ownership.",
                ));
            }
            let i = found[0];
            ptrs.push(u32::from_le_bytes(ram[i + 4..i + 8].try_into().unwrap()));
            descriptors.push(0x20000000 + i as u32);
        }
        let base = ptrs[0];
        if base & 3 != 0
            || !(0x20000000..=0x20001600).contains(&base)
            || ptrs
                .iter()
                .enumerate()
                .any(|(n, &p)| p != base + n as u32 * 128)
            || descriptors.iter().any(|&d| d < base + 512 && d + 20 > base)
        {
            return Err(Error::check(
                "Firmware buffers are not a safe contiguous 512-byte helper allocation.",
            ));
        }
        self.bus.stop()?;
        self.bus.cmd(5, 0x80, 0xb00c, 4)?;
        let moved = self.read_mcu(descriptors[0] + 4, 4)?;
        if u32::from_le_bytes(moved.try_into().unwrap()) != base + 384 {
            return Err(Error::check(
                "Firmware buffer pointer ownership could not be verified.",
            ));
        }
        self.allocate_helper()?;
        for (n, &d) in descriptors.iter().enumerate() {
            if u32::from_le_bytes(self.read_mcu(d + 4, 4)?.try_into().unwrap())
                != base + n as u32 * 128
            {
                return Err(Error::check("Firmware buffer allocation changed."));
            }
        }
        let mut code = include_bytes!(env!("CARTRIDGE_STUDIO_EMBEDDED_HELPER")).to_vec();
        if code.len() != 384
            || rom::sha(&code) != "8cc5079eceb93a19e3a09d435cf813411444a27326f6a681d31b926a24a3da47"
            || code[..2] != [0x5f, 0xa0]
        {
            return Err(Error::check(
                "The packaged helper failed its integrity check.",
            ));
        }
        // The link proof for four safe RAM bases identifies exactly this literal relocation.
        let old = u32::from_le_bytes(code[280..284].try_into().unwrap());
        if !(0x20000000..0x20000180).contains(&old) {
            return Err(Error::check("Invalid helper relocation."));
        }
        code[280..284].copy_from_slice(&(old - 0x20000000 + base).to_le_bytes());
        self.helper_base = Some(base);
        self.helper_code = code;
        self.load_helper()?;
        let mut packet = vec![0u8; 128];
        packet[..4].copy_from_slice(&0x4543484fu32.to_le_bytes());
        packet[7] = 116;
        for n in 0..116 {
            packet[12 + n] = n as u8;
        }
        self.write_buffer(3, &packet)?;
        self.call_helper()?;
        let result = u32::from_le_bytes(self.read_mcu(base + 392, 4)?.try_into().unwrap());
        if result != 0xec000000 | (0..116u32).sum::<u32>() {
            return Err(Error::check(
                "The helper echo test failed. Erase was not started.",
            ));
        }
        Ok(())
    }
    pub fn program_chunk(&mut self, bank: usize, offset: usize, data: &[u8]) -> Result<()> {
        if !self.flash_verified
            || bank >= 32
            || data.is_empty()
            || data.len() > 116
            || offset + data.len() > 16384
        {
            return Err(Error::check(
                "Unverified flash or invalid programming chunk.",
            ));
        }
        let mut p = vec![255; 128];
        p[..4].copy_from_slice(&0x53465354u32.to_le_bytes());
        p[4..6].copy_from_slice(&(offset as u16).to_le_bytes());
        p[6] = bank as u8;
        p[7] = data.len() as u8;
        p[8..12].fill(0);
        p[12..12 + data.len()].copy_from_slice(data);
        self.write_buffer(3, &p)?;
        self.call_helper()?;
        let status = u32::from_le_bytes(
            self.read_mcu(self.helper_base.unwrap() + 392, 4)?
                .try_into()
                .unwrap(),
        );
        if status != 0x600d0000 | data.len() as u32 {
            return Err(Error::new(
                "FLASH_PROGRAM_FAILED",
                format!("The helper could not program bank {bank}, offset {offset}."),
                "Retain the source and original backup, reconnect, then restore the saved source.",
            )
            .details(json!({"status":status}))
            .exit(5));
        }
        Ok(())
    }
    pub fn erase(&mut self) -> Result<()> {
        self.bus.cancel.check()?;
        if !self.flash_verified {
            return Err(Error::check("Erase requires a verified flash chip."));
        }
        self.bus.stop()?;
        self.select_bank(1)?;
        self.command(&[
            (0x5555, 0xaa),
            (0x2aaa, 0x55),
            (0x5555, 0x80),
            (0x5555, 0xaa),
            (0x2aaa, 0x55),
            (0x5555, 0x10),
        ])?;
        let end = Instant::now() + Duration::from_secs(30);
        while self.read_byte(0)? != 255 {
            self.bus.cancel.check()?;
            if Instant::now() > end {
                return Err(Error::new("ERASE_TIMEOUT","The flash chip did not finish erasing.","Keep the backup and source. Reconnect and retry the write using the saved source.").exit(5));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }
}
