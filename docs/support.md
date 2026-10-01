# Hardware support and troubleshooting

This is the supported envelope for the **first public beta, 0.1.0-beta.1**.
Treat it as a testing release; hardware coverage and physical qualification
are incomplete. A platform name
does not imply every cartridge, mapper or reader revision works. The application
refuses unsupported write operations; exact electrical profiles govern programming.
The detailed evidence and hashes are in [Reader support](readers.md).

## Readers and operations

| Reader configuration | GB / GBC ROM | GBA ROM | NES / Famicom ROM | Write / wipe |
| --- | --- | --- | --- | --- |
| INLretro | Read, backup, verify | Read, backup, verify; physical qualification pending | Supported profiles only, distinct 72-pin and 60-pin slot selections | Supported GB and UNROM-512 profiles; physical Rust qualification pending |
| GBxCart RW v1.3, PCB 4, R1–R30 or L1 | Blocked pending voltage-path qualification | Read, backup, verify; physically qualified on L1 | Unsupported | Unsupported |
| GBxCart RW v1.4-family, PCB 5/6, L12–L15 | Read, backup, verify | Read, backup, verify | Unsupported | Ferrante 512 only on PCB 6 / L14; physical Rust qualification pending |
| Epilogue GB Operator | Read, backup, verify | Read, backup, verify | Unsupported | Unsupported |

GBxCart physical read qualification covers PCB 6 / L14 and PCB 4 / L1; accepting
other listed protocol revisions does not mean they have all been physically
tested. GB Operator read qualification covers device version 1.11. The software
does not update any reader's firmware automatically.

## Cartridge profiles

| Family / profile | Scope | Detection and limits |
| --- | --- | --- |
| GB / GBC automatic | ROM-only up to 32 KiB; MBC1/MBC3 up to 2 MiB; MBC2 up to 256 KiB; MBC5 up to 8 MiB | Two matching validated headers; reads ROM only. Header mapper/size does not establish flash wiring. Specialized mapper variants and peripherals are not qualified. |
| GB `sst39sf040-audio-mbc5` | Ferrante 512 / SST39SF040 AUDIO/MBC5, 5 V, 512 KiB | Explicit manual board selection. Flash ID BF B7; MBC5 type `0x19` ROM without save RAM, exact declared size and valid checksums. INLretro or GBxCart PCB 6 / L14. |
| NES / Famicom `broke-unrom512` | Broke Studio UNROM-512 v2.1 interface, mapper 30, 512 KiB flash, 32 KiB CHR RAM | Hardware detection of flash, banks and mirroring; does not prove manufacturer/revision. Write requires matching memory and mirroring; no separate PRG RAM, CHR ROM or trainer. |
| NES / Famicom `nrom-128` | Mapper 0, 16 KiB PRG ROM, 8 KiB CHR ROM | Manual known-board selection, read/backup/verify only. Physical qualification pending. |
| NES / Famicom `nrom-256` | Mapper 0, 32 KiB PRG ROM, 8 KiB CHR ROM | Manual known-board selection, read/backup/verify only. Physical qualification pending. |
| GBA automatic | Standard linear ROM, up to 32 MiB, 3.3 V | Mandatory header checks; catalog size hints require a complete hash match. Unknown sizing uses sampled mirroring or the full window, with warnings and manual overrides. Read-only. |

An NES/Famicom cartridge does not contain an iNES file header. Cartridge Studio
creates one from its supported board profile, observed mirroring and full physical
capacity. The report distinguishes the selected physical slot from the ROM family.
Other NES mappers, including MMC1 and MMC3, are not implemented.

## What is preserved

- **Read:** ROM bytes and an operation report. Ordinary reads can opt out of
  double reads and strict Game Boy global checksums; GBA header checks remain mandatory.
- **Backup / Verify:** two complete matching ROM reads, hashes and reports.
  Verify also compares against the loaded source file. GB automatic mode follows
  the declared ROM length; a full-chip flash transaction also retains unused capacity.
