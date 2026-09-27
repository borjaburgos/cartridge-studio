# INLretro Game Boy qualification record

This is a historical record of the initial Python implementation's September 14,
2026 hardware tests. It preserves evidence and protocol references; it is not a
current usage guide or a substitute for Rust hardware qualification. Earlier
development implementations are retained only in a local private recovery archive,
not the public Git history. Current commands and support
are documented in the [main guide](../../README.md), [reader matrix](../readers.md)
and [architecture](../architecture.md).

## Physical reader and cartridge

- INL Retro-Prog, PCB v2.1 reported by the owner, USB descriptor version 2.03.
- Ferrante-style 512 KiB MBC5 board: SST39SF040 flash (`BF B7`), write-enable
  through the Game Boy AUDIO pin, 5 V supply, no save RAM.
- Installed image: `SOLARSTRIKER`, MBC5 (`0x19`), 256 KiB, 16 banks, CGB compatible.

A matching ROM header alone does not establish physical board compatibility.
Only explicitly supported flash-chip, wiring and board profiles permit erase
or programming. The current safety contract still requires two complete matching
backups, full blank verification, immediate bank readback and two final readbacks.

## Verified read, September 14, 2026

Two full physical reads were byte-identical. Nintendo logo, header checksum,
ROM length and global checksum (`0xC632`) passed.

- ROM SHA-256: `c61a6c3c8854415eb8465214b42f349282e8df079721262133c198b90318cd7a`.
- Evidence: local ignored `tmp/cartridge-backups/inlretro-2026-09-14/`.
- The unchanged image was booted in a local binjgb browser player. No game build
  or deployment was involved.

## Verified erase and rewrite, September 14, 2026

The flash identified as SST39SF040 with 512 KiB capacity. Before erase, two
full-chip backups matched. A temporary RAM helper passed its echo test and small
physical programming tests in two unused banks. The complete chip passed a blank
check; all 16 game banks were rewritten and individually verified. Two final
full-chip reads matched the source ROM plus erased (`FF`) padding exactly.

- Restored full-chip SHA-256: `97f50e2bd596ad75530da071daf71d4e61123c99960fd5fe0c54f084297ec017`.
- This matched the original full-chip backup. The ROM-sized `readback.gb` retained
  the ROM SHA-256 above.
- PyBoy cold boots passed in both CGB and DMG modes: greeting, title, story,
  movement, firing, enemies, audio and 12 seconds of stage-one combat per mode.
  These were opening-game checks, not campaign playthroughs.
- Evidence: local ignored `tmp/cartridge-backups/inlretro-rewrite-2026-09-14/`.
  Its `verified-rewrite/` folder retains the final transaction and readbacks.

The small C/Thumb helper uses 512 bytes of the reader's firmware-allocated
transfer RAM. Its allocation and code bytes are validated and an echo test runs
before erase. This does not install firmware or write the programmer's flash.
The old firmware supports halfword RAM uploads; its declared but unimplemented
numbered payload commands are not used.

Independent offline ARM tests remain available:

```sh
tmp/toolchains/inlretro-tests/bin/python scripts/test_inlretro_flash.py
```

Unicorn is a development-only dependency pinned in `requirements-tests.txt`.
The original PyBoy playtest environment is preserved in `tmp/toolchains/playtest/`,
with pins in `requirements-playtest.txt`. Neither environment ships with the app.

## Physical buttons

Neither physical button was needed for the successful read/write cycle. On this
PCB revision, the central, unrecessed Reset button resets an unresponsive reader.
The recessed BL button enters the firmware bootloader when held during power-up
or reset; it is not a dump button. Disconnect USB before changing cartridges.

## Protocol references

- [Manufacturer software and button instructions](https://gitlab.com/InfiniteNesLives/INL-retro-progdump),
  checked at `7f21176c2ff220cc36fd37355a88d7f7ecf6ff9f`, especially the shared
  dictionaries and host `app/dump.lua` and `app/buffers.lua`.
- [Pan Docs: cartridge headers and memory bank controllers](https://gbdev.io/pandocs/).
- [Microchip SST39SF040 data sheet](https://ww1.microchip.com/downloads/aemDocuments/documents/MPD/ProductDocuments/DataSheets/SST39SF010A-SST39SF020A-SST39SF040-Data-Sheet-DS20005022.pdf).
- [Ferrante 512 KiB Game Boy flash cartridge](https://ferrantecrafts.com/products/flash-cartridge-for-game-boy-512kb).
