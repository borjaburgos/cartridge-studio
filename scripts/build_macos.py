#!/usr/bin/env python3
"""Build, sign when possible, and package the Apple Silicon macOS candidate."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

from build_rust import build as build_rust

ROOT = Path(__file__).resolve().parents[1]
VERSION = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
ARCH = "arm64"
MINIMUM_MACOS = "13.0"
BUNDLE_ID = "io.github.borjaburgos.CartridgeStudio"
PROGRAMS = ("cartridge-studio", "cartridge-tui", "cartridge")
DOCUMENTS = (
    "install.md",
    "support.md",
    "roadmap.md",
    "readers.md",
    "desktop.md",
    "tui.md",
    "detection.md",
    "games.md",
    "release-notes.md",
    "macos-validation.md",
    "third-party.md",
)


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def output(*args):
    return subprocess.check_output(args, text=True).strip()


def reset(directory: Path):
    if directory.exists():
        shutil.rmtree(directory)
    directory.mkdir(parents=True)


def validate_host():
    if platform.system() != "Darwin" or platform.machine() != ARCH:
        raise SystemExit(
            "This qualified recipe requires Apple Silicon macOS. Intel and Universal builds are separate targets."
        )
    required = ("cargo", "codesign", "hdiutil", "iconutil", "pkgbuild", "productbuild", "qlmanage", "sips", "xattr")
    missing = [tool for tool in required if not shutil.which(tool)]
    if missing:
        raise SystemExit(f"Install Xcode command line tools; missing: {', '.join(missing)}")


def validate_inputs(binaries: Path):
    for name in PROGRAMS:
        executable = binaries / name
        if not executable.is_file():
            raise SystemExit(f"Missing release executable: {executable}")
        reported = output(str(executable), "--version")
        if VERSION not in reported.split():
            raise SystemExit(f"Version mismatch: {name} reports {reported!r}; expected {VERSION}")
    required = [
        ROOT / "LICENSE",
        ROOT / "tmp/rust-licenses/rust-components.json",
        ROOT / "tmp/game-catalog/NOTICE.txt",
        ROOT / "tmp/game-catalog/LIBRETRO-DATABASE-LICENSE.txt",
        *(ROOT / "docs" / name for name in DOCUMENTS),
    ]
    missing = [str(path) for path in required if not path.is_file()]
    if missing:
        raise SystemExit("Missing release input(s):\n" + "\n".join(missing))


def make_icon(destination: Path, work: Path):
    iconset = work / "AppIcon.iconset"
    iconset.mkdir()
    source = ROOT / "desktop/macos/AppIcon.svg"
    # Quick Look uses macOS's vector renderer and preserves transparency. It
    # produces SOURCE.svg.png in the requested output directory.
    render = work / "icon-render"
    render.mkdir()
    run("qlmanage", "-t", "-s", "1024", "-o", str(render), str(source), stdout=subprocess.DEVNULL)
    master = render / f"{source.name}.png"
    if not master.is_file():
        raise RuntimeError(f"macOS did not render the application icon: {master}")
    sizes = ((16, 1), (16, 2), (32, 1), (32, 2), (128, 1), (128, 2), (256, 1), (256, 2), (512, 1), (512, 2))
    for logical, scale in sizes:
        pixels = logical * scale
        suffix = f"{logical}x{logical}" + ("@2x" if scale == 2 else "")
        run(
            "sips",
            "-z",
            str(pixels),
            str(pixels),
            str(master),
            "--out",
            str(iconset / f"icon_{suffix}.png"),
            stdout=subprocess.DEVNULL,
        )
    run("iconutil", "-c", "icns", str(iconset), "-o", str(destination))


def copy_notices(destination: Path):
    destination.mkdir(parents=True)
    shutil.copy2(ROOT / "LICENSE", destination / "CARTRIDGE-STUDIO-LICENSE.txt")
    shutil.copytree(ROOT / "tmp/rust-licenses", destination / "rust")
    for name in ("NOTICE.txt", "LIBRETRO-DATABASE-LICENSE.txt"):
        shutil.copy2(ROOT / "tmp/game-catalog" / name, destination / name)
    shutil.copy2(ROOT / "desktop/assets/CREDITS.md", destination / "PLATFORM-LOGOS.md")
    shutil.copy2(ROOT / "desktop/assets/FIRA-LICENSE.txt", destination / "FIRA-LICENSE.txt")
    shutil.copy2(ROOT / "docs/third-party.md", destination / "THIRD-PARTY.md")


def sign(path: Path, identity: str | None, *, bundle=False):
    if identity:
        args = ["codesign", "--force", "--sign", identity, "--options", "runtime", "--timestamp"]
    else:
        args = ["codesign", "--force", "--sign", "-", "--timestamp=none"]
    if path.resolve().is_relative_to(Path("/private/tmp").resolve()):
        run(*args, str(path))
        return
    # macOS attaches immutable provenance metadata to files created in some
    # user document providers. codesign rejects that metadata while writing a
    # new signature. Sign a no-resource-fork copy on the system temporary
    # volume, then copy the embedded signature back into the ignored staging
    # tree. The temporary directory is always removed on exit.
    with tempfile.TemporaryDirectory(prefix="cartridge-studio-sign-", dir="/private/tmp") as temporary:
        staged = Path(temporary) / path.name
        run("ditto", "--norsrc", "--noextattr", str(path), str(staged))
        run(*args, str(staged))
        if path.is_dir():
            shutil.rmtree(path)
        else:
            path.unlink()
        run("ditto", "--norsrc", "--noextattr", str(staged), str(path))


def make_app(binaries: Path, destination: Path, work: Path, identity: str | None):
    macos = destination / "Contents/MacOS"
    resources = destination / "Contents/Resources"
    macos.mkdir(parents=True)
    resources.mkdir()
    shutil.copy2(binaries / "cartridge-studio", macos / "cartridge-studio")
    # The CLI dispatches worker mode from argv[0], preserving the shared Rust engine.
    shutil.copy2(binaries / "cartridge", macos / "cartridge-worker")
    make_icon(resources / "AppIcon.icns", work)
    copy_notices(resources / "Third-Party Notices")
    shutil.copy2(ROOT / "README.md", resources / "README.md")
    docs = resources / "Documentation"
    docs.mkdir()
    for name in DOCUMENTS:
        shutil.copy2(ROOT / "docs" / name, docs / name)
    info = {
        "CFBundleDevelopmentRegion": "en",
        "CFBundleDisplayName": "Cartridge Studio",
        "CFBundleExecutable": "cartridge-studio",
        "CFBundleIconFile": "AppIcon",
        "CFBundleIdentifier": BUNDLE_ID,
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleName": "Cartridge Studio",
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": VERSION,
        "CFBundleVersion": "2",
        "LSArchitecturePriority": [ARCH],
        "LSMinimumSystemVersion": MINIMUM_MACOS,
        "NSHighResolutionCapable": True,
        "NSHumanReadableCopyright": "Copyright © 2026 Cartridge Studio contributors",
        "NSPrincipalClass": "NSApplication",
        "CFBundleDocumentTypes": [
            {
                "CFBundleTypeName": "Cartridge ROM",
                "CFBundleTypeRole": "Viewer",
                "LSHandlerRank": "Alternate",
                "CFBundleTypeExtensions": ["gb", "gbc", "gba", "nes"],
            }
        ],
    }
    with (destination / "Contents/Info.plist").open("wb") as handle:
        plistlib.dump(info, handle, sort_keys=True)
    # copy2 preserves extended metadata from the build volume. Release bundles
    # must not contain Finder/resource-fork detritus, and provenance belongs to
    # the downloadable artifact rather than an intermediate build file.
    run("xattr", "-cr", str(destination))
    sign(macos / "cartridge-worker", identity)
    sign(macos / "cartridge-studio", identity)
    sign(destination, identity, bundle=True)


def core_scripts(destination: Path):
    destination.mkdir()
    script = destination / "preinstall"
    script.write_text(
        """#!/bin/sh
