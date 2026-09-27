"""Offline safety tests, including execution of the actual ARM helper in Unicorn.

Run with tmp/toolchains/inlretro-tests/bin/python scripts/test_inlretro_flash.py.
Unicorn is needed only for development tests, not for using the utility.
"""
import struct
import unittest

from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_MEM_WRITE
from unicorn.arm_const import UC_ARM_REG_SP, UC_ARM_REG_LR
from build_helper import CAPACITY, compile_helper, relocate_helper, sha, SOURCES



class FlashTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.base = 0x20000500
        cls.code = compile_helper(cls.base)

    def execute(self, bank, offset, data, magic=0x53465354, count=None):
        cpu = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
        cpu.mem_map(0x20000000, 0x2000)
        cpu.mem_map(0x48000000, 0x1000)
        cpu.mem_write(self.base, self.code)
        packet = struct.pack('<IHBBI', magic, offset, bank, len(data) if count is None else count, 0)
        cpu.mem_write(self.base + 384, packet + data.ljust(116, b'\xff'))
        cpu.reg_write(UC_ARM_REG_SP, 0x20001800)
        cpu.reg_write(UC_ARM_REG_LR, 0x20001001)
        flash = bytearray(b'\xff' * CAPACITY)
        state = dict(bank=1, unlock=0, written=[], pending=None, pins=0x2d)

        def on_write(cpu, access, address, size, value, user):
            # Independent physical-bus model: /WR controls MBC5, /AUDIO programs
            # NOR flash after the SST unlock sequence on physical address A14..0.
            if address == 0x48000028:
                state['pins'] &= ~value
                return
            if address != 0x48000018:
                return
            rising = value & ~state['pins']
            state['pins'] |= value
            a = int.from_bytes(cpu.mem_read(0x48000814, 4), 'little') & 65535
            d = (int.from_bytes(cpu.mem_read(0x48000414, 4), 'little') >> 8) & 255
            if rising & 8:
                self.assertEqual(a, 0x2000)
                state['bank'] = d
            if not rising & 32:
                return
            physical = a if a < 0x4000 else state['bank'] * 0x4000 + (a & 0x3fff)
            stage = state['unlock']
            if stage == 3:
                self.assertLess(physical, CAPACITY)
                flash[physical] &= d
                state['written'].append(physical)
                state['unlock'] = 0
            else:
                expected = [(0x5555, 0xaa), (0x2aaa, 0x55), (0x5555, 0xa0)][stage]
                self.assertEqual((physical & 0x7fff, d), expected)
                state['unlock'] += 1

        cpu.hook_add(UC_HOOK_MEM_WRITE, on_write)
        cpu.emu_start(self.base | 1, 0x20001000, count=2_000_000)
        result = int.from_bytes(cpu.mem_read(self.base + 392, 4), 'little')
        return flash, state, result, cpu

    def test_actual_arm_code_writes_exact_banks_and_releases_bus(self):
        for bank, offset in ((0, 0x130), (1, 0x3f8c), (2, 0), (16, 0x100), (31, 0x3f8c)):
            with self.subTest(bank=bank):
                data = bytes((i * 43) & 255 for i in range(116))
                flash, state, result, cpu = self.execute(bank, offset, data)
                expected = bytearray(b'\xff' * CAPACITY)
                start = bank * 16384 + offset
                expected[start:start+len(data)] = data
                self.assertEqual(flash, expected)
                self.assertEqual(result, 0x600d0074)
                self.assertEqual(int.from_bytes(cpu.mem_read(0x48000400, 4), 'little') >> 16, 0)
                self.assertEqual(state['pins'] & 0x2d, 0x2d)

    def test_bundled_relocated_helper_executes_at_different_allocations(self):
        original_base, original_code = self.base, self.code
        linked_base = 0x20000000
        code = compile_helper(linked_base)
        comparison = compile_helper(0x20000500)
        manifest = dict(link_base=linked_base, sha256=sha(code),
                        source_sha256=sha(b''.join(p.read_bytes() for p in SOURCES)),
                        relocations=[i for i in range(0, 384, 4) if code[i:i+4] != comparison[i:i+4]])
        try:
            for base in (0x20000000, 0x200004f8, 0x20000500, 0x20000b00):
                self.base, self.code = base, relocate_helper(code, manifest, base)
                self.assertEqual(self.code, compile_helper(base))
                payload = bytes(range(116))
                flash, state, result, cpu = self.execute(17, 12, payload)
                self.assertEqual(flash[17*16384+12:17*16384+128], payload)
                self.assertEqual(result, 0x600d0074)
            with self.assertRaises(ValueError):
                relocate_helper(bytes(384), manifest, 0x20000500)
            with self.assertRaises(ValueError):
                relocate_helper(code, manifest, 0x08000000)
        finally:
            self.base, self.code = original_base, original_code

    def test_invalid_helper_packets_never_pulse_write(self):
        for bank, offset, magic, count in ((32,0,0x53465354,1), (0,16384,0x53465354,1),
                                          (0,0,0,1), (0,0,0x53465354,117)):
            _, state, result, _ = self.execute(bank,offset,b'\0',magic,count)
            self.assertFalse(state['written'])
            self.assertEqual(result,0)

    def test_echo_and_erased_bytes_do_not_program(self):
        _, state, result, _ = self.execute(0,0,bytes(range(116)),0x4543484f)
        self.assertEqual(result,0xec000000 | sum(range(116)))
        self.assertFalse(state['written'])
        _, state, result, _ = self.execute(31,0,b'\xff'*116)
        self.assertFalse(state['written'])
        self.assertEqual(result,0x600d0074)


if __name__=='__main__':unittest.main(verbosity=2)
