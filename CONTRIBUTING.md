# Contributing to Cartridge Studio

Help make cartridge preservation easier to trust. Code is one contribution;
clear hardware reports, documentation, accessibility reviews, packaging and
repeatable tests are equally welcome.

The first public release is **0.1.0-beta.1**. Reproducible beta feedback and
explicitly scoped hardware qualification are especially useful.

Start with the [roadmap](docs/roadmap.md), [supported hardware](docs/support.md)
and [open issues](https://github.com/borjaburgos/cartridge-studio/issues).
For a substantial new reader, mapper or workflow, open a proposal before building
it so the electrical assumptions and user experience can be reviewed together.

## Report a bug or request hardware

Use the repository's issue forms. Include the application version, OS,
GUI/TUI/CLI, reader model/revision/firmware, slot, board profile and exact action.
Attach a redacted diagnostic report or a minimal synthetic reproduction when
possible. Explain expected behavior and the next action the error suggested.

Do not attach commercial ROMs, BIOS files, private saved games, access tokens or
personal device serial numbers. A ROM hash and size often provide enough context.
Board photos should be your own and exclude personal information. Never erase a
cartridge just to reproduce a bug unless a separate physical test is explicitly
agreed and its existing contents are backed up.

## Submit a change

1. Build and run the relevant offline checks in [Development](docs/development.md).
2. Keep reader protocols and safety decisions in `cartridge-core`; GUI/TUI share
   the application model and never send USB commands from widgets.
3. Add meaningful coverage for changed parsing, hardware transactions or failure
   recovery. Keep routine tests synthetic and disconnected from USB.
4. Update support claims and user instructions with the implementation. Label
   accepted protocol revisions separately from physically tested hardware.
5. Open a focused pull request explaining the problem, resulting behavior,
   validation and any remaining limits. Include UI captures for visual changes.

Before review, run formatting, workspace tests and strict Clippy as documented.
Generated files belong under ignored `tmp/`. Keep dependency notices and upstream
attribution intact. Do not add application runtime dependencies casually; explain
why a library is needed and how releases will remain self-contained.

## Hardware work has a higher bar

Unknown boards stay blocked for programming. New write support must identify the
physical flash interface, pin the source, retain two complete matching backups
before erase, verify blank memory, verify programmed banks and compare two final
readbacks. Fault injection should cover timeout, disconnect, cancellation,
changed source, bad identification and mismatching data. Convenience settings
must never weaken these guarantees.

Hardware reports should state precisely what was tested. An emulator smoke test
checks a dump's behavior; it does not qualify the electrical safety of a new
programming method. Do not claim a whole platform is supported from one cartridge.

## Community and licensing

Be specific, patient and respectful. Assume other contributors may be new to
Rust, electronics or retro cartridge terminology. Critique changes, not people,
and make actionable suggestions. Participation does not require sharing a
collection, purchasing hardware or making a financial contribution.

Original project code is [MIT-licensed](LICENSE). By contributing original work,
you agree to distribute it under that license. Retain separate upstream terms
for third-party code and data. Do not copy vendor applications or protocol implementations
without checking their license and preserving required attribution.

Report vulnerabilities through [SECURITY.md](SECURITY.md), not a public bug report
containing exploit details or private data.
