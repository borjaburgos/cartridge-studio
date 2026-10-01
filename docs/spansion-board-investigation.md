# S29GL032M / MBC5 board investigation

This is a separate cartridge from Ferrante 512. The user supplied front/back PCB
photos showing a Spansion S29GL032M90TAIR4, MBC5-labelled controller circuitry,
V62C518256L SRAM and a battery holder. The exact PCB model is partly obscured.
The Nintendo marking is not evidence that the board is an original retail PCB.

The loaded game has header title `POKEMON TRE`, cartridge type `0x13` (MBC3),
1 MiB declared ROM and 32 KiB declared RAM. These are properties of the loaded
image, not physical board identification. Two original 1 MiB reads match, but the
stored global checksum is incorrect and the offline catalog has no exact match.
No authenticity conclusion follows from the checksum alone.

## Diagnostic scope

`gbxcart_spansion_read_qualification` is an explicitly invoked development
example for the user-confirmed photographed board on GBxCart PCB 6 / L14.
It is not run by automated tests or offered as a qualified write profile.

The Rust diagnostic initializes the GB bus at the 3.3 V reader setting and uses
MBC5 bank registers to read the nominal 4 MiB physical capacity. It checks two
headers against the retained game before each pass and requires the complete
first MiB to match the earlier dump. Two full reads must agree before any flash
query. It then reads software ID using documented normal and swapped-D0/D1
unlock variants on WR, reads CFI, and resets the chip to ROM mode after each
query, including cancellation/error paths. A third full read must match the
pre-query backups. No erase, program, unprotect or save-memory commands are sent.

The query report retains raw ID/CFI records and labels programming unqualified.
Recognizing a CFI capacity is not sufficient to enable writes. The production
Ferrante programmer still requires its own exact SST chip identification; these
diagnostics never set its programming authorization flags.

Raw ROMs, reports and supplied photos are not repository/distribution assets.
Generated investigation data stays under `tmp/gbxcart-new-cartridge-20260930/`.

## Hardware result

The completed run after reseating the cartridge used GBxCart v1.4 / PCB 6,
firmware L14, at the 3.3 V setting. Normal AA/55 unlock commands entered software
ID mode; the swapped-D0/D1 variant did not. Manufacturer byte `01` and device
bytes `7E`, `1A`, `00` at byte offsets `0`, `2`, `0x1C`, `0x1E` agree with the
photographed S29GL032M R4 part. ID and CFI bytes appeared duplicated at adjacent
addresses. CFI decoded with stride 2 and no data-bit swap: `QRY`, command set
`0002`, capacity 4,194,304 bytes and two erase regions.

Two independent full-capacity reads matched. The first 1 MiB exactly matched
the retained game dump; the remaining 3 MiB read as `FF`. A third full-capacity
read after the queries matched both backups, confirming unchanged ROM contents.
The full-capacity SHA-256 is
`59f4fc8ccf087df64c31912b09cf2ca771b5dcd11e156e4f94672cf4fd18457f`.
Evidence is in `reseated-capacity/report.json` and the three `.bin` captures
under the investigation directory above.

An earlier attempt returned zeros and failed the retained-game comparison before
any identification query. Keep that capture separate from the verified backups;
it is not evidence of an erased chip. The successful reseated run does not prove
the exact cause of the earlier connection failure.

That initial run qualified read and identification only. SRAM was not accessed
or backed up. The subsequent erase/program qualification is recorded below.

## References

- [Spansion S29GL-M datasheet](https://datasheet.octopart.com/S29GL128M90TFIR10-Spansion-datasheet-512378.pdf): software autoselect, reset and CFI query commands; S29GL032M capacity.
- [FlashGBX S29GL032M90T board profile](https://github.com/lesserkuma/FlashGBX/blob/master/FlashGBX/config/fc_DMG_S29GL032M90T.txt): WR routing and swapped-D0/D1 command variant for its explicitly named boards. Those PCB names do not identify this photographed board.
- [FlashGBX GBxCart transport](https://github.com/lesserkuma/FlashGBX/blob/master/FlashGBX/LK_Device.py): GBxCart firmware command/variable framing.

These are interoperability references. The runtime remains native Rust and does
not invoke or bundle FlashGBX.

## Erase/program implementation

The subsequent implementation uses normal AA/55 unlock commands, WR routing,
3.3 V, unbuffered AMD byte programming and a 120-second chip-erase deadline.
Only the exact R4 ID and observed CFI geometry are accepted. Programming addresses
all banks through `0x4000–0x7FFF`, including bank zero after checking its alias;
unlock commands remain in the fixed low window. GBxCart's forced-bank-1 option
is disabled for this profile. The shared transaction now takes physical capacity
from the reader instead of assuming 512 KiB.

The retained game is MBC3-labelled with an invalid global checksum. The normal
write path intentionally rejects that as a new compatible MBC5 source. The
explicit development qualification can restore its pinned full-capacity backup
exactly, without modifying the header or claiming MBC3/RTC support.

Offline tests cover full-capacity writes/wipes and rejection before erase for
ID, CFI and backup failures, plus blank verification and programming failures.
The first authorized hardware run erased and blank-checked all 4 MiB, then
programmed and verified 145 banks. The next bank read timed out with 4,089 of
4,096 requested bytes received. The transaction failed without reporting success;
its original full-capacity backups match the earlier read-only backups. Evidence
is retained in `program-qualification/pattern/report.json` under the investigation
directory. Erase/program replies were not retried.

Spansion ROM reads now use paced 1 KiB bursts to reduce pressure on the CH340
serial bridge, matching the existing GBA read envelope. An incomplete response
still stops the operation. The partial first run is not a passing qualification.

## Completed paced-read qualification

The rerun completed on GBxCart PCB 6 / L14 at 3.3 V, using 1 KiB ROM reads
with a 1 ms pause between requests. Both the synthetic-pattern transaction and
original-content restoration completed successfully. Each retained two matching
full-capacity backups, blank-checked all 4 MiB after erase, verified all 256 banks,
and compared two power-cycled final 4 MiB readbacks.

The pattern includes unique data across every bank, both halves and bank ends.
Its final SHA-256 is
`6bb0c2abcb82fb8ad79e15f649033936c70e78f8f532baf77a1e7b7df8a9615c`.
The restored cartridge's SHA-256 is
`59f4fc8ccf087df64c31912b09cf2ca771b5dcd11e156e4f94672cf4fd18457f`,
identical to the original backup. The original header and incorrect global
checksum were preserved exactly; SRAM was never accessed.

Reports and raw evidence are under `program-qualification-paced/` in the
investigation directory, with an aggregate `qualification-summary.json`.
The GUI/TUI/CLI profile `s29gl032m-r4-wr-mbc5` is now physically qualified for
this exact normal-data-wiring R4 board and reader/firmware combination. This does
not qualify different board wiring, flash revisions, save memory, or other readers.