set -eu
target=${3:-/}
managed="$target/usr/local/share/cartridge-studio/components"
if [ -f "$managed/gui" ]; then
  plist="$target/Applications/Cartridge Studio.app/Contents/Info.plist"
  if [ -f "$plist" ] && [ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$plist" 2>/dev/null || true)" = "io.github.borjaburgos.CartridgeStudio" ]; then
    rm -rf "$target/Applications/Cartridge Studio.app"
  fi
fi
for component in tui cli; do
  [ -f "$managed/$component" ] || continue
  case "$component" in tui) program=cartridge-tui ;; cli) program=cartridge ;; esac
  rm -f "$target/usr/local/bin/$program"
done
rm -f "$managed/gui" "$managed/tui" "$managed/cli"
exit 0
"""
    )
    script.chmod(0o755)


def marker(root: Path, name: str):
    path = root / "usr/local/share/cartridge-studio/components" / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(VERSION + "\n")


def make_pkg(
    app: Path,
    binaries: Path,
    work: Path,
    destination: Path,
    application_identity: str | None,
    installer_identity: str | None,
):
    roots = work / "pkg-roots"
    packages = work / "component-packages"
    scripts = work / "pkg-scripts"
    roots.mkdir()
    packages.mkdir()

    core = roots / "core"
    engine = core / "usr/local/libexec/cartridge-studio"
    engine.mkdir(parents=True)
    shutil.copy2(binaries / "cartridge", engine / "cartridge-worker")
    sign(engine / "cartridge-worker", application_identity)
    share = core / "usr/local/share/cartridge-studio"
    copy_notices(share / "licenses")
    (share / "VERSION").write_text(VERSION + "\n")
    docs = share / "docs"
    docs.mkdir()
    for name in DOCUMENTS:
        shutil.copy2(ROOT / "docs" / name, docs / name)
    core_scripts(scripts)

    gui = roots / "gui"
    (gui / "Applications").mkdir(parents=True)
    shutil.copytree(app, gui / "Applications/Cartridge Studio.app", symlinks=True)
    marker(gui, "gui")

    tui = roots / "tui"
    (tui / "usr/local/bin").mkdir(parents=True)
    shutil.copy2(binaries / "cartridge-tui", tui / "usr/local/bin/cartridge-tui")
    sign(tui / "usr/local/bin/cartridge-tui", application_identity)
    marker(tui, "tui")

    cli = roots / "cli"
    (cli / "usr/local/bin").mkdir(parents=True)
    shutil.copy2(binaries / "cartridge", cli / "usr/local/bin/cartridge")
    sign(cli / "usr/local/bin/cartridge", application_identity)
    marker(cli, "cli")

    identifiers = {
        "core": f"{BUNDLE_ID}.core",
        "gui": f"{BUNDLE_ID}.gui",
        "tui": f"{BUNDLE_ID}.tui",
        "cli": f"{BUNDLE_ID}.cli",
    }
    run("xattr", "-cr", str(roots))
    for component, identifier in identifiers.items():
        args = [
            "pkgbuild",
            "--root",
            str(roots / component),
            "--identifier",
            identifier,
            "--version",
            VERSION,
            "--install-location",
            "/",
        ]
        if component == "core":
            args.extend(("--scripts", str(scripts)))
        args.append(str(packages / f"{component}.pkg"))
        run(*args)

    distribution = work / "Distribution.xml"
    distribution.write_text(
        f"""<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
  <title>Cartridge Studio {VERSION}</title>
  <organization>{BUNDLE_ID}</organization>
  <domains enable_anywhere="false" enable_currentUserHome="false" enable_localSystem="true"/>
  <options customize="always" require-scripts="true" hostArchitectures="arm64"/>
  <welcome file="welcome.html" mime-type="text/html"/>
  <license file="license.txt" mime-type="text/plain"/>
  <conclusion file="conclusion.html" mime-type="text/html"/>
  <choices-outline>
    <line choice="core"/>
    <line choice="gui"/>
    <line choice="tui"/>
    <line choice="cli"/>
  </choices-outline>
  <choice id="core" visible="false" start_selected="true"><pkg-ref id="{identifiers['core']}"/></choice>
  <choice id="gui" title="Graphical application" description="Cartridge Studio.app for Finder and the Dock" start_selected="true"><pkg-ref id="{identifiers['gui']}"/></choice>
  <choice id="tui" title="Terminal interface" description="Installs /usr/local/bin/cartridge-tui" start_selected="false"><pkg-ref id="{identifiers['tui']}"/></choice>
  <choice id="cli" title="Command-line interface" description="Installs /usr/local/bin/cartridge" start_selected="false"><pkg-ref id="{identifiers['cli']}"/></choice>
  <pkg-ref id="{identifiers['core']}" version="{VERSION}" onConclusion="none">core.pkg</pkg-ref>
  <pkg-ref id="{identifiers['gui']}" version="{VERSION}" onConclusion="none">gui.pkg</pkg-ref>
  <pkg-ref id="{identifiers['tui']}" version="{VERSION}" onConclusion="none">tui.pkg</pkg-ref>
  <pkg-ref id="{identifiers['cli']}" version="{VERSION}" onConclusion="none">cli.pkg</pkg-ref>