- **Write / Wipe:** exact supported hardware only; retained source where applicable,
  two full backups before erase, blank verification, and mandatory final readbacks.
  Writes also verify each bank. Read preferences cannot disable these checks.
- **Offline tools:** GB/GBC/GBA/iNES/NES 2.0 inspection, SHA-256/SHA-1/CRC32
  comparison, supported-profile compatibility checks, exact catalog identity,
  optional artwork and shared operation history.

Save RAM, EEPROM, save Flash, FRAM, RTC, Game Boy Camera photos, special
peripherals, GBA DACS/banked/video cartridges and SD-card flashcart management
are outside the current release. ROM backup does not back up a saved game.
Retail mask ROM cannot be rewritten. A ROM hash cannot establish cartridge
authenticity, PCB model or safe write wiring.

## Qualification means evidence

“Implemented” means the software path exists. “Physically qualified” means a
specified reader, firmware and cartridge completed recorded hardware tests.
Simulated protocol tests do not replace that evidence. The native Rust write/wipe
paths have simulations and safety checks, but physical erase/program qualification
is still outstanding; earlier implementation tests are retained as historical evidence.

Verified native reads include INLretro MBC1, GBxCart PCB 6 / L14 MBC5 and GBA,
GBxCart PCB 4 / L1 GBA, and GB Operator GB and GBA. Known retail dumps match
catalog hashes and have emulator smoke tests. Those tests demonstrate boot,
graphics, audio and input where recorded, not completion of every game.

## Common problems

| Message or symptom | Next action |
| --- | --- |
| No reader / access denied | Use Check USB or `cartridge doctor`; check a data-capable cable and the [Linux access setup](install.md#linux-device-access). |
| Several reader candidates | Select a reader family, specify a GBxCart `--port`, or disconnect extra candidates. A CH340 port alone is not proof of GBxCart identity. |
| Reader busy | Close Playback, FlashGBX and any other process using that reader, then retry. |
| Wrong platform / GBA indicator | Select the cartridge's actual family and slot. Disconnect USB before reseating or exchanging cartridges. |
| Invalid header or different bytes between reads | Retain the raw files/report. Unplug USB, reseat and inspect contacts, then retry. Do not bypass failed consistency checks to label the result verified. |
| Unknown board | Check PCB/chip markings and the profile table. A known game title is insufficient; request a new profile rather than substituting a similar one. |
| Write disabled | Check reader, firmware, exact flash profile and source compatibility. Retail ROM and all GBA writing are unsupported. |
| No database match | The game may be homebrew, patched, uncatalogued or differently padded. Inspect the read evidence; no match alone does not prove a bad read or bootleg. |
| Artwork unavailable | ROM preservation is already complete. Retry Refresh artwork with a network connection and writable library, or use cached images offline. |
| Window too small | Enlarge the GUI to 1000 × 700 logical pixels or terminal to 120 × 32 cells. Active operations continue; safe Stop remains available. |
| Interrupted write or unconfirmed cleanup | Keep the backups, source and diagnostic report. Follow the recovery instructions; do not assume the cartridge is usable until verified. |

When reporting a problem, include application version, OS, reader PCB/firmware,
slot/profile, action and the error code/report. Redact personal paths and device
serial numbers. Do not attach commercial ROMs, BIOS files or private saves. Use
the [bug or hardware request forms](https://github.com/borjaburgos/cartridge-studio/issues/new/choose).

## Unreleased Spansion profile

The current development checkout adds `s29gl032m-r4-wr-mbc5` for the confirmed
4 MiB / 3.3 V / normal-data-wiring board on GBxCart PCB 6 / L14. Read and
identification are physically verified; erase/program is implemented with
simulated tests and awaits physical qualification. It is not part of the
published beta's support matrix above. See [reader instructions](readers.md#spansion-s29gl032m-r4-unreleased)
and [the board investigation](spansion-board-investigation.md).
