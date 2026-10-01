# Reader support

Cartridge Studio **0.1.0-beta.1**, the first public beta, includes three Rust
reader drivers. The qualification records below distinguish physically tested
combinations from implemented but unqualified paths. Reader
selection is separate from the physical cartridge slot and board profile.

| Reader | Supported operations | Current limits |
| --- | --- | --- |
| INLretro | GB / Color, GBA and NES / Famicom detect, read, backup and verify; supported GB and Famicom flash-board write/wipe paths | GBA is read-only at 3.3 V and physical INLretro GBA qualification is pending; writing requires an explicitly supported flash board; physical Rust write qualification remains pending |
| GBxCart RW | GB / Color and GBA ROM detect, read, backup and verify; Ferrante 512 write/wipe on PCB 6 / L14 | v1.3 PCB 4 / L1 is qualified for read-only GBA. v1.4-family L12–L15 reads GB / Color and GBA. No GBA writing, save RAM or RTC |
| Epilogue GB Operator | GB / Color and GBA ROM detect, read, backup and verify | Native bulk USB; save memory, RTC and cartridge programming are pending |

The GBxCart driver accepts PCB 4 (v1.3) with legacy R1–R30 or extended L1 for
read-only GBA access. Its hardware cannot be software powered off, so Cartridge
Studio releases the bus after reading and tells the user to unplug USB before
handling the cartridge. Game Boy / Color access remains blocked on v1.3 until
that voltage path is qualified.

PCB identifiers 5 (v1.4) and 6 (v1.4a/b/c) support GB / Color and GBA. The latter
identifier does not distinguish the printed a/b/c revision. Physical qualification
currently covers PCB 6 with L14; the other accepted firmware revisions share the
protocol but have not been tested on physical hardware here. Unknown firmware,
older hardware, bootloader mode and unqualified mode-switch hardware are refused
before cartridge power or bus access. Firmware is never updated automatically.

The manufacturer names remain in hardware identifiers, driver selection and
protocol documentation. They are not the application brand. The shared crates
and commands use `cartridge-*`, independent of a particular reader.

## Choosing a reader

Use the desktop sidebar's **Reader** selector, or **U** in the terminal to cycle
Automatic / INLretro / GBxCart RW / GB Operator. Then use **Check USB** / **F5**. The choice is
saved in the shared preferences. Changing it clears detection and any pending
write review. With any reader, choose Game Boy / Color or Game Boy Advance and
Automatic for ordinary reading. For a Ferrante 512, select its explicit board
profile for write/wipe. A note beside the desktop buttons explains unavailable
actions; the terminal shares the same capability checks.

```sh
cartridge doctor                            # automatic selection
cartridge --reader gbxcart doctor
cartridge --reader gbxcart detect --platform gameboy
cartridge --reader gbxcart gameboy read --no-artwork
cartridge --reader gbxcart gameboy backup
cartridge --reader gbxcart gameboy verify /path/to/game.gbc
cartridge --reader gbxcart --port /dev/ttyUSB0 doctor
cartridge --reader operator doctor
cartridge --reader operator gameboy backup
cartridge --reader operator gba backup
```

Automatic discovery considers the exact INLretro and GB Operator USB identities,
plus CH340 serial ports (1a86:7523) without opening arbitrary serial ports. CH340 is also used by unrelated
devices: it is only a candidate. An identity handshake must confirm the PCB,
extended firmware, reader name and power-control flags before cartridge access.
More than one candidate is an actionable ambiguity error; select a reader family,
use an explicit GBxCart serial port in the CLI, or disconnect additional devices.
On macOS an explicit port typically starts with `/dev/cu.`. Mac hardware and release
packaging still require separate qualification.

A port cannot be used with `--reader inlretro` or `--reader operator`. Serial ports are exclusively locked;
close FlashGBX and other reader applications if the port is busy. The driver does
not toggle DTR or reset the reader firmware on opening. Serial reads have bounded
timeouts and cancellation checks. Bank writes are restricted to supported MBC
registers. Flash commands are confined to the explicit SST39SF040 AUDIO/MBC5
profile and require chip identification before erase/programming.
GBxCart v1.4-family cartridge power is turned off after normal completion, errors
and cancellation; v1.3 releases the bus because it has no software power-off.
An unconfirmed cleanup tells the user to unplug USB before handling the cartridge.