</installer-gui-script>
"""
    )
    resources = work / "installer-resources"
    resources.mkdir()
    (resources / "welcome.html").write_text(
        f"<html><body><h2>Cartridge Studio {VERSION}</h2><p>Select the graphical application, terminal interface, command-line interface, or any combination. The shared native Rust engine is always installed.</p></body></html>"
    )
    shutil.copy2(ROOT / "LICENSE", resources / "license.txt")
    (resources / "conclusion.html").write_text(
        "<html><body><p>Open Cartridge Studio from Applications. Selected terminal commands are available in <code>/usr/local/bin</code>. Existing settings and cartridge backups were not changed.</p></body></html>"
    )
    args = [
        "productbuild",
        "--distribution",
        str(distribution),
        "--package-path",
        str(packages),
        "--resources",
        str(resources),
    ]
    if installer_identity:
        args.extend(("--sign", installer_identity, "--timestamp"))
    args.append(str(destination))
    run(*args)


def make_dmg(app: Path, work: Path, destination: Path, identity: str | None):
    source = work / "dmg-root"
    source.mkdir()
    shutil.copytree(app, source / app.name, symlinks=True)
    (source / "Applications").symlink_to("/Applications")
    shutil.copy2(ROOT / "docs/install.md", source / "INSTALL.md")
    run(
        "hdiutil",
        "create",
        "-fs",
        "HFS+",
        "-format",
        "UDZO",
        "-imagekey",
        "zlib-level=9",
        "-volname",
        f"Cartridge Studio {VERSION}",
        "-srcfolder",
        str(source),
        str(destination),
    )
    if identity:
        sign(destination, identity)


def archive_notices(notices: Path, destination: Path):
    with tarfile.open(destination, "w:gz") as archive:
        for path in sorted(notices.rglob("*")):
            archive.add(path, arcname=Path("Third-Party Notices") / path.relative_to(notices))


def notarize(path: Path, profile: str):
    run("xcrun", "notarytool", "submit", str(path), "--keychain-profile", profile, "--wait")
    run("xcrun", "stapler", "staple", str(path))
    run("xcrun", "stapler", "validate", str(path))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--application-identity", help="Developer ID Application identity")
    parser.add_argument("--installer-identity", help="Developer ID Installer identity")
    parser.add_argument("--notary-profile", help="notarytool keychain profile")
    args = parser.parse_args()
    if args.notary_profile and not (args.application_identity and args.installer_identity):
        raise SystemExit("Notarization requires both Developer ID Application and Installer identities.")
    validate_host()
    os.environ.setdefault("MACOSX_DEPLOYMENT_TARGET", MINIMUM_MACOS)
    binaries = ROOT / "tmp/rust-target/release" if args.skip_build else build_rust()
    validate_inputs(binaries)

    work = ROOT / "tmp/macos-package"
    dist = ROOT / "tmp/dist-macos"
    reset(work)
    reset(dist)
    if args.notary_profile:
        qualification = ""
    elif args.application_identity or args.installer_identity:
        qualification = "-local-not-notarized"
    else:
        qualification = "-unsigned-local"
    dmg = dist / f"Cartridge-Studio-{VERSION}-macOS-{ARCH}{qualification}.dmg"
    package = dist / f"Cartridge-Studio-{VERSION}-macOS-{ARCH}{qualification}.pkg"
    notices = dist / f"Cartridge-Studio-{VERSION}-Third-Party-Notices.tar.gz"
    # The signed payload is staged on the system temporary volume. Newer macOS
    # releases attach immutable document-provider provenance in ~/Documents;
    # staging there would make codesign reject otherwise clean bundle files.
    # Only final artifacts and review metadata persist under ignored tmp/.
    with tempfile.TemporaryDirectory(prefix="cartridge-studio-package-", dir="/private/tmp") as temporary:
        stage = Path(temporary)
        app = stage / "Cartridge Studio.app"
        make_app(binaries, app, stage, args.application_identity)
        staged_dmg = stage / dmg.name
        staged_package = stage / package.name
        make_dmg(app, stage, staged_dmg, args.application_identity)
        make_pkg(
            app,
            binaries,
            stage,
            staged_package,
            args.application_identity,
            args.installer_identity,
        )
        if args.notary_profile:
            notarize(staged_dmg, args.notary_profile)
            notarize(staged_package, args.notary_profile)
        shutil.copyfile(staged_dmg, dmg)
        shutil.copyfile(staged_package, package)
        archive_notices(app / "Contents/Resources/Third-Party Notices", notices)
        shutil.copy2(app / "Contents/Info.plist", work / "Info.plist")
        shutil.copy2(stage / "Distribution.xml", work / "Distribution.xml")
    install = dist / f"Cartridge-Studio-{VERSION}-INSTALL.md"
    notes = dist / f"Cartridge-Studio-{VERSION}-RELEASE-NOTES.md"
    shutil.copy2(ROOT / "docs/install.md", install)
    shutil.copy2(ROOT / "docs/release-notes.md", notes)
    products = (dmg, package, notices, install, notes)
    sums = dist / "SHA256SUMS"
    sums.write_text(
        "".join(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n" for path in products)
    )
    state = {
        "version": VERSION,
        "architecture": ARCH,
        "minimum_macos": MINIMUM_MACOS,
        "bundle_version": "2",
        "application_identity": args.application_identity,
        "installer_identity": args.installer_identity,
        "notarized": bool(args.notary_profile),
        "local_signature": "Developer ID" if args.application_identity else "ad hoc",
        "artifacts": [str(path) for path in (*products, sums)],
    }
    (dist / "build-metadata.json").write_text(json.dumps(state, indent=2) + "\n")
    print(json.dumps(state, indent=2))


if __name__ == "__main__":
    main()
