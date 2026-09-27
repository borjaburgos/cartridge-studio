# Develop Cartridge Studio

The shipped application is Rust. Python scripts prepare pinned build assets,
package releases and run optional independent test tools; no Python runtime is
included in installed packages. Read [AGENTS.md](../AGENTS.md) for workspace and
hardware rules, and [Architecture](architecture.md) for the crate boundaries.

## Layout

| Path | Responsibility |
| --- | --- |
| `rust/cartridge-core/` | ROM formats, mapper/board logic, reader protocols, transactions, checksums, catalog and artwork |
| `rust/cartridge-worker/` | One request in an isolated process with bounded JSON events and cooperative cancellation |
| `rust/cartridge-app/` | Shared GUI/TUI state, capabilities, write review, preferences, history and worker client |
| `rust/cartridge-desktop/` | Iced desktop presentation |
| `rust/cartridge-tui/` | Ratatui terminal presentation |
| `rust/cartridge-cli/` | Clap command line and structured output |
| `embedded/gb-helper/` | Existing C/Thumb helper executed in INLretro MCU RAM; not host code or a firmware update |
| `desktop/` | Application assets, integration files and USB rules |
| `scripts/` | Build, packaging and independent developer checks |
| `docs/` | Current guides, architecture, roadmap and historical qualification records |
| `tmp/` | Ignored build outputs, downloads, caches, backups and test environments |

Do not commit cartridge dumps, saves, BIOS files, downloaded game art, toolchain
installations or device serial numbers. The historical checkout folder name is
not an application branding requirement.

## Build on Linux

Use Rust **1.88 or newer**, Cargo, Python **3.11 or newer**, a C compiler/linker,
and the Linux window-system development libraries used by Iced. CI's Ubuntu
recipe installs `libxml2`, `libxkbcommon-dev`, `libwayland-dev` and `libx11-dev`;
Arch package names differ. Python is required by source tooling, not users of
release packages.

The unchanged ARM helper is pinned to LLVM **22.1.8** so its checked binary and
relocations reproduce. The helper setup tool downloads the official Linux x86-64
archive, checks its hash and retains only required tools; the download is about
1.9 GB. Prepare it once, then build:

```sh
python3 scripts/setup_llvm.py
export PATH="$PWD/tmp/toolchains/llvm/bin:$PATH"
export CARGO_HOME="$PWD/tmp/toolchains/cargo"
python3 scripts/build_rust.py
```

Cargo output is configured under `tmp/rust-target/`. The build prepares the pinned
game catalog and helper, builds the native executables and collects dependency
notices. The first run needs network access for verified inputs and Rust crates;
subsequent builds can reuse cached assets. `--debug` produces a debug build.
Do not alter the helper integrity manifest merely to accept a different compiler.

```sh
tmp/rust-target/release/cartridge --help
tmp/rust-target/release/cartridge-studio --no-device
tmp/rust-target/release/cartridge-tui --no-device
```

macOS source paths exist but packaging and hardware are not qualified. Windows
porting is outstanding; Unix-specific serial, process and filesystem assumptions
must be resolved rather than treating a successful library build as product support.

## Offline checks

Prepare build inputs once, then use the same Cargo home:

```sh
export CARGO_HOME="$PWD/tmp/toolchains/cargo"
cargo fmt --all --check
cargo test --locked --offline --workspace
cargo clippy --locked --offline --workspace --all-targets -- -D warnings
python3 scripts/test_installer.py
python3 scripts/test_rust_notices.py
```

Tests use simulated transports and synthetic ROMs. They must never open a physical
reader. Target a relevant crate/test suite while iterating. GUI checks use Iced's
headless simulator; TUI checks use Ratatui's test backend. GUI images are written
to `tmp/rust-native-qa/`. Inspect changed layouts, including the 1000 × 700 desktop
and 120 × 32 terminal boundaries, pending reviews and cancellation during resizing.

The independent ARM test executes the helper under Unicorn, without USB:

```sh
python3 -m venv tmp/toolchains/inlretro-tests
tmp/toolchains/inlretro-tests/bin/pip install -r requirements-tests.txt
tmp/toolchains/inlretro-tests/bin/python scripts/test_inlretro_flash.py
```

Optional emulator tools and `requirements-playtest.txt` are developer-only.
They are not the planned application emulator launcher and are never packaged.

## Build and check a release

```sh
python3 scripts/build_desktop.py
CARTRIDGE_STUDIO_TEST_INSTALL_BUNDLE="$PWD/tmp/dist/cartridge-studio" python3 scripts/test_installer.py
python3 scripts/test_arch_packages.py
```

Artifacts, manifests and `SHA256SUMS` are under `tmp/dist/`. Public Arch assets
omit `.BUILDINFO`, which otherwise exposes the local builder's installed package
inventory; originals remain in the ignored packaging directory. Public packages
do not claim reproducible-build provenance. Archive owner metadata and embedded
source paths are normalized for privacy. Review the package
inventory: include intentional runtime docs and notices; exclude source-only
research, ROMs, saves, artwork caches, device logs and untracked experiments.
The portable archive and split Arch packages must have the same version and
support claims. Packaging is currently a Linux/Arch workflow.

For an explicitly requested local install:

```sh
python3 scripts/install_desktop.py --components gui,tui,cli
```

Use an isolated `CARTRIDGE_STUDIO_INSTALL_PREFIX` for installer experiments.
Preserve the user's existing library. A test fixture or synthetic ROM must never
be copied to physical hardware as an incidental part of testing.

## Changing hardware behavior

Keep voltage selection, access bounds, firmware identity and supported-profile
checks in the core. Read support comes before programming support. Add a mock
transport or golden transcript and failure cases before asking for physical tests.
Keep source pinning, exact board checks, two matching full pre-erase backups,
blank verification, per-bank readback and two full final readbacks mandatory.

Physical tests are separate, explicitly authorized work. Record reader revision,
firmware, board, operation, hashes, raw outcomes and recovery. A simulated test is
not a physical qualification, and matching ROM bytes do not establish flash wiring.
Read [Contributing](../CONTRIBUTING.md) before submitting changes.
