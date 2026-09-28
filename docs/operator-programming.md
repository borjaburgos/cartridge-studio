# GB Operator programming investigation

Status: the separate Rust transaction and a provisional legacy USB adapter are
implemented. A physical firmware 9.5.0 / Ferrante 512 attempt failed erase/program
qualification. Production ROM writing and wiping remain disabled, including a
guard inside the adapter. Synthetic tests do not override this hardware result.

## Verified locally

On 2026-09-27, the connected GB Operator advertised USB device version 1.11.
Its cartridge record identifies firmware 9.5.0; USB device version and firmware
version are different fields.

The user explicitly confirmed the physical cartridge as a Ferrante 512. Before
the write attempt, its existing ROM was
512 KiB, MBC5, Color-compatible, with valid header and global checksums. Two
independent native Rust backups match. Separately captured read-only backups
match those native results. This proves repeatable reading, not the physical
flash chip model or capacity. In particular, a ROM header is not a flash ID.

The existing parser accepted only cartridge kind `0x20`. This cartridge returns
`0x22`; the GB / Color record parser now accepts `0x20`, `0x21`, and `0x22`.
Unknown kinds remain blocked. The current image has an empty title and identical
switchable-bank contents; this observation alone does not establish a fault.

The requested SolarStriker DX Captain Felipe v0.7.2 `.gb` asset is 262,144 bytes,
uses MBC5, and passes both checksums. Its release digest is:

`e0077aac4c8d499b5152c0a62b0defeff1e07c3ac44767f8a932bef453cc7553`

ROMs, device identifiers, backups, vendor binaries, and analysis output stay in
ignored local research storage. None belong in a public distribution.

## Why the existing flash transaction cannot simply be enabled

The qualified INLretro / GBxCart transaction independently identifies the physical
flash chip, saves two complete backups, checks an erase by reading every byte,
programs and reads back each bank, then performs two complete final reads.

The legacy Operator host protocol exposes a combined Write Game operation:
a command initiates erasing, the firmware emits erase status, and the host then
streams ROM data. A transfer acknowledgement is not a readback or blank check.
The public reverse-engineering notes identify command `0x01` but do not document
its parameters or flash-identification guarantees. Host interoperability research
also shows a newer streaming protocol with a separate Detect Flashcart operation;
that does not establish that firmware 9.5.0 implements it.

We have not established a supported way on this firmware to obtain an exact
physical flash identity, independently read the erased chip before programming,
or interleave bank readbacks with the active write stream. Do not guess command
parameters, send a write command as a detection probe, substitute a cached ROM
for a readback, or upgrade firmware automatically.

## Work required before enabling writes

1. Resolve the failed erase/program result below. The physical model is user-confirmed;
   no electronic JEDEC ID has been obtained through the Operator.
2. Establish the command framing, firmware applicability, identification result,
   erase status, data pacing, completion status and cancellation behavior from
   documented protocol evidence or controlled reference traces.
3. Implement and qualify the USB adapter against the approved transaction below.
   Do not silently relax the shared transaction for other readers.
4. Qualify transport framing, replies, timeout and cancellation behavior with
   reference traces. Transaction fault injection alone does not prove USB behavior.
5. Qualify on the explicitly authorized cartridge, retaining two full backups,
   pinned source, all available erase evidence and two independent full-capacity
   final readbacks. Report which checks were actually possible.
6. Enable GUI/TUI/CLI capabilities only for the qualified firmware/profile pair,
   including recovery guidance and a clear explanation for unsupported boards.

## Approved Operator transaction

On 2026-09-27 the user approved firmware-managed combined erase/program, without
an intermediate host blank check or immediate per-bank readbacks. This exception
does not enable YOLO mode or permit an unknown physical board.

`cartridge-core::operator_programming::write_with` implements this policy:

1. Validate the reviewed SHA-256 and compatible source; durably retain the source.
2. Record the physical board evidence and require a qualified firmware protocol.
   For the controlled attempt, the user explicitly confirmed Ferrante 512. Reports
   distinguish this from electronic identification and leave JEDEC IDs null; they
   must never substitute the ROM header or invent observed flash-chip IDs.
3. Save two fresh, complete 512 KiB backups and require equality.
4. Recheck the device and cartridge record and cancellation before a durably recorded
   combined erase/program stage.
5. Program a full-capacity target with unused bytes padded to `FF`.
6. Independently read all 512 KiB twice, comparing both reads to the padded target.
7. Retain the readbacks and report which checks were performed and unavailable.

The hardware-injected tests cover source changes, unknown/wrong flash, identity
changes, short reads, mismatched backups, both final-pass failures (including
unused flash), cancellation, adapter timeout/disconnect/unexpected-reply errors,
and cleanup failure. They send no USB commands. These tests validate transaction
ordering and failure retention, not a real firmware implementation.

The shared GUI/TUI model and CLI now explain that programming qualification is
pending instead of suggesting that selecting Ferrante will enable Operator writes.
There is no public capability change and no claim of successful hardware writing.

## Protocol research boundary

Static interoperability research on Playback 1.10.0 shows that USB product
`123d` selects the streaming implementation only for firmware newer than 9.5.0;
9.5.0 selects the legacy implementation. This distinguishes the connected reader
from the newer Detect Flashcart command path. No newer command has been sent as
a speculative detection probe.

Legacy framing uses command 1, save-chip classification, ROM byte length and
reported save byte length, followed by the command CRC. The host exchanges a
zero-filled handshake before each 256 data frames and exchanges 64-byte data
frames individually. These are interoperability findings, not a successful
qualification. The provisional adapter accepts only zero-filled acknowledgements
and the known erase-busy marker; an acknowledgement is never proof of erasure.

A controlled reference trace with an independently verified erase remains needed.
Do not use a destructive command to discover board identity or enable this
firmware/profile pair based on the synthetic transport tests.

## Failed physical attempt and recovery

On 2026-09-27 (local time), the explicitly authorized write retained a pinned
v0.7.2 source and two fresh matching full-capacity backups before command 1. The
Operator acknowledged erase readiness. Progress was unexpectedly slow; after a
requested USB reconnect, the transport returned an endpoint-stall error at byte
offset 23,488. This does not establish the cause of the stall or prove that the
firmware alone is responsible.

Initial recovery reads disagreed. After reseating with USB disconnected, two
512 KiB reads agreed. They differed from the original backup in 6,053 bytes,
mostly consistent with programming bits without a complete erase. They did not
match the requested game. Neither the interrupted image nor the operation is
reported as successful. The original backups remain intact.

The user has been asked to move the cartridge to GBxCart RW for recovery using
the already-qualified transaction. The local investigation record tracks the
recovery outcome; do not claim recovery before two final readbacks pass.

Hardware failures are journaled before USB cleanup so a cleanup stall cannot hide
the error and recovery paths. The USB library's cancellation path can wait beyond
its transfer timeout; this is a diagnostic concern, not a proven cause of this
attempt's slow progress. No debugger was attached to the running write.

## References

- [Requested game release](https://github.com/borjaburgos/solarstriker-dx-captain-felipe/releases/tag/v0.7.2)
- [Epilogue flashcart support](https://www.epilogue.co/support/hardware/flashcart-support)
- [Public Operator protocol notes](https://github.com/jaames/gb-operator-reverse-engineering)
- [GBOpyrator read-protocol reference](https://github.com/N0ciple/gbopyrator)

The references are protocol research, not bundled runtime dependencies. No
Playback source code or binaries are included in Cartridge Studio.
