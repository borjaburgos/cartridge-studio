# Application architecture

Cartridge Studio uses a shared Rust application model for its desktop and terminal
interfaces. Runtime cartridge code, desktop, terminal and command line are Rust.
Python is used only by development scripts, not by the installed application.

```text
GUI (Iced) ──┐
             ├─ cartridge-app ── cartridge-worker ── cartridge-core
TUI (Ratatui)┘                                         │
CLI (Clap) ────────────────────────────────────────────┘
                                                      ├─ nusb ── INLretro
                                                      ├─ nusb ── GB Operator
                                                      └─ serialport ── GBxCart RW
```

## Core and worker

`rust/cartridge-core` owns ROM formats, mapper logic, physical board detection,
compatibility, USB protocol, full read/backup/write/wipe transactions, checksums,
game identification, artwork validation and recovery journals. GUI/TUI widgets
never issue USB commands. nusb uses Linux usbfs or macOS IOKit directly; no libusb
or original host reader program is executed.

`rust/cartridge-worker` executes one typed request in a separate process. Requests and
bounded JSON events travel through pipes. SIGTERM requests cooperative
cancellation. The worker saves recovery information and releases the reader
before exiting. Native writes preserve source pinning, exact board checks, two
complete matching backups before erase, full blank verification, bank readback
and two complete final readbacks.

The reader's existing firmware is unchanged. A small existing C/Thumb helper
under `embedded/gb-helper/` runs in the programmer's MCU RAM for the supported
Game Boy flash board. It is compiled, relocation-checked and embedded at build
time. It is neither a host runtime nor a firmware replacement.

## Shared application model

`rust/cartridge-app` is the presentation and operation boundary shared by both
interfaces. It owns platform/profile state, detected hardware, loaded ROM
identity, immutable write review, capabilities, preferences, history, bounded
activity, actionable errors and portable file browsing.

Only a matching uppercase confirmation word can approve the retained review.
Changing reader/platform/profile invalidates detection and review. Loading another file
clears the old source. A successful result line is provisional until the worker
exits successfully; malformed output, unexpected exit and stderr are handled by
one transport implementation. Output/event buffers are bounded. Stop and Drop
request cooperative cleanup; no new job is permitted while one is active.

Preferences remain compatible with the old `studio/settings.json`, preserving
unknown fields. Malformed preferences use safe defaults and are not silently
overwritten. Both interfaces use the same library and reports. ROM exports
recheck the loaded SHA-256 and create new files without overwriting existing data.

## Interfaces

`rust/cartridge-desktop` uses Iced 0.14 with winit and tiny-skia software rendering.
Windows, input and clipboard integrate with the operating system; controls are
Rust-rendered rather than GTK widgets. PNG decoding, SVG logos and a fallback
font are compiled in. There is no browser, Python runtime or GPU requirement.
The interface subscribes to worker updates only while busy; idle operation has
no application polling timer.

`rust/cartridge-tui` uses Ratatui 0.30 and Crossterm 0.29. Keyboard commands, mouse
buttons, bracketed paste and file browsing use the same model and worker. It
redraws on input or model changes instead of running an idle animation loop.
Terminal state is restored on normal exit and Rust panic. The screen shows a
resize notice below 120 columns by 32 rows, preserving the current operation.
The desktop does the same below 1000 by 700 logical pixels.

The command line remains `rust/cartridge-cli`. `cartridge tui` executes its sibling native
terminal, so it does not depend on an unrelated executable on PATH. Frontends
are independently selectable in the installer and locate their shared worker
relative to their own executable.
The packaged CLI and worker names are hard links to one executable, dispatching
by entry name; the worker still runs in its own process. This shares the embedded
catalog on disk and avoids shipping two copies. A standalone worker binary is
also available in development builds.

## Packaging and dependencies

`Cargo.lock` pins Rust libraries. Core data/helper assets are prepared by
`scripts/build_rust.py`; `scripts/build_desktop.py` produces a native portable
archive and Arch package without PyInstaller. Linux desktop integration libraries
and their dependency closure are bundled under `lib/`; host glibc, kernel and
display session remain operating-system requirements. Library versions and
upstream notices are retained in `licenses/`. No ROM or downloaded game art is
shipped. Documentation packaging uses an explicit list rather than untracked
workspace files.

