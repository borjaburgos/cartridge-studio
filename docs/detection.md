# Cartridge detection

Choose your reader and the physical **NES**, **Famicom**, **Game Boy / Color**
or **Game Boy Advance** slot, keep the board on **Automatic**, then press
**Detect** (`P` in the terminal interface). Connect only one cartridge;
unplug USB before changing it. Neither physical button is needed. The reader's
USB identity identifies the programmer, not which INLretro slot contains a cartridge,
so the platform choice is still required.

| Cartridge | Automatic result | Available operations |
| --- | --- | --- |
| Supported UNROM-512 interface: SST39SF040, 512 KiB PRG, 32 KiB banked CHR RAM | Hardware match, flash ID, memory and mirroring | Read, back up, verify; write/wipe review after detection |
| GB/C ROM-only, MBC1, MBC2, MBC3, MBC5 with a valid header | Title, mapper, declared ROM/RAM size and color capability | Read, back up ROM, verify; declared RAM is not accessed |
| Standard linear GBA with a valid header | Title, game code and header validity; catalog size hint or later size estimation | Read, back up ROM, verify on supported readers; no saves |
| Blank or invalid-header GB flash cartridge | Cannot infer mapper/flash wiring | Select its exact supported manual profile if known |
| NROM-128 / NROM-256 | Not automatically identifiable | Select a known manual profile; read-only |
| Other or ambiguous Famicom boards | No profile selected; reason and next action displayed | Blocked until a supported board is established |

Loading a ROM does not identify the physical board. NROM-like mirrored reads
can also come from banked boards, blank chips or poor contacts, so the app does
not treat them as proof. The UNROM-512 result identifies a compatible hardware
interface, not the PCB manufacturer or its printed revision. The existing
`broke-unrom512` profile is used for that interface; physical qualification was
performed on a Broke Studio UNROM-512 v2.1 with vertical mirroring.

Famicom detection compares repeated bus samples, requests the SST software ID
twice through the mapper-30 command interface, requires a distinct response
from ordinary ROM data, and checks that the original ROM samples return. It
then tests four independent CHR RAM banks, restores and verifies the original
test bytes, measures mirroring and checks the fixed final PRG window. It sends
no erase or program command. This is a bounded probe of the supported flash
interface, not a universal identification algorithm for arbitrary EEPROM,
development, or custom hardware. Detection failures include the evidence and
failed check in Details / Activity.

Game Boy detection compares two headers and validates the boot logo, header
checksum, mapper and ROM size. It never tries flash-enable pin wiring. A valid
MBC5 header does not prove a writable MBC5 cartridge. The separate qualified
AUDIO/MBC5 write profile must still be selected explicitly.

GBA detection validates its boot logo, fixed header byte and header checksum.
GBA headers contain no ROM length: any catalog size hint remains provisional
until a full read matches the exact catalog hash. Automatic sizing and manual
overrides are described in [Reader support](readers.md#game-boy-advance).

Detection enables a review, never an immediate erase. The review names the
resolved hardware and pins the loaded ROM checksum. The normal physical
checks, source validation, two complete backups and write readback checks
remain mandatory. Rechecking the reader, changing profile/platform, or a
hardware-operation error clears the detected profile. Detection is not saved
between sessions. Automatic Famicom reads/backups/verifications detect again.

Command line:

```sh
cartridge detect --platform famicom
cartridge detect --platform gameboy
cartridge --reader operator detect --platform gba
cartridge --reader inlretro detect --platform nes
cartridge --json famicom probe
cartridge famicom read                    # detect, then read twice
cartridge famicom backup                  # detect, then back up twice
cartridge famicom verify '/path/game.nes'  # detect, then compare twice
```

CLI offline checks and erase/write still require an explicit resolved profile.
Use the profile printed by detection, for example `--profile broke-unrom512`.
No automatic profile is inferred from a source file.

Protocol references: [INL mapper-30 implementation](https://github.com/InfiniteNesLives/INL-retro-progdump/blob/7f21176c2ff220cc36fd37355a88d7f7ecf6ff9f/host/scripts/nes/mapper30.lua),
[SST39SF040 software ID and commands](https://ww1.microchip.com/downloads/aemDocuments/documents/MPD/ProductDocuments/DataSheets/SST39SF010A-SST39SF020A-SST39SF040-Data-Sheet-DS20005022.pdf),
[Game Boy cartridge headers](https://gbdev.io/pandocs/The_Cartridge_Header.html).

Offline protocol checks:
`CARGO_HOME="$PWD/tmp/toolchains/cargo" cargo test --locked --offline -p cartridge-core --test protocol`. Hardware testing
uses full ROM reads before and after detection; erase/program tests are not
part of detection qualification.