For Linux permission errors on INLretro or GB Operator, run
`sudo cartridge usb-setup`, or install `70-cartridge-studio.rules` manually and
reload udev. Each rule includes exact product strings as well as VID/PID. GBxCart
uses the serial-device group provided by the distribution (`uucp` on Arch;
commonly `dialout` elsewhere), then requires signing in again. We do not install
a broad CH340 access rule that would also grant access to unrelated serial hardware.

## GB Operator

Close Epilogue Playback before using the reader. Cartridge Studio validates the
exact Epilogue product identity, temporarily detaches Linux's CDC control driver,
claims only the data interface, and restores the driver when the operation ends.
Commands use the device's CRC-protected 64-byte frames and periodic flow-control
acknowledgements. Timeout, rejection, busy-interface, permission and disconnect
errors each give a specific next action.

GB / Color reads use the cartridge-size record reported by the device, then pass
the raw image through the same mapper, header, checksum, two-pass verification and
catalog pipeline as the other readers. Cartridge Studio contains no Playback code
or runtime. GBA records do not include the ROM size, so the driver first reads the
256-byte header. A unique catalog game-code size becomes provisional until the
complete hash matches; unknown games use the shared mirroring/full-window sizing
path. Each backup pass is a separate complete device stream. Save commands are
absent. Provisional ROM programming remains disabled in normal builds: the
firmware 9.5.0 / Ferrante 512 hardware attempt failed erase/program qualification.
Use GBxCart RW or INLretro with the qualified Ferrante profile for writing.

The GB reader accepts the `0x20`, `0x21` and `0x22` cartridge-kind records;
previous builds incorrectly rejected Color variants such as the `0x22` record
returned by the Ferrante test cartridge. Unknown kinds still stop before a ROM
read. Reports retain the observed kind, record and firmware version when present.
See [Operator programming investigation](operator-programming.md) for the
remaining programming limitations and the qualification plan.

## Writing and wiping a Ferrante 512

Select **Game Boy / Color**, **GBxCart RW**, and the board profile
**Ferrante 512 · SST39SF040 AUDIO/MBC5**. Choose **Check USB**, then **Inspect**.
Load a ROM and choose **Write…**, or choose **Wipe…** without a ROM. Review the
operation and type its confirmation word. Write includes erase automatically.

This first GBxCart writing implementation requires PCB 6 (v1.4a/b/c) and L14
firmware, at 5 V. Other reader firmware can still use Automatic ROM reading.
The firmware is never updated automatically. The physical board must have
512 KiB SST39SF040 flash (ID BF B7), MBC5 banking and AUDIO write-enable wiring.
A known game or an MBC5 header alone cannot identify that wiring. Retail ROMs
and unknown flash boards remain blocked.

ROM sources must use MBC5 type 0x19, have no save RAM, fit within 512 KiB and
have valid header/global checksums and an exact declared size. The source is
pinned and saved before hardware access. Both operations retain two matching
full-chip backups and repeat flash identification before erase. Write checks
all 512 KiB for blank bytes, verifies each programmed bank, then compares two
power-cycled full-chip reads with the source plus blank unused capacity. Wipe
also checks all 512 KiB twice, including a power cycle. Raw files and reports
remain available on failure or cancellation; erase/program replies are never
blindly retried. Save-memory access is not part of this profile.

```sh
cartridge --reader gbxcart gameboy probe --profile sst39sf040-audio-mbc5
cartridge --reader gbxcart gameboy write /path/to/game.gbc --profile sst39sf040-audio-mbc5 --yes
cartridge --reader gbxcart gameboy wipe --profile sst39sf040-audio-mbc5 --yes
```

## What a backup contains

