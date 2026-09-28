# macOS 0.1.0-beta.2 candidate validation

This record distinguishes packaged-product evidence from simulated protocol and
physical-hardware qualification. It was produced on 2026-09-27 on an Apple
Silicon Mac Studio running macOS 26.6.2 with Xcode 27.0. The release target is
`arm64-apple-darwin`, the declared deployment target is macOS 13.0 and the Apple
bundle version is `2`.

## Build and automated checks

- The complete locked Rust workspace passed formatting, strict Clippy and 100
  tests. Coverage includes the shared core and application model, isolated worker
  progress/error handling, cooperative cancellation and shutdown, GUI and TUI
  layouts, simulated reader protocols, two-read preservation, and write/wipe
  safety transactions. Automated tests did not open USB hardware.
- Release preflight, macOS LLVM asset selection, notice provenance, component
  installer reconfiguration and website download-pinning checks passed. The
  release notice gate resolved all 539 actual macOS release/build components.
- The official LLVM 22.1.8 Apple Silicon archive matched its pinned SHA-256. Its
  `clang` and `llvm-objcopy` produced the exact manifest-qualified embedded ARM
  helper with relocation and source-integrity checks still enforced.
- Four independent helper execution tests passed in pinned Unicorn 2.1.4 under
  Rosetta. The native arm64 Unicorn 2.1.4 development wheel exits on its second
  high-address `mem_map` call on this host; the x86_64 wheel avoids that emulator
  defect. Unicorn and Rosetta are development-test tools and are not shipped or
  required by Cartridge Studio.
- The pinned offline Libretro inputs produced 38,930 catalog records and a
  30,990,336-byte database. The synthetic Game Boy Color fixture was identified
  offline with valid header/global checksums and SHA-256
  `0d0ddd64c76f8ed834bb8f6f208ba4db8151fb338b621ecdd49745a1442658a0`.

## Packaged product

- The final DMG checksum and filesystem verified. The app and nested worker pass
  strict local code-signature verification with an ad-hoc signature.
- Every GUI, worker, TUI and CLI Mach-O is arm64, declares macOS 13.0, links only
  Apple system libraries/frameworks and contains no Homebrew or private build path.
- The PKG exposes GUI, TUI and CLI choices with an always-selected shared core.
  Expanded final payloads passed GUI-only, TUI-only, CLI-only and combined
  reconfiguration simulations; existing synthetic backups and settings survived
  reinstall and selection changes. A privileged installation into `/Applications`
  and `/usr/local` still requires administrator authorization on the test Mac.
- The CLI and TUI binaries extracted from the final component payloads reported
  the candidate version with a system-only `PATH`. CLI inspection and a TUI session
  loaded the synthetic ROM through the exact packaged shared worker.

## Native application exercise

The final app was launched outside the checkout from the read-only DMG. It was
then quit and launched by double-clicking `Cartridge Studio.app` in Finder with no
development `PATH`. The bundled worker started, performed reader enumeration and
shut down normally. With no supported reader connected it displayed actionable
macOS guidance and no Linux udev or group-membership instructions.

The live pass exercised the Cartridge Studio/File/Edit/Window/Help menus, About
metadata, Command+, Settings, filtered native Open and Save panels, standard edit
commands, close/hide and reopen, quit, resizing through the 1000 × 700 boundary,
the retained synthetic ROM view and the small-window notice. Retina bundle metadata
and high-resolution rendering were present. Native menus and dialogs had normal
macOS accessibility elements. Keyboard focus routing is implemented and covered by
the UI regression suite, but Iced 0.14 did not expose workspace widget semantics to
the macOS accessibility tree; VoiceOver workspace use remains unqualified.

Synthetic screenshots, containing no commercial artwork or user-home paths, are:

- `tmp/screenshots/Cartridge-Studio-0.1.0-beta.2-main.jpeg`
- `tmp/screenshots/Cartridge-Studio-0.1.0-beta.2-about.jpeg`
- `tmp/screenshots/Cartridge-Studio-0.1.0-beta.2-settings.jpeg`
- `tmp/screenshots/Cartridge-Studio-0.1.0-beta.2-small-window.jpeg`
- `tmp/screenshots/Cartridge-Studio-0.1.0-beta.2-finder-worker.jpeg`

## Explicitly unqualified

No reader/cartridge pair completed a physical macOS read, so there are no physical
hash comparisons to report. No cartridge was erased or programmed; no save was
restored and no reader firmware was changed. Intel and Universal builds are
separate targets and were not built. Until suitable credentials are present, the
local candidate is not Developer ID signed, notarized or stapled and Gatekeeper
acceptance is not claimed.
