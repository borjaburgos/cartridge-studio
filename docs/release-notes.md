# Release notes

## 0.1.0-beta.2 — macOS arm64 release candidate

This candidate adds the first native macOS distribution while keeping the Linux
0.1.0-beta.1 downloads pinned to their published assets. It targets Apple Silicon
(`arm64`) and macOS 13.0 or newer. Intel and Universal builds remain separate,
unqualified targets.

### macOS candidate

- A Retina-aware `Cartridge Studio.app` with a native application menu, About,
  Settings, standard Edit/Window commands, Command-key shortcuts, native ROM
  open/save panels, Finder/Dock reopen behavior and safe cancellation before quit.
- A DMG for the self-contained GUI and a standard Installer package with separate
  GUI, TUI and CLI choices. The shared worker is always installed; GUI-only and
  terminal-only selections do not depend on each other. Re-running the installer
  changes the managed selection without touching Application Support backups or settings.
- Native paths use `~/Library/Application Support/Cartridge Studio`, while an
  existing `~/Library/Application Support/INL` library remains in place and is reused.
- All bundled executables are Apple Silicon Mach-O files with a macOS 13.0
  deployment target and system-framework-only dynamic linkage. No Homebrew,
  Python, browser shell, compiler or development `PATH` is required after installation.
- The pinned LLVM 22.1.8 Apple Silicon toolchain reproduces the embedded helper
  byte-for-byte and retains relocation/integrity checks. Pinned catalog inputs and
  the actual macOS release dependency graph pass the enforced notice gate.
- macOS CI builds and tests the unsigned packages without credentials while
  preserving the Linux CI job.

The native menu, About panel, Settings shortcut, standard editing commands,
filtered Open and Save panels, close/reopen behavior, small-window notice and
Finder launch were exercised from the packaged DMG. Finder launch also started
the bundled worker without a development `PATH` and returned macOS-specific
reader guidance. The complete record and synthetic screenshot names are in
[macOS candidate validation](macos-validation.md).

Keyboard focus routing is implemented and regression-tested, and native panels
and menus expose their normal accessibility elements. Iced 0.14 does not expose
the Rust-rendered workspace widgets as a semantic macOS accessibility tree,
however, so VoiceOver operation of the main workspace remains unqualified.

This locally built candidate is **ad-hoc signed, not Developer ID signed, not
notarized and not stapled** because no suitable identities or notary credentials
were available on the qualification Mac. Gatekeeper acceptance is therefore not
claimed. Finder/UI and synthetic offline qualification are recorded separately;
no physical reader/cartridge combination is qualified for macOS by this candidate.
Write, wipe, restore, firmware update and save restoration were not performed.

## 0.1.0-beta.1 — First public beta

Cartridge Studio brings several cartridge readers into one native Rust
application, with graphical, terminal and command-line interfaces. This is the
first public beta: a foundation for community testing and contributions, with
explicit hardware limits and no stable-release promise yet.

### Available in this beta

- **Linux x86-64** portable archive and split Arch packages, with selectable GUI,
  TUI and CLI. Qualified on Arch Linux / Omarchy; these builds require glibc
  **2.43 or newer**. macOS and Windows distributions remain planned.
- **INLretro, GBxCart RW and Epilogue GB Operator** reader drivers, with explicit
  reader/firmware checks and separate NES/Famicom physical-slot selections.
- **GB/GBC ROM reading** for ROM-only, MBC1, MBC2, MBC3 and MBC5, and **standard
  linear GBA ROM reading** up to 32 MiB on supported readers. GBxCart v1.3 is
  limited to GBA. NES/Famicom profiles cover UNROM-512 and manual NROM-128/256.
- **Preservation and verification:** two matching reads for backups and cartridge
  verification, SHA-256/SHA-1/CRC32, retained raw passes and actionable reports.
- **Known-game identity:** exact offline hash/size matching, with optional cached
  box art, title screens and gameplay screenshots after a recognized read.
- **Explicit flash-board workflows:** compatibility review, pinned source,
  two full backups before erase, blank checks, bank verification and final
  readbacks for supported SST39SF040 AUDIO/MBC5 and UNROM-512 profiles.
- Shared library, preferences and operation history; minimum-window notices;
  cooperative cancellation; and CLI JSON output for local scripting.

### Beta limitations

Native Rust write/wipe transactions have simulated tests but **physical Rust
programming qualification remains pending**. INLretro GBA and NROM hardware
qualification are also pending. Implemented protocol support is not evidence
that every reader revision or cartridge has been tested. Consult the
[complete support table](support.md) and [qualification records](readers.md).

Save-memory backup/restore, RTC, Game Boy Camera photo export, authenticity
assessment, other INLretro console slots and an end-user emulator launcher are
not implemented. GBA writing, SD-card cartridge management and arbitrary NES
mappers are unsupported. A known game hash does not prove an original physical
cartridge, and retail mask ROM cannot be rewritten.

### Packaging and community

Core and frontends are native Rust. Application libraries are bundled; no Python,
browser runtime, original host reader software or runtime compiler is required.
The host still supplies its operating system, compatible glibc and graphical
session where needed. Packages contain no games, saves, BIOS files or downloaded
game artwork.

Original project code is MIT-licensed. Third-party components, catalog data,
fonts and platform marks retain their own terms and notices. Release preparation
checks binary versions, package checksums and notice coverage. Build paths and
archive ownership are normalized for privacy; these builds do not yet claim
reproducible-build provenance.

The [project website](https://borjaburgos.github.io/cartridge-studio/),
[installation guide](install.md), [roadmap](roadmap.md) and
[contribution guide](https://github.com/borjaburgos/cartridge-studio/blob/main/CONTRIBUTING.md)
provide the entry points for testing and helping the project grow. Report beta
issues with reader, firmware, board/profile, application version and redacted
operation details. Never attach commercial ROMs or private saves.

This public history begins with the first beta. Earlier development code and
release packages are preserved separately in a local private recovery archive;
hardware qualification evidence remains documented where relevant.
