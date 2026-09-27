# INLretro Famicom qualification record

This is a historical record of the initial Python implementation's September 14,
2026 hardware tests. It preserves evidence and protocol references; it is not a
current usage guide or a substitute for Rust hardware qualification. Earlier
development implementations are retained only in a local private recovery archive,
not the public Git history. For current support and usage,
see the [main guide](../../README.md), [reader matrix](../readers.md) and
[architecture](../architecture.md).

Famicom and NES are separate physical slots: 60-pin and 72-pin respectively.
Current commands and interfaces distinguish them explicitly.

## Qualified physical board

- INLretro PCB v2.1, USB descriptor version `0203`.
- Broke Studio UNROM-512 v2.1, mapper 30.
- 512 KiB SST39SF040 flash (`BF B7`), four 8 KiB CHR RAM banks, no separate PRG RAM.
- Vertical mirroring; other jumper configurations were not physically qualified
  by these historical tests.

The identification sequence checks flash ID without erase, restores ROM read mode
and compares the surrounding ROM sample. Mirroring is checked through CIRAM A10
with two address patterns and both mapper bit-7 states. CHR RAM banking checks
briefly change and then restore a byte in each of the four volatile banks.
ROM headers alone do not establish hardware compatibility.

For mapper 30, smaller ROMs repeat across the 512 KiB flash so the fixed final bank
contains the reset vectors. Only PRG bytes are programmed; the synthesized iNES
or NES 2.0 header belongs to the backup file. Timing, logical size and memory
requirements may require metadata because cartridges do not store that header.
Complete physical backups preserve unused capacity and any flash-backed saves.

## Verified cycle, September 14, 2026

Two original 512 KiB reads matched. The full chip passed a blank check after
erase, and all 32 programmed banks passed immediate readback. The first complete
final read matched, but the process received SIGTERM during the second read.
A separate read-only verification then made two fresh full-chip reads; both
matched the original exactly. No second erase or programming operation occurred.

- Full PRG SHA-256: `9ed41170aa941ec690d52083aec8c47334f56a988a224b3cb957d7e71da13994`.
- Headered ROM SHA-256: `5b284e80eb603d3dc559da024ab54dc25e4f411520b603a8e3d50246cdd47402`.
- Final `readback.nes` matched the original backup byte-for-byte.
- FCEUmm cold-boot/controller tests ran 2,256 frames (about 37 seconds) on the
  original and final readback. All 18 video checkpoints matched. Input changed
  PRG banks, CHR banks, nametables and scrolling. The tested image was an
  interactive cartridge diagnostic with a character sprite and memory labels.

Evidence remains locally under ignored `tmp/cartridge-backups/`:

| Folder | Retained evidence |
| --- | --- |
| `famicom-qualified-original/` | Two original reads, headered ROM, report and playtest |
| `famicom-qualified-rewrite/` | Independent backups, blank capture, source, journal and final/partial readbacks |
| `famicom-qualified-final-verification/` | Two fresh matching reads, final ROM, successful report and playtest images |

Earlier captures in `famicom-2026-09-14/original-prg.read*.bin` contain only EE/EF
and are explicitly marked invalid. Do not restore them. Reseating the cartridge
resolved identification; no button or firmware change was needed. Repeated
identical reads alone do not establish a playable game.

The implementation uses the manufacturer's mapper-30 write routine and supported
current-buffer USB payload command. It avoids the old firmware's unimplemented
numbered payload commands and obsolete mirroring opcode. Famicom does not use
the Game Boy ARM RAM helper.

## Independent emulator reproduction

The optional development player uses FCEUmm pinned to
`236ccdfc911e84c60fea6b9d0699c2d440a8de14` in `tmp/toolchains/fceumm/`.
It needs numpy and Pillow from the separate playtest environment, not from the
installed app. The script does not access physical hardware:

```sh
tmp/toolchains/playtest/bin/python scripts/playtest_famicom.py ROM --output PATH
```

A local interactive player is also retained:

```sh
tmp/toolchains/playtest/bin/python scripts/serve_famicom.py ROM --output PATH --port 8768
```

Open `http://127.0.0.1:8768/`. Arrow keys control direction, Z is A, X is B,
Enter is Start and Shift is Select. It listens only on this computer.

## Protocol references

- [Manufacturer source](https://gitlab.com/InfiniteNesLives/INL-retro-progdump),
  checked at `7f21176c2ff220cc36fd37355a88d7f7ecf6ff9f`: shared dictionaries,
  `firmware/source/nes.c`, `flash.c`, and host `app/flash.lua` / `app/buffers.lua`.
- [Broke Studio Famicom UNROM-512 board](https://www.homebrew-factory.com/pcb/102-51-famicom-unrom-512-mapper-30-pcb-nesmaker.html).
- [NESdev UNROM-512 hardware and submappers](https://www.nesdev.org/wiki/UNROM_512).
- [iNES format](https://www.nesdev.org/wiki/INES) and [NES 2.0 format](https://www.nesdev.org/wiki/NES_2.0).
- [FCEUmm emulator source](https://github.com/libretro/libretro-fceumm).
