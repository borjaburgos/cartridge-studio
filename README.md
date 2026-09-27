# Cartridge Studio

**Your cartridges. Your tools. One studio.**

Read, preserve and verify retro game cartridges across multiple readers. Write
homebrew to explicitly supported flash boards. Choose a graphical desktop, an
interactive terminal or a command line, all powered by the same native Rust core.

[Website](https://borjaburgos.github.io/cartridge-studio/) ·
[Download the first beta](https://github.com/borjaburgos/cartridge-studio/releases/tag/v0.1.0-beta.1) ·
[Supported hardware](docs/support.md) · [Roadmap](docs/roadmap.md) ·
[Contribute](https://github.com/borjaburgos/cartridge-studio/blob/main/CONTRIBUTING.md)

## Why this exists

Cartridge Studio began with a pile of cartridge readers and a familiar
frustration: each needed its own application, setup and way of working. Switching
hardware meant switching tools, and it was hard to tell what was safe to read or
write.

The aim is one dependable workspace for the cartridges and readers you already
own. Retro gamers can preserve their collections. Homebrew creators can check
and program supported boards. DIYers, developers and hardware hackers can inspect
results, automate work and help expand the supported hardware.

## Built around the cartridge

- **Three interfaces, one engine.** GUI, TUI and CLI share cartridge logic,
  compatibility checks, backups and reports. Install only the interfaces you want.
- **Evidence you can keep.** Two matching reads for backups and verification;
  CRC32, SHA-1 and SHA-256; original read files and actionable failure reports.
- **A recognizable collection.** Exact offline game matching for GB, GBC, GBA
  and NES, with optional box art, title screens and gameplay screenshots.
- **Deliberate programming.** Explicit board profiles, source checks, two
  backups before erasing, blank checks and full readback verification.
- **Native Rust.** Core, desktop, terminal and command line run without Python,
  a browser engine, the original reader applications or a runtime compiler.
- **Useful offline.** Reading, verification, game identification and local
  history work without an account or network. Artwork is optional.

## First public beta

**0.1.0-beta.1 is an early testing release.** Hardware coverage is deliberately
limited, and native Rust write/wipe still needs physical qualification. Keep
verified backups and check the support table before using a cartridge. Report
problems through the issue templates; macOS and Windows packages are not available.

**Linux x86-64 ships now**, qualified on Arch Linux / Omarchy. The portable
archive bundles its application libraries; split Arch packages are also available.
The current packages require **glibc 2.43 or newer**. The host supplies its kernel
and, for the GUI, a Wayland or X11 session. “Self-contained” means no separate application runtime to install;
the project still uses and credits third-party libraries.

macOS and Windows applications are on the [roadmap](docs/roadmap.md). macOS
source paths exist, but there is no qualified Mac release yet. Windows needs
platform work. Neither platform currently has a supported downloadable package.
The GUI uses native operating-system windows and Rust-rendered Iced controls.

| Reader | Read, back up and verify | Write and wipe |
| --- | --- | --- |
| INLretro | GB / Color; standard linear GBA ROMs; supported NES and Famicom boards | SST39SF040 AUDIO/MBC5 and UNROM-512 profiles |
| GBxCart RW | GB / Color and GBA on supported v1.4-family firmware; GBA only on supported v1.3 firmware | Ferrante 512 on PCB 6 / L14 only |
| Epilogue GB Operator | GB / Color and standard linear GBA ROMs | Not implemented |

Supported GB reading covers ROM-only, MBC1, MBC2, MBC3 and MBC5. NES/Famicom
support covers UNROM-512 / mapper 30 and manual NROM-128 / NROM-256 profiles.
NES and Famicom have separate physical slot selections. GBA is read-only, up to
32 MiB. This is **not blanket support for every game or cartridge** on a platform.

Native write/wipe workflows are implemented and tested with simulated devices;
physical Rust erase/program qualification remains pending. INLretro GBA and
NROM hardware qualification are also pending. See the [complete support matrix](docs/support.md)
and [recorded reader qualification](docs/readers.md) before choosing hardware.
Save-memory backup/restore, RTC, Camera photos and SD-card cartridges are not
supported yet. A matching game hash identifies bytes, not an original cartridge
or a writable board.

## Get started

Download the Linux archive and `SHA256SUMS` from the
[0.1.0-beta.1 release](https://github.com/borjaburgos/cartridge-studio/releases/tag/v0.1.0-beta.1).
Check the archive checksum, extract it, then run its installer:

```sh
./install.sh                         # choose GUI, TUI and/or CLI
./install.sh --components tui,cli    # terminal and scripting only
```

The per-user installer preserves your library and settings. See the
[installation guide](docs/install.md) for Arch packages, requirements and USB access.

```sh
cartridge-studio                     # graphical desktop
cartridge-tui                        # interactive terminal
cartridge --help                     # command line
cartridge doctor                    # discover a reader and check access
cartridge --reader operator gba backup
cartridge --json inspect /path/to/game.gb
```

With USB unplugged, insert one cartridge, then reconnect. Choose the reader and
the actual slot, leave the board on **Automatic**, and choose **Detect**, then
**Backup**. Retail mask-ROM cartridges cannot be erased or rewritten. INLretro's
physical buttons are not needed for normal use; leave its bootloader button alone.

## For builders and contributors

Community help is welcome: hardware test reports, new reader and board support,
accessibility, documentation, packaging and thoughtful bug reports all move the
project forward. See [CONTRIBUTING.md](https://github.com/borjaburgos/cartridge-studio/blob/main/CONTRIBUTING.md)
and the [development guide](https://github.com/borjaburgos/cartridge-studio/blob/main/docs/development.md) to begin.

The [roadmap](docs/roadmap.md) tracks emulator launching, confidence-based
original/bootleg assessment, macOS and Windows GUIs, broader INLretro slot
coverage, save preservation and Game Boy Camera photo export. The existing
`cartridge --json` interface serves scripts; a separate network API will wait for
a concrete need and a versioned contract.

Cartridge Studio is open source under the [MIT license](https://github.com/borjaburgos/cartridge-studio/blob/main/LICENSE).
Community contributions are welcome. Bundled libraries, catalog data, fonts and
platform marks retain their own terms;
see [third-party notices](docs/third-party.md). Cartridge Studio is independent
of the console and reader manufacturers. No games, BIOS files or downloaded game
artwork are distributed in its packages.

## Documentation

- [Install and update](docs/install.md), [GUI guide](docs/desktop.md), [TUI guide](docs/tui.md)
- [Hardware support and troubleshooting](docs/support.md), [reader details](docs/readers.md), [detection](docs/detection.md)
- [Game recognition and artwork](docs/games.md), [architecture](https://github.com/borjaburgos/cartridge-studio/blob/main/docs/architecture.md), [development](https://github.com/borjaburgos/cartridge-studio/blob/main/docs/development.md)
- [Roadmap](docs/roadmap.md), [release notes](docs/release-notes.md), [security reporting](https://github.com/borjaburgos/cartridge-studio/blob/main/SECURITY.md)

The checkout retains its historical `INL/` folder name so existing backup and
recovery paths remain valid. The application and commands are Cartridge Studio.
