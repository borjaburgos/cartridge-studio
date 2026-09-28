# GB Operator programming investigation

Status: investigation only. ROM writing and wiping remain disabled. The cartridge
has not been erased or programmed by this investigation.

## Verified locally

On 2026-09-27, the connected GB Operator advertised USB device version 1.11.
Its cartridge record identifies firmware 9.5.0; USB device version and firmware
version are different fields.

The user described the inserted cartridge as a Ferrante. Its existing ROM is
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

1. Confirm the physical Ferrante model and flash chip independently of its header.
2. Establish the command framing, firmware applicability, identification result,
   erase status, data pacing, completion status and cancellation behavior from
   documented protocol evidence or controlled reference traces.
3. Determine whether the firmware permits the existing blank and bank checks.
   If it cannot, agree explicitly on an Operator-specific safety policy before
   adding a different transaction. Do not silently relax the shared transaction.
4. Implement the transport and transaction in Rust, with offline fault injection
   for rejection, unexpected responses, timeout, disconnect and cancellation.
5. Qualify on the explicitly authorized cartridge, retaining two full backups,
   pinned source, all available erase evidence and two independent full-capacity
   final readbacks. Report which checks were actually possible.
6. Enable GUI/TUI/CLI capabilities only for the qualified firmware/profile pair,
   including recovery guidance and a clear explanation for unsupported boards.

## References

- [Requested game release](https://github.com/borjaburgos/solarstriker-dx-captain-felipe/releases/tag/v0.7.2)
- [Epilogue flashcart support](https://www.epilogue.co/support/hardware/flashcart-support)
- [Public Operator protocol notes](https://github.com/jaames/gb-operator-reverse-engineering)
- [GBOpyrator read-protocol reference](https://github.com/N0ciple/gbopyrator)

The references are protocol research, not bundled runtime dependencies. No
Playback source code or binaries are included in Cartridge Studio.