GB / Color backups contain the complete ROM size declared by the validated cartridge
header. GBA has no header size field; see the GBA section below. They do not include unused flash capacity, save RAM or RTC contents. Two
reads must agree for backup and verification; ordinary Read permits a single-pass
option. CRC32, SHA-1 and SHA-256 are recorded. Strict ordinary reads also require
the Game Boy global checksum. A known-game match is based on exact bytes, never on
the header title alone, and does not prove the physical flash board is writable.

## Game Boy Advance

Use `cartridge --reader inlretro gba read`, `cartridge --reader gbxcart gba read`
or `cartridge --reader operator gba read`, or select **Game Boy Advance** and
**Read** in the GUI/TUI. This release supports only standard linear GBA ROMs up to
32 MiB. INLretro uses its shared side-entry GB/GBA connector; GBxCart uses its
32-pin cartridge connector. GBA initialization always selects 3.3 V before access.
Detect validates the boot logo, fixed header
byte and header checksum, and shows the game code. It does not access saved games.

Automatic sizing first considers a unique size associated with the header's game
code in the offline catalog. The complete dump must then match a catalogued SHA-1
and size; a header title or code alone never establishes game identity. If this
check fails, raw reads remain available and the operation reports an unconfirmed
size. Without a unique catalog hint, several separated samples are checked for ROM
mirroring. This is an estimate, marked as such in the report; otherwise the full
32 MiB address window is read. Unknown images retain a visible warning. No padding,
mirror trimming, EEPROM-area patching or invented bytes are applied.

Override the size using **GBA ROM size** in the GUI workspace or Preferences,
**G** in the TUI, or `--rom-size-mib 1|2|4|8|16|32` with CLI read/backup/verify.
The GUI/TUI additionally expose 256 and 512 KiB for small homebrew images.
Selecting 32 MiB preserves the full linear address window when the size is unknown.
The backup directory needs at least 160 MiB free. A second read is compared in
blocks without retaining a second full ROM in memory. Each raw pass and the final
`.gba` file are saved separately. Verify compares the complete bytes to the loaded
file, and always requires two reads. GBA header checks are mandatory; GBA has no
built-in checksum for its entire ROM.

The GBxCart GBA driver uses 1 KiB paced serial bursts with firmware word-address increments.
An incomplete ROM reply can be retried at the exact original address, at most
twice per block and 16 times per operation. Retry counts appear in reports. Other
errors are not retried. Timeout recovery never pads the response or weakens the
second-read comparison or catalog confirmation. Persistent faults remain actionable
errors, with raw files retained and power-off attempted.

DACS storage is explicitly rejected. Other banked boards, 3D-memory video carts,
SD-card flash carts, save SRAM/FRAM/EEPROM/Flash and RTC are outside this release.
Unknown board types cannot be reliably identified from their game header.
No flash-ID probing, save writes, erase commands or bootleg unlock writes are sent.

## Physical qualification

The owner's Ferrante 512 cartridge on a PCB-6 GBxCart RW / L14 contained a
256 KiB MBC5 SolarStriker Color ROM. Native automatic discovery and header detection
passed. Two complete reads were identical, with valid header/global checksums:

- SHA-256: `d651deb621f102b0e3603bd4f2b0168b41014ca34ecaf09fb75b01c87c5482af`
- CRC32: `ef736140`
- Exact catalog match: none (modified SolarStriker).

The installed TUI also produced the same two-pass dump through its normal
controls, with resizing, file browsing and clean exit verified. The installed
CLI verified two fresh cartridge reads against the retained ROM.

The exact dump passed a PyBoy cold boot in both CGB and DMG modes: greeting, title,
opening story, stage-one start, left/right movement, firing, enemies and audio,
including 12 seconds of combat per model. This is a gameplay smoke test, not a
complete campaign test. Dumps, emulator output and diagnostic reports remain in
ignored `tmp/gbxcart-reference/` and `tmp/cartridge-backups/`. No physical erase or
program operation was performed.

