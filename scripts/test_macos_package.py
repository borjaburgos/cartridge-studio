#!/usr/bin/env python3
"""Validate the macOS app, DMG, selectable package, linkage, and checksums."""

import hashlib
import json
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
VERSION = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
DIST = ROOT / "tmp/dist-macos"


def output(*args):
    return subprocess.check_output(args, text=True).strip()


def main():
    metadata = json.loads((DIST / "build-metadata.json").read_text())
    assert metadata["version"] == VERSION
    assert metadata["architecture"] == "arm64"
    artifacts = [Path(path) for path in metadata["artifacts"]]
    dmg = next(path for path in artifacts if path.suffix == ".dmg")
    pkg = next(path for path in artifacts if path.suffix == ".pkg")
    if metadata["notarized"]:
        assert "-unsigned-local" not in dmg.name and "-not-notarized" not in dmg.name
    elif metadata["application_identity"] or metadata["installer_identity"]:
        assert "-local-not-notarized" in dmg.name and "-local-not-notarized" in pkg.name
    else:
        assert "-unsigned-local" in dmg.name and "-unsigned-local" in pkg.name
    subprocess.run(["hdiutil", "verify", str(dmg)], check=True, stdout=subprocess.DEVNULL)
    if metadata["application_identity"]:
        subprocess.run(["codesign", "--verify", "--strict", str(dmg)], check=True)
        signature = subprocess.run(
            ["codesign", "--display", "--verbose=4", str(dmg)],
            text=True,
            capture_output=True,
            check=True,
        )
        assert "Authority=Developer ID Application" in signature.stdout + signature.stderr
    signature_check = subprocess.run(
        ["pkgutil", "--check-signature", str(pkg)], text=True, capture_output=True
    )
    package_info = signature_check.stdout + signature_check.stderr
    if metadata["installer_identity"]:
        assert signature_check.returncode == 0, package_info
        assert "Developer ID Installer" in package_info, package_info
    else:
        assert signature_check.returncode != 0 and "Status: no signature" in package_info, package_info

    with tempfile.TemporaryDirectory(prefix="cartridge-studio-validation-", dir="/private/tmp") as temporary:
        temporary = Path(temporary)
        mount = temporary / "mounted"
        mount.mkdir()
        subprocess.run(["hdiutil", "attach", "-readonly", "-nobrowse", "-mountpoint", str(mount), str(dmg)], check=True, stdout=subprocess.DEVNULL)
        try:
            app = mount / "Cartridge Studio.app"
            info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
            assert info["CFBundleShortVersionString"] == VERSION
            assert info["CFBundleVersion"].isdigit()
            assert info["LSMinimumSystemVersion"] == "13.0"
            assert info["CFBundleIdentifier"] == "io.github.borjaburgos.CartridgeStudio"
            assert info["NSHighResolutionCapable"] is True
            subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True)
            if metadata["application_identity"]:
                signature = subprocess.run(
                    ["codesign", "--display", "--verbose=4", str(app)],
                    text=True,
                    capture_output=True,
                    check=True,
                )
                assert "Authority=Developer ID Application" in (
                    signature.stdout + signature.stderr
                )

            expanded = temporary / "expanded"
            subprocess.run(["pkgutil", "--expand-full", str(pkg), str(expanded)], check=True)
            distribution = (expanded / "Distribution").read_text()
            for choice in ("gui", "tui", "cli"):
                assert f'id="{choice}"' in distribution
            assert 'id="core" visible="false" start_selected="true"' in distribution
            payloads = {}
            roots = {}
            for component, expected in {
                "core": "usr/local/libexec/cartridge-studio/cartridge-worker",
                "gui": "Applications/Cartridge Studio.app/Contents/MacOS/cartridge-studio",
                "tui": "usr/local/bin/cartridge-tui",
                "cli": "usr/local/bin/cartridge",
            }.items():
                root = expanded / f"{component}.pkg/Payload"
                executable = root / expected
                assert executable.is_file(), f"{component}: {expected}"
                roots[component] = executable
                payloads[component] = {
                    "entrypoint": expected,
                    "files": sum(1 for path in root.rglob("*") if path.is_file()),
                }
            preinstall = expanded / "core.pkg/Scripts/preinstall"
            script = preinstall.read_text()
            assert "Application Support" not in script
            assert "cartridge-studio/components" in script
            assert all(f'"$managed/{name}"' in script for name in ("gui", "tui", "cli"))

            choice_sets = {}
            for name, selected in {
                "gui-only": {"gui"},
                "tui-only": {"tui"},
                "cli-only": {"cli"},
                "combined": {"gui", "tui", "cli"},
            }.items():
                changes = temporary / f"choices-{name}.plist"
                changes.write_bytes(
                    plistlib.dumps(
                        [
                            {
                                "choiceIdentifier": choice,
                                "choiceAttribute": "selected",
                                "attributeSetting": int(choice in selected),
                            }
                            for choice in ("gui", "tui", "cli")
                        ]
                    )
                )
                applied = subprocess.check_output(
                    [
                        "/usr/sbin/installer",
                        "-showChoicesAfterApplyingChangesXML",
                        str(changes),
                        "-pkg",
                        str(pkg),
                        "-target",
                        "/",
                    ]
                )
                model = plistlib.loads(applied)
                choices = {}

                def collect_choices(value):
                    if isinstance(value, dict):
                        if "choiceIdentifier" in value and "choiceIsSelected" in value:
                            choices[value["choiceIdentifier"]] = bool(value["choiceIsSelected"])
                        elif value.get("choiceAttribute") == "selected":
                            choices[value["choiceIdentifier"]] = bool(value["attributeSetting"])
                        for child in value.values():
                            collect_choices(child)
                    elif isinstance(value, list):
                        for child in value:
                            collect_choices(child)

                collect_choices(model)
                assert choices["core"]
                assert {choice for choice in ("gui", "tui", "cli") if choices[choice]} == selected
                choice_sets[name] = sorted(selected)

            simulated = temporary / "simulated-target"
            library = simulated / "Users/test/Library/Application Support/Cartridge Studio"
            library.mkdir(parents=True)
            (library / "existing-backup.gb").write_bytes(b"existing synthetic backup")
            (library / "settings.json").write_text('{"existing":true}\n')
            selections = {
                "gui-only": {"gui"},
                "tui-only": {"tui"},
                "cli-only": {"cli"},
                "combined": {"gui", "tui", "cli"},
            }
            installed_paths = {
                "gui": simulated / "Applications/Cartridge Studio.app",
                "tui": simulated / "usr/local/bin/cartridge-tui",
                "cli": simulated / "usr/local/bin/cartridge",
            }
            for selected in selections.values():
                subprocess.run([str(preinstall), "", "", str(simulated)], check=True)
                for component in ("core", *sorted(selected)):
                    shutil.copytree(
                        expanded / f"{component}.pkg/Payload",
                        simulated,
                        dirs_exist_ok=True,
                        symlinks=True,
                    )
                assert (simulated / "usr/local/libexec/cartridge-studio/cartridge-worker").is_file()
                for component, path in installed_paths.items():
                    assert path.exists() == (component in selected), (component, selected)
                assert (library / "existing-backup.gb").read_bytes() == b"existing synthetic backup"
                assert (library / "settings.json").read_text() == '{"existing":true}\n'

            executables = [
                app / "Contents/MacOS/cartridge-studio",
                app / "Contents/MacOS/cartridge-worker",
                roots["core"],
                roots["tui"],
                roots["cli"],
            ]
            linkage = {}
            for executable in executables:
                file_info = output("file", str(executable))
                assert "arm64" in file_info and "x86_64" not in file_info, file_info
                linked = output("otool", "-L", str(executable))
                libraries = [line.strip().split(" ", 1)[0] for line in linked.splitlines()[1:]]
                assert all(path.startswith(("/usr/lib/", "/System/Library/")) for path in libraries), linked
                binary = executable.read_bytes()
                assert not any(
                    token in binary
                    for token in (b"/opt/homebrew", b"/Users/", b"/private/tmp/")
                )
                load = output("vtool", "-show-build", str(executable))
                minimum = re.search(r"minos ([0-9.]+)", load)
                assert minimum and minimum.group(1) == "13.0", load
                subprocess.run(["codesign", "--verify", "--strict", str(executable)], check=True)
                if metadata["application_identity"]:
                    signature = subprocess.run(
                        ["codesign", "--display", "--verbose=4", str(executable)],
                        text=True,
                        capture_output=True,
                        check=True,
                    )
                    assert "Authority=Developer ID Application" in (
                        signature.stdout + signature.stderr
                    )
                linkage[f"{executable.parent.name}/{executable.name}"] = libraries

            if metadata["notarized"]:
                subprocess.run(["xcrun", "stapler", "validate", str(dmg)], check=True)
                subprocess.run(["xcrun", "stapler", "validate", str(pkg)], check=True)
                subprocess.run(["spctl", "--assess", "--type", "exec", str(app)], check=True)
                subprocess.run(["spctl", "--assess", "--type", "install", str(pkg)], check=True)
        finally:
            subprocess.run(["hdiutil", "detach", str(mount)], check=True, stdout=subprocess.DEVNULL)

    for line in (DIST / "SHA256SUMS").read_text().splitlines():
        expected, name = line.split(maxsplit=1)
        assert hashlib.sha256((DIST / name).read_bytes()).hexdigest() == expected, name
    report = {
        "version": VERSION,
        "architecture": "arm64",
        "minimum_macos": "13.0",
        "bundle_metadata": True,
        "retina": True,
        "system_only_linkage": linkage,
        "component_payloads": payloads,
        "installer_choice_sets": choice_sets,
        "selection_reinstall_and_change": True,
        "reconfiguration_script_preserves_user_data": True,
        "checksums": True,
        "signature": metadata["local_signature"],
        "notarized": metadata["notarized"],
        "hardware_access": False,
    }
    target = ROOT / "tmp/macos-package-validation.json"
    target.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
