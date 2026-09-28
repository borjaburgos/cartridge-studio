# Cartridge Studio workspace

All cartridge-tool work is independent of SolarStriker. Keep the Rust application in
`rust/`, packaging/assets in `desktop/`, development tools in `scripts/`, unchanged
on-device helper in `embedded/gb-helper/`, and documentation in `docs/`.
Generated files, Cargo cache, cartridge backups, verification reports, local
players and test environments belong in ignored `tmp/`.

## Cartridge safety

Reading and offline checks are separate from physical programming. Erase or
program a cartridge only when the current task authorizes it. Never weaken
physical board checks, source pinning, two matching full backups before erase,
blank verification, per-bank readback or two final readbacks. Unknown physical
boards stay blocked. Routine tests must not open USB or run historical hardware
programming qualification scripts.

The user authorized an Operator-specific transaction on 2026-09-27: firmware
may combine erase and programming, without a host blank check between them or
immediate bank readbacks. Retain physical board qualification, pinned source,
two matching full-capacity backups, and two fresh full-capacity final readbacks.
Report unavailable checks explicitly; transport acknowledgements are not
verification. This exception does not change other readers or enable YOLO mode.

## Native Rust architecture

`cartridge-core` owns cartridge logic; `cartridge-worker` executes one isolated request.
`cartridge-app` owns the shared application model and process client. `cartridge-desktop`
(Iced) and `cartridge-tui` (Ratatui) are presentation adapters; widgets must not issue
USB commands. `cartridge-cli` is the native scriptable entry point. The Python backend,
GUI/TUI and wheels are retired; a private local recovery archive preserves development history.
There is no runtime fallback. Keep the existing worker contract and actionable errors consistent.

Build with `python3 scripts/build_rust.py`. Cargo output and downloads must stay
under `tmp/`; use `CARGO_HOME="$PWD/tmp/toolchains/cargo"`. Run ordinary checks:

- `cargo test --locked --offline` with that Cargo home.
- `cargo clippy --locked --offline --workspace --all-targets -- -D warnings`.
- `python3 scripts/test_build.py` for release preflight checks.
- `python3 scripts/test_rust_notices.py` for dependency-notice provenance checks.
- `python3 scripts/check_website.py` for website assets and release links.
- `tmp/toolchains/inlretro-tests/bin/python scripts/test_inlretro_flash.py` for the
  independent ARM helper CPU tests. The independent player is `tmp/toolchains/playtest/`.

Desktop interaction/rendering tests use Iced's headless simulator; no display or
cartridge is needed. TUI tests use Ratatui's real test backend. View generated
screenshots under `tmp/rust-native-qa/` when changing layouts. Preserve resize
notices below 1000 × 700 desktop pixels and 120 × 32 terminal cells, plus pending
state and cooperative cancellation during resizing.

Build portable/Arch distributions with `python3 scripts/build_desktop.py`.
Products stay in `tmp/dist/`. `python3 scripts/install_desktop.py` installs a
versioned native package and retains this checkout's existing backup library.
The installer asks for GUI/TUI/CLI selection; pass `--components gui,tui,cli` for
unattended installation. Test selection and reconfiguration with
`python3 scripts/test_installer.py`; set `CARTRIDGE_STUDIO_TEST_INSTALL_BUNDLE` to the absolute
`tmp/dist/cartridge-studio` path to run the same checks against real binaries.
Runtime packages must contain no Python, GTK, Textual, PyInstaller, original host
reader software, ROM images or downloaded game artwork. Preserve all required
license notices. Package only intentionally selected documentation, not unrelated
untracked files. macOS packaging/hardware qualification is separate release work.

## Project website and community

The GitHub Pages marketing site lives in `website/`; it is static HTML/CSS with
no dependency installation. Keep its supported hardware, planned platforms,
release version and minimum glibc aligned with `docs/support.md`, `Cargo.toml`
and the release's measured runtime requirements. Link downloads to published,
verified release assets. Use only deliberate public assets; app screenshots must
contain synthetic data and no personal paths, ROMs or downloaded game artwork.
Follow `docs/releasing.md` for publishing and `docs/roadmap.md` for tracked work.