The same PCB-6/L14 reader was qualified with the owner's retail **Final Fantasy
Tactics Advance (USA)** GBA cartridge (`AFXE`, revision 0). Two complete 16 MiB
reads were identical and matched the catalog exactly:

- SHA-256: `43fc8204c6dceee58828aebc7af0c72eb807e99f35ad641c8bb0a4fa8b6edc19`
- SHA-1: `4ac05441f4de70a4ec3dd932116346c61b8783d9`
- CRC32: `5645e56c`

Initial unpaced reads exposed incomplete serial replies. Their partial dumps and
reports are preserved. The final paced, two-pass qualification completed with
zero retries. mGBA 0.10.5 booted the exact dump without a retail BIOS, rendered the
title and opening school scene, played audio and accepted menu/dialogue inputs.
The installed CLI identified it exactly and cached optional artwork; the installed
TUI loaded it through its file browser, displayed checksums and restored correctly
after resizing. GBA ROMs, screenshots and test tools remain in ignored
`tmp/gba-qualification/`; no ROM, saved game, BIOS or emulator is in the application
package. Save memory and cartridge flash were never written.

On 2026-09-27, the same Final Fantasy Tactics Advance cartridge was read through
a GBxCart RW v1.3 (PCB 4 / L1). Two complete 16 MiB reads completed with no
retries and matched each other, the PCB-6/L14 dump and the exact catalog record:

- SHA-256: `43fc8204c6dceee58828aebc7af0c72eb807e99f35ad641c8bb0a4fa8b6edc19`
- SHA-1: `4ac05441f4de70a4ec3dd932116346c61b8783d9`
- CRC32: `5645e56c`

mGBA 0.10.5 booted the v1.3 dump and accepted input. This qualified the L1
ACK-less extended handshake, fixed-voltage GBA setup and bus-release cleanup.
The retail cartridge's save memory was not accessed and nothing was written.

Also on 2026-09-27, an Epilogue GB Operator with device version 1.11 read the
owner's original **SolarStriker (World)** Game Boy cartridge. Two complete 64 KiB
reads were identical and matched the catalog exactly:

- SHA-256: `fece5b09e599c1f8116602fcabd558cc8dbbfe1c6c253853c5e871b0d732c11c`
- SHA-1: `a8d6acb026b0d0a8a2bcaea094951e915a3deb80`
- CRC32: `11817103`

PyBoy reached Stage 1, produced changing frames and audio, and responded to the
scripted controls. The test used ROM reads only; saved games and cartridge flash
were not accessed. Qualification artifacts remain under ignored
`tmp/operator-qualification/` and `tmp/cartridge-backups/`.

The same GB Operator then read the retail **Final Fantasy Tactics Advance (USA)**
GBA cartridge. Its `0x30` cartridge record and a 256-byte native Rust probe
reported the valid `AFXE` header. Two independent 16 MiB streams matched each
other, both qualified GBxCart dumps and the exact catalog record:

- SHA-256: `43fc8204c6dceee58828aebc7af0c72eb807e99f35ad641c8bb0a4fa8b6edc19`
- SHA-1: `4ac05441f4de70a4ec3dd932116346c61b8783d9`
- CRC32: `5645e56c`

mGBA 0.10.5 booted the Operator dump, rendered the title screen, generated audio
and accepted scripted input. The ROM and screenshots remain in ignored
`tmp/operator-gba-qualification/`. Save memory was not read or written.

Offline tests cover fragmented serial replies, firmware rejection before power,
negative acknowledgements, premature disconnects, cancellation cleanup, two-pass
mismatch retention, the MBC5 high bank bit and forbidden register writes. Existing
INLretro golden transcripts and write-safety simulations remain unchanged.

## GBxCart flash qualification status

The connected Ferrante 512 returned flash ID `BF B7`; two native GBxCart reads
of its complete 512 KiB matched on 2026-09-16. Its 256 KiB SolarStriker Color
ROM has SHA-256 `c61a6c3c8854415eb8465214b42f349282e8df079721262133c198b90318cd7a`;
the unused 256 KiB is blank. The full-chip SHA-256 is
`97f50e2bd596ad75530da071daf71d4e61123c99960fd5fe0c54f084297ec017`.

