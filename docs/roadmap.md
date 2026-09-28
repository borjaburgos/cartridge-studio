# Cartridge Studio roadmap

The goal is one dependable cartridge workspace across readers and operating
systems, with equally capable graphical, terminal and scripting interfaces.
The first public release is **0.1.0-beta.1**; the support table distinguishes
implemented paths from physically qualified hardware.
This roadmap records direction and acceptance criteria, not release-date promises.
Use the [issue tracker](https://github.com/borjaburgos/cartridge-studio/issues)
for implementation discussion and
[feature requests](https://github.com/borjaburgos/cartridge-studio/issues/new/choose)
for proposals. Hardware availability and reproducible evidence determine sequencing.

## Foundation already available

- Native Rust core, isolated GUI/TUI worker, graphical desktop, TUI and CLI.
- Three reader families: INLretro, GBxCart RW and Epilogue GB Operator.
- Explicit NES/Famicom slot selection, GB/GBC reading and standard GBA ROM reading.
- Two-pass preservation, checksums, compatibility checks and recovery reports.
- Exact offline game catalog with optional cached artwork.
- Linux portable and Arch packages with selectable GUI/TUI/CLI components.

[Support](support.md) distinguishes implemented paths from physically qualified
reader/firmware/cartridge combinations. In particular, native Rust write/wipe,
INLretro GBA and NROM need further hardware qualification.

## Planned work

| ID | Feature | Status | Done when |
| --- | --- | --- | --- |
| [CS-01](https://github.com/borjaburgos/cartridge-studio/issues/1) | Hardware qualification and reliable reads | Next | INLretro GBA, manual NROM and supported Rust flash transactions have recorded hardware results, failure/recovery evidence and accurate support labels. Fixtures cover dirty contacts, partial replies and two-pass mismatches. |
| [CS-02](https://github.com/borjaburgos/cartridge-studio/issues/2) | Play from cartridge through your emulator | Next | Users select an executable and arguments per platform, read and verify the inserted cartridge, then launch that retained ROM. GUI/TUI/CLI show setup and launch errors. Partial reads never launch. |
| [CS-03](https://github.com/borjaburgos/cartridge-studio/issues/3) | Saved-game backup and restore | Next | Save technology and size are identified separately from ROM; two matching backups and recoverable, verified restores work on explicitly qualified boards/readers. Existing saves are backed up before replacement. |
| [CS-04](https://github.com/borjaburgos/cartridge-studio/issues/4) | Original / reproduction assessment | Planned research | Reports show observed board/chip evidence, catalog identity and explicit confidence/unknown states. A ROM hash alone never labels a cartridge original or counterfeit. No destructive probing is used. |
| [CS-05](https://github.com/borjaburgos/cartridge-studio/issues/5) | Native macOS GUI, TUI and CLI | Planned | Apple Silicon and Intel targets have native windows, platform conventions, reader access, signed/notarized installers, component selection, update behavior and physical hardware qualification. |
| [CS-06](https://github.com/borjaburgos/cartridge-studio/issues/6) | Native Windows GUI, TUI and CLI | Planned | Windows transport/process/filesystem work is complete, reader drivers have clear setup, packages are signed, and native windows, keyboard/accessibility behavior and hardware are qualified. |
| [CS-07](https://github.com/borjaburgos/cartridge-studio/issues/7) | Self-contained releases | Ongoing | Each supported OS has reproducible packages, verified checksums, dependency/license inventories and no separately installed application runtime. Host OS requirements are clearly stated and tested. |
| [CS-08](https://github.com/borjaburgos/cartridge-studio/issues/8) | Every INLretro physical slot | Planned | Expand the current NES, Famicom and shared GB/GBA connector support to SNES / Super Famicom, Nintendo 64 and Mega Drive / Genesis. Add read/verify first, with electrical/mapper checks and physical qualification per slot. Writing only follows exact flash-board support. |
| [CS-09](https://github.com/borjaburgos/cartridge-studio/issues/9) | Game Boy Camera photo backup | Planned; depends on CS-03 | Read the Camera's save data without altering it, retain the raw backup, export album images with palettes/contact sheets, and distinguish recoverable deleted images from current album slots. |
| [CS-10](https://github.com/borjaburgos/cartridge-studio/issues/10) | More cartridge boards and readers | Ongoing | Community hardware reports become explicit, tested profiles with protocol references, electrical limits, transport simulations and qualified physical readbacks. Add unsupported NES mappers and GB variants incrementally. |
| [CS-11](https://github.com/borjaburgos/cartridge-studio/issues/11) | RTC preservation | Planned; depends on CS-03 | Preserve raw clock state, timestamp and format alongside save backups; restoration is explicit and verified. Host-time emulation cannot silently rewrite the cartridge clock. |
| [CS-12](https://github.com/borjaburgos/cartridge-studio/issues/12) | Collection and accessibility | Planned | Local search/filtering, readable progress/recovery, accessible navigation and predictable layouts work across frontends. Metadata sources have clear licenses and identity remains exact. |
| [CS-13](https://github.com/borjaburgos/cartridge-studio/issues/13) | Stable automation contract | Planned | Document and version CLI JSON results/errors, exit behavior and cancellation; add integration fixtures. Promote a library or optional local API only for a demonstrated integration. |
| [CS-14](https://github.com/borjaburgos/cartridge-studio/issues/14) | YOLO mode: fast, unverified transfers | Planned | Explicit GUI/TUI/CLI mode performs single-pass reads and direct writes/wipes without optional identification/compatibility checks, automatic backups, source hash pinning, checksums, blank checks or readbacks. Results are labeled unverified; per-reader timing and unavoidable firmware behavior are documented. |

## YOLO mode

Tracked in [CS-14](https://github.com/borjaburgos/cartridge-studio/issues/14).

An explicit fast mode for users who want to read, write or wipe immediately,
without Cartridge Studio's optional verification and compatibility checks.
Reads use one pass; writes skip automatic backups, repeated physical
identification, source hash pinning, blank verification, per-bank readbacks and
final readbacks. Bad ROM checksums do not block the operation. Users can supply
the supported board profile and transfer size when automatic selection would
require extra checks. Metadata and artwork lookup stay off the transfer path.

Verified mode remains the default. GUI and TUI expose the same explicit choice
as a proposed CLI `--yolo` flag, with **YOLO — unverified** shown during the
operation and in the retained result. Selecting the mode should not introduce
repeated confirmation dialogs. Transfer completion never implies verified bytes
or a playable game.

Required command framing, profile voltage/timing, acknowledgements and error
handling still apply. YOLO does not supply a missing programming algorithm or
suppress disconnects, partial transfers, file errors or firmware rejections.
Some readers enforce checks internally; document these limits rather than
claiming firmware verification was disabled. Shared Rust policy, protocol-trace
tests and physical timing measurements must demonstrate the skipped work across
all frontends. The current verification-only transaction requirements need an
explicit exception when this planned mode is implemented.

## Play means a verified local copy

Cartridge Studio already produces ROM files usable by external emulators;
its development playtests are not an end-user launcher. [CS-02](https://github.com/borjaburgos/cartridge-studio/issues/2) adds that missing
workflow: insert, read, verify, play. The emulator runs a retained local ROM,
not live reads from cartridge pins. Emulators are user-configured and separately
installed. Save synchronization is separate work with explicit backup and restore,
not an automatic consequence of launching a game.

Embedded emulator cores, rewind, achievements, shaders, cheats and controller
mapping can be considered later. The first launcher should work with the emulator
people already prefer, keep its executable/arguments reviewable, and avoid shell
command interpolation.

## What authenticity can establish

Tracked in [CS-04](https://github.com/borjaburgos/cartridge-studio/issues/4).

A known hash establishes that the dumped bytes match a catalog entry. A
reproduction may contain exactly those bytes, and an original cartridge can have
unstable contacts or corrupted memory. Board/chip identification, electrical
behavior and user-supplied PCB evidence can strengthen an assessment, but not every
reader exposes the necessary information. Unknown is a useful result. Reports
must explain evidence and uncertainty instead of promising a universal bootleg test.

## Do we need an API?

Tracked in [CS-13](https://github.com/borjaburgos/cartridge-studio/issues/13).

Not a network service yet. The CLI already exposes `--json` for local automation,
and the core is organized as Rust crates. The worker's JSON pipe protocol is an
internal frontend contract, not a promised stable public API.

The next useful step is to document and version the existing automation surface.
A supported Rust library API or opt-in local service should follow a real consumer
such as an emulator integration or collection manager. Any future service needs
exclusive device ownership, bounded inputs, versioning, cancellation, and the same
write review and recovery guarantees as the local application. None should require
a background server merely to read a cartridge.

## How to move an item forward

Open or join the linked issue, describe the user need, and include the reader,
firmware, cartridge/board and OS involved. A clear report, board photo you own,
protocol source, test fixture or loan of test hardware can be as useful as code.
Do not upload commercial ROMs or private saves. See
[Contributing](https://github.com/borjaburgos/cartridge-studio/blob/main/CONTRIBUTING.md).

The [Playback review](https://github.com/borjaburgos/cartridge-studio/blob/main/docs/epilogue-review.md)
records product research that informs this roadmap; it does not imply those
features are implemented or Epilogue code is included.
