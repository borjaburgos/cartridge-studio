# Game recognition and artwork

After a successful **Read** or **Back up**, Cartridge Studio identifies the saved
ROM against an included offline catalog. The native app loads the result and
shows a game identity panel and available images; the terminal shows text
metadata in its **Game identity** panel. **Load ROM** also identifies existing
files offline, and the desktop displays available cached images.

Known releases show their catalog title (including region/version qualifiers)
and available publisher, developer, year, genre, region and player information.
Missing fields are omitted. Catalog metadata can contain omissions or mistakes;
the identity match and its source are displayed separately.

Available **box art**, **gameplay screenshots**, and **title screens** can download
after a read. The desktop shows thumbnails. The TUI remains text-based and does
not display images or require a terminal graphics protocol; saved artwork files
can be opened separately in your preferred image viewer.

Use **Refresh artwork** in the desktop to retry missing downloads. Loading a file
alone does not start a network request. In desktop Preferences, turn off
**Download artwork after a known game is read**, or use **A** in the TUI to toggle
automatic artwork downloads.
Previously cached images remain available offline.

## Identity and limits

- Matching requires an exact SHA-1 **and byte count**, not the filename or title
  in the ROM header. Game Boy and Color share a lookup across both catalogs.
- NES/Famicom first checks the entire file. If no match exists, a valid ROM
  without a trainer is checked against headerless PRG + CHR catalog entries.
  This is labelled **Exact game-data match; NES file header excluded**. The
  original ROM file and its full-file verification checksums are unchanged.
- Extra padding, repeated flash banks, trainers, patches and altered ROM bytes
  are not guessed away. Unknown content says **No exact database match**.
  Multiple catalog entries sharing a checksum are shown as ambiguous; artwork
  is not chosen automatically. A missing match does not mean the dump is bad.
- Game identity does not establish the physical board, write compatibility,
  cartridge authenticity or playability. All existing cartridge checks still apply before a write.
- This first provider offers box images, not guaranteed physical cartridge-label
  scans. Descriptions, manual game association, additional media providers and
  label scans are not included in this release.

## Storage and errors

The bundled catalog works without an account, API key, emulator or network.
Only a recognized game title and platform are used in artwork requests to the
Libretro thumbnail repositories on GitHub. ROM bytes, filenames and checksums
are never uploaded. Requests are HTTPS, bounded in size, and have timeouts.

The shared library keeps artwork under `game-cache/artwork/`. Successful GUI,
TUI and CLI reads retain `game.json` and available `artwork/*.png` beside
the ROM and read report. Those references are relative, so a copied backup keeps
its images. Later artwork retries update an existing matching sidecar.

Identification begins after USB is released and a successful read report is
saved. An offline connection, missing image, unavailable catalog, cache write
failure or cancelled lookup cannot invalidate that completed cartridge read.
Messages explain whether to retry artwork, check library folder access, or
reinstall a missing catalog. Failed or incomplete cartridge reads do not trigger
game lookup. Read/backup without a valid ROM output retains its existing warning.

CLI examples:

```sh
cartridge game-info '/path/to/game.gb'
cartridge --json game-info '/path/to/game.nes' --artwork
cartridge famicom read --no-artwork
cartridge gameboy read --no-artwork
```

## Sources and packaging

The catalog contains 38,930 records from the Game Boy, Game Boy Color, Game Boy Advance and NES
databases in [Libretro database](https://github.com/libretro/libretro-database),
pinned to revision `92a7c5adf8c8362d7b88a35042f0211ae3d88316`. Libretro credits
its upstream contributors, including No-Intro. It is licensed under
[CC BY-SA 4.0](https://github.com/libretro/libretro-database/blob/92a7c5adf8c8362d7b88a35042f0211ae3d88316/LICENSE).
We convert RDB records to a platform-limited SQLite index. The packaged data is
under the same license, with attribution and the complete license included.

[Libretro thumbnails](https://github.com/libretro-thumbnails/libretro-thumbnails)
supplies separately downloaded artwork. The database license does not grant
rights to the games or artwork. The image sources credit their contributors and
the respective games' developers/publishers. Images are not bundled in releases.

The portable Linux app and Arch package embed the offline index in the Rust
backend and include attribution under `licenses/`. The native build prepares it
automatically. The public beta has no Python wheel distribution. To prepare and
test the source catalog, run:

```sh
python3 scripts/build_game_catalog.py
CARGO_HOME="$PWD/tmp/toolchains/cargo" cargo test --locked --offline -p cartridge-core --test games
```

The first build downloads public metadata and verifies pinned SHA-256 values;
subsequent builds can reuse those verified inputs offline. All generated data
stays under `tmp/game-catalog/`. Updating the catalog is an explicit release
maintenance step: update the pin and hashes, rebuild, and rerun the tests.
`CARTRIDGE_STUDIO_GAME_CATALOG` can select a local test catalog. It does not trigger downloads.

The Rust core now owns all matching, catalog access and artwork handling. The
pinned SQLite catalog is embedded in the native binaries; notices accompany the
packages. No catalog extraction or writable cache is required for identification.
