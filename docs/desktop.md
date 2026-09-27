# Native desktop

Launch `cartridge-studio` from the application menu or terminal. The desktop
is written in Rust with Iced and a software renderer. It uses native OS windows
and Rust-rendered controls, with real platform SVGs, cached game art, light/dark
appearance, clipboard integration and drag-and-drop ROM loading.

1. Connect one cartridge with USB unplugged, then reconnect the reader.
2. Select your **Reader** in the sidebar (Automatic, INLretro, GBxCart RW or GB Operator).
   Choose **NES**, **Famicom**, **Game Boy / Color** or **Game Boy Advance** to match the physical
   connector, then press **Check USB**. NES uses the 72-pin slot; Famicom uses
   its separate 60-pin slot. The selected connector appears below the heading.
3. Leave the physical board on Automatic and press **Detect**. Unknown hardware
   remains blocked for writes; Game Boy flash wiring requires the exact manual profile.
4. **Read** saves a ROM. **Backup** always reads twice. **Verify** compares the
   cartridge against the loaded ROM. **Checksums** compares expected file hashes.
5. **Write…** checks compatibility before showing a review. **Wipe…** also requires
   review. Type the exact uppercase confirmation word, or cancel. Mandatory
   backups and verification run in the cartridge service regardless of read options.

**Load ROM** and Ctrl+O open the built-in file browser. Paste a full path, navigate
folders, or drop a ROM on the window. **Save a copy** checks that the loaded ROM
has not changed and saves to a new filename. **Backup history** opens retained
reports and ROMs. **Activity** provides copy/export of diagnostic text.

Preferences select the library, ordinary-read checks, optional artwork downloads
and appearance. The file `studio/settings.json` is shared with the terminal;
existing settings and backup reports are retained during the Rust migration.
Malformed preferences produce an actionable message and are not overwritten.

The workspace requires **1000 × 700 logical pixels**. Smaller windows show a
resize notice instead of squeezed controls; ongoing operations continue, with a
Stop control available. Returning to a suitable size restores the workspace and
pending dialog. Within a supported window, the content scrolls and operation
controls remain at the bottom. Keep USB connected while stopping.

The portable installer and Arch package include the application libraries. No
Python, GTK, browser engine, libusb installation or runtime compiler is required.
The host still provides Linux, glibc and its graphical session. The current build
is qualified on Arch/Omarchy x86-64. macOS and Windows releases are planned.

Development builds use `python3 scripts/build_desktop.py`. Install the resulting
version with `python3 scripts/install_desktop.py`, then select GUI and any other
interfaces you want. Use `--components gui` for a GUI-only installation or
`--all` for all interfaces. The shared worker is installed automatically; neither
CLI nor TUI is required by the desktop. Rerun the installer to change the selection.
Arch users can install `cartridge-studio-gui` with its matching `core` package.
Run offline native UI checks
with `CARGO_HOME="$PWD/tmp/toolchains/cargo" cargo test --locked --offline -p cartridge-desktop`; these need no display or cartridge.
Generated screenshots are under `tmp/rust-native-qa/headless/`.

Loading a ROM leaves the physical slot selected by the user unchanged. NES and
Famicom share ROM formats and supported board profiles, but each has a separate
slot selection and logo. Switching slots invalidates board detection and any
pending write/wipe review. Reviews and saved operation reports identify the slot.

INLretro, supported GBxCart firmware and GB Operator provide GB / Color and GBA
ROM reads and verification with Automatic board selection. GBxCart v1.3 supports
GBA only. See [supported hardware](support.md) for qualification limits.
Unavailable operations stay disabled and the workspace explains the limit. Changing
readers clears connection/detection state; use Check USB to connect the new selection.