The per-user installer creates a version folder and redirects launchers without
changing previous app folders or backups. A custom build/test installation can
use `CARTRIDGE_STUDIO_INSTALL_PREFIX`; the normal prefix is `~/.local`. Existing user library
selection is preserved by `scripts/install_desktop.py` through `CARTRIDGE_STUDIO_DATA_DIR`.
Mac-native source paths exist, but macOS release/signing and hardware qualification
are not claimed by the Linux package. Windows requires additional transport,
process and filesystem work; no Windows release is currently supported.

## Verification

The core has golden protocol replay, malformed ROM cases, cancellation and
simulated complete write/wipe transactions with failure injection. The unchanged
helper also executes independently in Unicorn ARM tests. Shared-model tests cover
source pinning, profile invalidation, confirmation, settings preservation and
exports; process tests cover stderr draining, cancellation and malformed output.

The desktop uses Iced's headless simulator to click actual controls, assert
confirmation/resize behavior and render all pages/dialogs at supported sizes.
These rendered snapshots avoid stale-frame captures from a compositor that is
not presenting an obscured window. Terminal tests render into Ratatui's real test
backend and exercise input at the minimum size. Physical qualification is always
separate, explicit and recorded; routine tests never open USB.

## Physical slots and ROM families

The app has separate NES (72-pin), Famicom (60-pin), Game Boy / Color and GBA
choices. For compatibility with existing requests, the `platform` request field
identifies this selection: `nes`, `famicom`, `gameboy` or `gba`. The service maps NES and
Famicom onto the same NES bus commands and mapper implementation; firmware has
one `NES_INIT` operation for this bus, not a separate Famicom selector.

A `.nes` file identifies a ROM family and mapper, not a physical connector. ROM
inspection and game catalog keys retain the existing `famicom` family for both.
Loading a source never changes the physical-slot selection. Changing slots clears
hardware detection and any pending destructive review, even within the same family.
Reports retain `slot` with `slot_source: user_selection`, including failed
transactions; this is not a claim that the reader senses which connector is in use.
Only one cartridge may be connected. Earlier reports without `slot` do not imply
that one particular physical connector was used.

## Reader boundary

`readers::Kind` selects Automatic, INLretro, GBxCart RW or GB Operator;
`readers::Device` owns the
selected transport and enforces capabilities before cartridge access. The
service request includes `reader` and an optional CLI serial `port`. Existing
requests default to Automatic. Firmware identity is distinct from cartridge
metadata; reports retain both. Shared `gb::RomReader` operations reuse mapper
selection, validation, durable output, checksums and catalog lookup. GBxCart
uses an independent serial driver. `gb::FlashWriter` extends the ROM-reading
interface with board-specific preparation, erase and bank programming; qualified
writable readers share the same durable transaction and mandatory checks. The INLretro
RAM helper stays inside its driver, while GBxCart uses native firmware flash
commands for SST39SF040 AUDIO/MBC5 only. Neither frontend sends flash commands.

The serialport crate is built without its libudev feature; serial I/O and
discovery use OS facilities. The GB Operator driver uses nusb bulk endpoints with
CRC-protected fixed frames, restores the Linux CDC driver after each operation,
and exposes GB/C ROM reads through the shared Game Boy mapper pipeline, plus
GBA through the linear ROM interface. It deliberately
contains no save or programming commands. These drivers add no Python, libusb or
separate runtime requirement. See [reader support](readers.md) for the exact
supported envelope.

## GBA reader boundary

`cartridge-core::gba` owns header validation, ROM sizing and durable read transactions.
Its linear `RomReader` trait is separate from the banked Game Boy trait. The INLretro
and GBxCart implementations select the 3.3 V GBA bus and convert byte addresses into
firmware word addresses. GB Operator performs a 256-byte header probe, then prepares
one complete CRC-framed bulk stream per pass. GUI/TUI never send hardware commands.
INLretro uses its shared side-entry GB/GBA connector and native buffered page reads. Service capability
checks reject GBA write/wipe before opening hardware. `gba_rom_bytes`
is an optional request override, shared by GUI/TUI preferences and the CLI.
