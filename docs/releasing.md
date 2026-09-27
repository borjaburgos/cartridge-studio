# Publishing Cartridge Studio

The repository is the source for the app, documentation, roadmap and project
website. Release packages contain only the selected application payload; `tmp/`,
cartridge images, user settings, private diagnostic reports and downloaded game
artwork must never be published.

## Prepare a release

1. Update the workspace version in `Cargo.toml`, the release notes, README and
   the versioned links in `website/index.html`. Refresh `Cargo.lock` by building.
2. Run the offline Rust checks and Python packaging tests documented in
   [development](development.md). Run `python3 scripts/check_website.py`.
3. On the qualified Arch x86-64 build host, run
   `python3 scripts/build_desktop.py`. This builds from source, gathers upstream
   notices and packages only an explicit list of user documentation. A stale
   `--skip-build` binary is rejected.
4. Check every entry in `tmp/dist/SHA256SUMS`, run component installer tests
   against the actual bundle, and run `python3 scripts/test_arch_packages.py`.
5. Review `runtime-requirements.json` and record the measured minimum glibc in
   the website and release notes. Inspect package contents and license reports.
   These local builds are not claimed to be reproducible. Personal build paths
   are remapped, archive ownership is normalized and the Arch `.BUILDINFO`
   machine inventory is omitted from public packages. Complete local packages
   remain under ignored `tmp/` for diagnostics.

## Publish matching source and binaries

Commit the reviewed source and documentation. Update `main` with the reviewed
commit and create an annotated `vVERSION` tag on that exact commit; never retag a
published release. Create a draft GitHub release with the six versioned Linux
artifacts and `SHA256SUMS`, using a body file so the notes retain their formatting.
Download the assets from that draft to a fresh directory and verify the checksums
again. Compare the remote asset list with the checksum manifest, then publish the
release. Mark prerelease versions such as `0.1.0-beta.1` as GitHub prereleases;
do not label them as a stable or latest release. The website pins the beta tag
because GitHub’s stable `releases/latest` endpoint excludes prereleases.
Preserve published public release tags.

The six artifacts are the portable `.tar.gz` archive, the Arch all-interface
metapackage, and the Arch `core`, `gui`, `tui` and `cli` packages. The Arch
metapackage alone is not a complete installation; include its dependencies.
Cargo and portable archives use SemVer (`0.1.0-beta.1`); Arch packages use
`0.1.0beta1`, which sorts before `0.1.0` in pacman. All split-package dependencies
must use the same Arch version.

The project website pins a specific verified release. Its download URLs must
only advance after the corresponding assets exist. This avoids silently sending
people to unrelated future release assets with different names or requirements.

## Publish the website

GitHub Pages serves `website/` through `.github/workflows/pages.yml`. Enable
Pages with GitHub Actions as its source. The workflow validates local assets,
documentation links and versioned downloads before upload. Set the repository
homepage to `https://borjaburgos.github.io/cartridge-studio/`.

The site is plain HTML, CSS and a small optional script. It has no package
installation or build step, analytics, third-party fonts or cookies. Preview it
with `python3 -m http.server 8773 --directory website` during development.
The website includes a real headless app screenshot with synthetic homebrew
metadata. It contains no game artwork, personal paths or cartridge bytes.

The public project begins with `0.1.0-beta.1`, one initial source commit and one
beta release. Private development history and earlier release packages were
retained in an ignored local recovery archive before the public launch.
Never include that archive in source exports or release assets.
