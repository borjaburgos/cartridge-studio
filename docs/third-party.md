# Third-party components

Cartridge Studio contains Rust application binaries, bundled Linux desktop
integration libraries and macOS system-framework integrations. Original project code is under the [MIT license](https://github.com/borjaburgos/cartridge-studio/blob/main/LICENSE);
that license does not replace the separate terms for the components below.
The public beta contains no Python, PyGObject, GTK, Libadwaita, Textual, Rich
or PyInstaller runtime. Earlier development interfaces are retained only in a
local private recovery archive, not the public Git history. No original
host-side programmer application is executed.

Rust dependency versions are pinned in `Cargo.lock`. Build output includes
`licenses/rust/rust-components.json` and upstream license files. The manifest
explicitly includes workspace development and target-specific dependencies as
well as production dependencies, recording each component's source and notice
status. Each release gate requires complete notices for its target production/build
dependency graph. The macOS arm64 graph passes that gate; reviewed Apple crates
whose archives contain only an SPDX declaration retain that declaration plus the
canonical MIT text from a pinned SPDX license-list-data revision. Some
other-target/development entries remain declaration-only and are not part of the
macOS or Linux release graph. Windows must pass its own gate before shipping. Major
components include:

- Iced / winit / tiny-skia / cosmic-text: Rust desktop windowing, rendering and text.
- Ratatui / Crossterm: terminal layout and input.
- nusb: native Linux usbfs / macOS IOKit USB transport.
- serialport 4.10.1: native serial discovery and I/O, without libudev. MPL-2.0;
  unmodified source is available at https://crates.io/crates/serialport/4.10.1 .
  Its license and component record are included in the package.
- serde, serde_json, RustCrypto hashes, crc32fast, rusqlite with bundled SQLite,
  ureq/rustls, clap, ctrlc, chrono, fs2 and URL parsing.

These are upstream libraries under their respective licenses, predominantly
MIT/Apache-2.0/BSD. The exact component manifests and notices govern each version.
Fira Sans is embedded by Iced for a fallback font; its upstream SIL Open Font
License is retained with the font assets' notices.

Linux desktop integration libraries are separately replaceable under `lib/`.
`licenses/system/packages.json` records exact Arch package versions and library
names. Upstream notices and common licenses accompany them. They are unmodified
builds from Arch packaging sources at
https://gitlab.archlinux.org/archlinux/packaging/packages . `libgcc_s` is distributed
under GPL-3.0 with the GCC Runtime Library Exception. Preserve notices and source
obligations when redistributing. A public release must include the applicable
license inventory and source materials or offers required by each component.

The offline game catalog derives from Libretro database GB/GBC/GBA/NES records
(CC BY-SA 4.0), converted into an embedded SQLite index. Its pinned revision,
attribution and complete license are included under `licenses/`. Optional game
artwork comes from Libretro thumbnails, remains with its respective rights
holders, and is cached only on demand; it is not included in application packages
or relicensed under the catalog license. See [game recognition](games.md).

Platform logos are Nintendo marks from Wikimedia Commons. Source pages, SVG
credits, revisions and hashes are recorded in `desktop/assets/CREDITS.md`, shipped
as `licenses/PLATFORM-LOGOS.md`. The Cartridge Studio icon is original artwork. The application
is independent and is not affiliated with Nintendo.

The programmer firmware is unchanged. The small C/Thumb helper under
`embedded/gb-helper/` runs on the programmer, not on the host. Protocol references
remain in the technical documentation. Reader reference software, ROM dumps,
emulator installations and hardware-qualification screenshots belong only in
ignored `tmp/`. The marketing site separately includes a deliberately published
app screenshot with synthetic homebrew metadata; its asset credits are recorded
alongside the platform marks.