Physical Rust erase/program qualification is pending a separately authorized
hardware session and recorded results.
Offline tests cover all 32 banks, backup mismatch, failed blank checks, failed
programming, final readback mismatch, negative acknowledgements, serial timeout,
disconnection, cancellation, unsupported firmware and source changes before
opening a reader. Raw backups and the qualification record are in ignored
`tmp/gbxcart-write-qualification/` and `tmp/cartridge-backups/`.

## Protocol references

Protocol facts were checked against the device command/variable declarations and
reader initialization in [FlashGBX](https://github.com/lesserkuma/FlashGBX),
particularly `FlashGBX/hw_GBxCartRW.py` and `FlashGBX/LK_Device.py`, and the
[manufacturer's GBxCart RW project](https://github.com/insidegadgets/GBxCart-RW).
The host implementation here is independently written Rust; no FlashGBX Python
source, original host binary or updater is shipped or executed.

The Ferrante board commands are checked against the upstream
[SST39SF040 AUDIO profile](https://github.com/lesserkuma/FlashGBX/blob/2d9682c0d1abbb556c27d7ba1715453c9776b563/FlashGBX/config/fc_DMG_SST39SF040_AUDIO.txt).
GBxCart uses firmware A7 configuration, D4 flash sequences and D3 byte programming;
normal mapper writes stay on B2. No upstream host implementation is bundled.

## Spansion S29GL032M R4 (unreleased)

The checkout adds **Spansion S29GL032M R4 · WR/MBC5 · 4 MiB** to the shared
GUI/TUI/CLI profile list. It requires GBxCart PCB 6 / L14 and the physically
confirmed board described in [the investigation](spansion-board-investigation.md).
This is not a generic S29GL032M profile: other revisions, swapped data wiring,
and other readers are rejected. Physical testing on PCB 6 / L14 erased and
blank-checked all 4 MiB, programmed a distinct pattern across all 256 banks,
and matched it in two power-cycled full-capacity readbacks.

The reader selects 3.3 V before powering the cartridge. Every destructive
transaction requires responding ID `01 7E 1A 00`, the matching CFI command set
and bottom-boot sector layout, unchanged ROM after identification, and a checked
MBC5 bank-zero alias. It retains two matching 4 MiB backups and repeats identity
before erasing. Programming uses the switchable ROM window, including bank zero,
to avoid writes to the ROM bank registers. No automatic sector unlocking is used.

Write accepts MBC5 type `0x19` without RAM, or types `0x1A`/`0x1B` with 8/32 KiB
RAM, up to 4 MiB. Header, size and global checksums must match. MBC3/RTC, rumble
and larger saves are rejected. This does not imply save-memory support: SRAM
is not accessed, backed up or restored. Write erases first, checks every byte for
blank state, verifies each bank, and compares two fresh full-capacity reads.
Wipe retains the same backups and performs two full blank checks.

```sh
cartridge --reader gbxcart gameboy probe --profile s29gl032m-r4-wr-mbc5
cartridge --reader gbxcart gameboy check /path/to/game.gbc --profile s29gl032m-r4-wr-mbc5
cartridge --reader gbxcart gameboy write /path/to/game.gbc --profile s29gl032m-r4-wr-mbc5 --yes
cartridge --reader gbxcart gameboy wipe --profile s29gl032m-r4-wr-mbc5 --yes
```

`gbxcart_spansion_program_qualification` is a separately invoked, destructive
hardware example, never part of routine tests. It requires three identical retained
full-capacity reads, a completed read-only report, a pinned SHA-256 and explicit
confirmation. It writes/verifies a synthetic pattern across every bank, then
restores the original bytes with the same backup, blank, bank and final checks.
Its narrowly pinned raw-restoration adapter is confined to that example, because
an existing backup can have an incompatible header or incorrect checksum. There
is no GUI/worker option to bypass normal source compatibility validation.
