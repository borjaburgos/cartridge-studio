# Rust terminal interface

Run `cartridge-tui`. Choose TUI in the installer, or use `./install.sh --components tui`
for a terminal-only installation. GUI and CLI are optional; the terminal menu
entry launches the TUI directly and no graphical libraries are installed for it.
Arch users select `cartridge-studio-tui` with its matching `core` package.

With CLI installed, `cartridge tui` is an equivalent shortcut; with GUI installed,
`cartridge-studio --tui` also works. All open the same Rust
Ratatui interface, with no Python or Textual runtime. The package's worker is
located beside the native executable. Preferences, backups, metadata and safety
rules are shared with the desktop.

The terminal needs **120 columns × 32 rows**. At smaller sizes a notice replaces
the workspace and dialogs, preserving state. Operations continue; X can still
request a safe stop. The app renders on input or progress and restores terminal
state on exit. No special font or terminal image protocol is needed.

| Key | Action |
| --- | --- |
| 1 / 2 / 3 / 4 | NES; Famicom; Game Boy / Color; Game Boy Advance |
| 5 / 6 / 7 | History; supported boards; preferences |
| Ctrl+O | Browse or paste a ROM path |
| U / F5 | Choose reader; check USB |
| P / M | Detect cartridge; cycle physical board profile |
| R / B / V | Read; backup; verify cartridge |
| C | Check ROM compatibility offline |
| W / Shift+W | Review write; review wipe |
| H / L / E | Checksums; activity; error details |
| D / S / A | Double read; strict GB checks; artwork |
| G | Cycle GBA ROM size; shown in the GBA workspace and Preferences |
| T | Toggle light/dark appearance |
| X | Review stop |
| Q | Quit when idle |
| Tab / Shift+Tab / Enter | Move focus; activate a button |
| Escape | Cancel a dialog |
| ? / F1 | Full keyboard guide |

Inside the file browser, Tab switches between the path and list. Up/Down selects
an entry, Enter opens it, and Backspace in list mode moves to the parent. Ctrl+U
clears the path; bracketed paste is accepted as text, never as commands. In
history, Enter opens the selected report and O loads its output ROM. Detail
views scroll with arrows/PageUp/PageDown and export with E. Checksum views compare
on Enter and export with Ctrl+E. Preferences use F to change the library.

Write and wipe require the exact uppercase confirmation word. The source hash,
reader, platform, profile and backup library are pinned in the review. Read settings do
not weaken mandatory backups or write verification. Stop waits for the worker
rather than abandoning the programmer.

For development, build with `python3 scripts/build_rust.py` and run
`tmp/rust-target/release/cartridge-tui`. Offline tests are
`CARGO_HOME="$PWD/tmp/toolchains/cargo" cargo test --locked --offline -p cartridge-tui`. Linux packages include the TUI. Rust uses portable
macOS APIs, but a tested Mac binary and installer remain release work. The public
beta ships Linux native binaries, with no Python wheel distribution.

NES and Famicom are separate physical-slot choices. Loading a `.nes` file cannot
choose between them, so it preserves your selection. Switching slots clears
detected hardware and pending write/wipe confirmation. Reports retain the selected
slot separately from the ROM family. Connect only one cartridge at a time.

Reader selection is shown in Preferences; U cycles Automatic / INLretro / GBxCart RW / GB Operator,
and F5 connects. Unsupported GBxCart actions produce reader-specific guidance.
