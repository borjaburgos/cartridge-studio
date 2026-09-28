# Install Cartridge Studio

The published **0.1.0-beta.1** remains the Linux x86-64 release. The
**0.1.0-beta.2 release candidate** adds Apple Silicon macOS 13.0 or newer.
Both are testing builds with limited hardware coverage; check
[supported configurations](support.md) before using a cartridge. Windows and
Intel/Universal Mac packages are not available. Obtain candidate files and their
`SHA256SUMS` together from [GitHub Releases](https://github.com/borjaburgos/cartridge-studio/releases).

## macOS Apple Silicon candidate

Verify the downloaded DMG or PKG against `SHA256SUMS` with `shasum -a 256`.
For the graphical app, open the DMG and drag **Cartridge Studio.app** to
Applications. For component selection, open the PKG, choose **Customize**, and
select GUI, TUI, CLI or any combination. The terminal choices install predictable
commands at `/usr/local/bin/cartridge-tui` and `/usr/local/bin/cartridge`; the
shared worker is kept under `/usr/local/libexec/cartridge-studio`.

Open the app from Finder or Launchpad. Installed terminal components run with:

```sh
cartridge-tui --no-device
cartridge --help
cartridge doctor
```

Re-run the same-version or newer PKG to add or remove managed components. The
installer removes only components marked as managed by Cartridge Studio and then
installs the current selection. It never removes `~/Library/Application Support/Cartridge Studio`,
the legacy `~/Library/Application Support/INL` library, or a custom library.

The local 0.1.0-beta.2 candidate is ad-hoc signed because Developer ID and
notarization credentials were unavailable. It is not notarized or stapled, and
Gatekeeper acceptance is not claimed. Do not disable Gatekeeper or weaken system
security. A distribution intended for general installation still requires
Developer ID Application/Installer signing, notarization and stapling.

macOS does not use Linux udev rules or serial-device groups. Close Playback,
FlashGBX and other cartridge applications, connect the reader directly with a
data-capable cable, and select the current `/dev/cu.usbserial…` port for GBxCart
when using the CLI. The installed app needs no Homebrew packages or development tools.

## Portable Linux archive

Download `cartridge-studio-VERSION-linux-x86_64.tar.gz` and `SHA256SUMS` from the
same release. Compare the archive's `sha256sum` output with its entry in
`SHA256SUMS` before extracting. Running `sha256sum -c SHA256SUMS` verifies all
assets but reports missing files for any assets you did not download.

Open a terminal in the extracted `cartridge-studio` directory and run:

```sh
./install.sh
```

Choose any combination of **GUI**, **TUI** and **CLI**. The cartridge engine is
always included; graphical libraries are selected only with the GUI. For an
unattended install, make the choice explicit:

```sh
./install.sh --components gui,tui,cli
./install.sh --components tui
./install.sh --components cli
./install.sh --all
```

The installer writes to your user account, adds commands and menu entries, and
preserves settings and cartridge backups. Rerun it to change your selection. It
keeps previous version folders for recovery; removing a component from the active
selection does not delete those old versions. Keep the extracted archive for
reconfiguration. A noninteractive session requires `--components` or `--all`.

The package bundles its desktop integration libraries, fallback font, USB
implementation and offline catalog. No Python, GTK, libusb installation, browser
or compiler is needed at runtime. The host provides Linux, compatible glibc,
device access and a Wayland/X11 session for the GUI. The archive's
`runtime-requirements.json` records the build's glibc requirement: **2.43 or newer**
for version 0.1.0-beta.1. Current Arch
builds are not claimed to run on older-glibc distributions. The TUI and CLI do
not require a graphical session.

## Arch packages

Install the release's `cartridge-studio-core` package together with the matching
`cartridge-studio-gui`, `cartridge-studio-tui` and/or `cartridge-studio-cli` package.
Pass those downloaded package files together to `sudo pacman -U`. Every frontend
requires the exact same core version. The `cartridge-studio` metapackage selects
all three interfaces. These are release assets, not an assertion that an official
Arch or AUR package exists.

## Launch and choose hardware

```sh
cartridge-studio
cartridge-tui
cartridge --help
cartridge doctor
```

`cartridge tui` and `cartridge-studio --tui` are shortcuts when their respective
components and the TUI are installed. A missing component produces an install
instruction. GUI/TUI accept a ROM path, `--library FOLDER` for a backup library,
and `--no-device` for offline browsing.

Choose **Reader** in the desktop sidebar, or **U** in the TUI. Automatic selection
works with exactly one candidate. When several are connected, select a reader
family; for several GBxCart devices, use the CLI's explicit `--port` option.
The app never guesses between connected candidates.

Disconnect USB before changing cartridges and connect only one cartridge per
reader. Choose **NES (72-pin)**, **Famicom (60-pin)**, **Game Boy / Color** or
**Game Boy Advance** to match the cartridge and slot. NES and Famicom are distinct
connectors; a `.nes` file cannot identify which is occupied. Leave the board on
Automatic for supported reading and use Detect before Backup. INLretro's buttons
are not required for normal operations.

## Linux device access

INLretro and GB Operator use exact-device udev rules. With CLI installed:

```sh
sudo cartridge usb-setup
```

Then reconnect the reader. If `sudo` cannot find a per-user command, invoke its
installed absolute path. Without CLI, install the archive's
`70-cartridge-studio.rules` in `/etc/udev/rules.d/`, reload rules with
`sudo udevadm control --reload-rules`, then reconnect USB. The Arch core package
installs the same rules.

GBxCart uses serial permissions. Grant your account access through the
distribution's serial-device group (`uucp` on Arch; often `dialout` elsewhere),
then sign out and back in. The udev rules above do not grant access to arbitrary
CH340 serial devices. Close FlashGBX or Playback before using the corresponding
reader in Cartridge Studio.

## Settings and upgrades

GUI and TUI share `studio/settings.json`, their library and reports. A custom
library can be selected with `--library` or `CARTRIDGE_STUDIO_DATA_DIR`.
`INL_DATA_DIR` is retained only as a legacy override. Changing a library location
does not move earlier backups automatically.

On macOS the default library is `~/Library/Application Support/Cartridge Studio`.
If that location does not yet exist and `~/Library/Application Support/INL` does,
the existing library remains in place and is reused so saved reports and backups
keep their original recovery paths.

For users of private development builds, the app was formerly called INL
Cartridge Studio; managed legacy launchers are replaced during installation.
The first public beta contains only native Rust applications. Earlier development
implementations and packages are kept in a local private recovery archive, not
in the public Git history or public releases.

For a source build, follow [Development](https://github.com/borjaburgos/cartridge-studio/blob/main/docs/development.md).
For errors, use [Support and troubleshooting](support.md).
