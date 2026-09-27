#!/usr/bin/env python3
"""Build native Rust executables and self-contained Linux distributions.

Python is a development tool only. It is not included or required at runtime.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tarfile
import tomllib
from urllib.parse import quote
from build_rust import build as build_rust
from build_arch import arch_version, build_arch

ROOT = Path(__file__).resolve().parents[1]
VERSION = tomllib.loads((ROOT/'Cargo.toml').read_text())['workspace']['package']['version']
PROGRAMS = ('cartridge-studio', 'cartridge-tui', 'cartridge', 'cartridge-worker')
DOCUMENTS = ('install.md', 'support.md', 'roadmap.md', 'readers.md', 'desktop.md',
    'tui.md', 'detection.md', 'games.md', 'release-notes.md', 'third-party.md')
# These are the desktop integration libraries loaded by winit/softbuffer, not
# an application runtime. Bundle their dependency closure except the OS libc.
LINUX_LIBRARIES = ('libgcc_s.so.1', 'libwayland-client.so.0', 'libwayland-cursor.so.0',
    'libxkbcommon.so.0', 'libxkbcommon-x11.so.0', 'libX11.so.6', 'libX11-xcb.so.1',
    'libXcursor.so.1', 'libXrandr.so.2', 'libXi.so.6', 'libXinerama.so.1',
    'libXrender.so.1', 'libXfixes.so.3', 'libXext.so.6', 'libxcb.so.1',
    'libxcb-render.so.0', 'libxcb-shape.so.0', 'libxcb-xfixes.so.0',
    'libxcb-randr.so.0', 'libxcb-xinput.so.0')
SYSTEM_LIBRARIES = ('libc.so.', 'libm.so.', 'libdl.so.', 'libpthread.so.', 'librt.so.', 'ld-linux')

def validate_build_host():
    if platform.system() != 'Linux':
        raise SystemExit('Release packaging currently requires Arch Linux. Build the Rust workspace directly on macOS; signed macOS packaging is not yet qualified.')
    missing = [tool for tool in ('pacman', 'makepkg', 'ldd', 'readelf') if not shutil.which(tool)]
    if missing:
        raise SystemExit('This release recipe bundles Arch libraries and builds Arch packages. '
            f'Use an Arch Linux build host with these tools installed: {", ".join(missing)}. '
            'For a source build on another Linux distribution, use python3 scripts/build_rust.py.')


def validate_release_inputs(binaries):
    # Check before replacing any existing bundle. In particular, --skip-build must
    # never publish older executables under the version in the current manifest.
    for name in PROGRAMS:
        if name == 'cartridge-worker':
            continue  # The package uses the CLI executable in worker mode.
        executable = binaries/name
        if not executable.is_file():
            raise SystemExit(f'Missing release executable: {executable}. Run python3 scripts/build_rust.py before packaging.')
        try:
            reported = subprocess.check_output([str(executable), '--version'], text=True, timeout=15).strip()
        except (OSError, subprocess.SubprocessError) as error:
            raise SystemExit(f'Cannot verify {executable}: {error}. Rebuild the release executables before packaging.') from error
        if VERSION not in reported.split():
            raise SystemExit(f'Release version mismatch: {name} reports {reported!r}; Cargo.toml declares {VERSION}. '
                'Run python3 scripts/build_rust.py, then package again without stale binaries.')
    required = [ROOT/'LICENSE', ROOT/'tmp/rust-licenses/rust-components.json',
        *(ROOT/'tmp/game-catalog'/name for name in ('NOTICE.txt', 'LIBRETRO-DATABASE-LICENSE.txt')),
        *(ROOT/'docs'/name for name in DOCUMENTS)]
    for path in required:
        if not path.is_file():
            raise SystemExit(f'Missing release input: {path}. Restore the versioned documentation or run python3 scripts/build_rust.py to rebuild dependency notices.')

def system_package_record(package, database=Path('/var/lib/pacman/local')):
    name, version = subprocess.check_output(['pacman', '-Q', package], text=True).strip().split(maxsplit=1)
    description = database/f'{name}-{version}'/'desc'
    fields = {}
    for section in description.read_text().split('\n\n'):
        lines = section.splitlines()
        if lines:
            fields[lines[0].strip('%')] = lines[1:]
    base = fields.get('BASE', [name])[0]
    architecture = fields['ARCH'][0]
    archive_name = f'{name}-{version}-{architecture}.pkg.tar.zst'
    return {'package': name, 'version': version, 'source_base': base,
        'architecture': architecture, 'upstream_url': fields.get('URL', [None])[0],
        'licenses': fields.get('LICENSE', []),
        'packaging_repository': f'https://gitlab.archlinux.org/archlinux/packaging/packages/{quote(base, safe="")}',
        'binary_archive_url': f'https://archive.archlinux.org/packages/{name[0]}/{quote(name, safe="")}/{quote(archive_name, safe="")}' }


def public_tar_member(member):
    # A portable release must not expose the build account through tar ownership.
    member.uid = member.gid = 0
    member.uname = member.gname = 'root'
    return member


def bundle_libraries(bundle):
    destination = bundle/'lib'; destination.mkdir()
    notices = bundle/'licenses/system'; notices.mkdir(parents=True)
    pending = [Path('/usr/lib')/name for name in LINUX_LIBRARIES]
    seen = set(); packages = {}; files = []
    while pending:
        path = pending.pop()
        if path.name in seen or path.name.startswith(SYSTEM_LIBRARIES): continue
        if not path.is_file(): raise RuntimeError(f'Build host is missing {path}; install its development/runtime package before packaging.')
        seen.add(path.name)
        shutil.copy2(path.resolve(), destination/path.name)
        listing = subprocess.check_output(['ldd', str(path)], text=True)
        for line in listing.splitlines():
            fields = line.split()
            if len(fields) >= 3 and fields[1] == '=>' and fields[2].startswith('/'):
                pending.append(Path(fields[2]))
            elif 'not found' in line: raise RuntimeError(f'Missing linked library: {line}')
        package = subprocess.check_output(['pacman', '-Qoq', str(path.resolve())], text=True).strip()
        if package not in packages:
            packages[package] = system_package_record(package)
            licenses = Path('/usr/share/licenses')/package
            if licenses.exists(): shutil.copytree(licenses, notices/package, dirs_exist_ok=True)
        files.append({'filename': path.name,
            'sha256': hashlib.sha256((destination/path.name).read_bytes()).hexdigest(),
            **packages[package]})
    # Some windowing libraries request unversioned dlopen names before SONAMEs.
    for library in list(destination.iterdir()):
        if '.so.' in library.name:
            alias = destination/(library.name.split('.so.')[0]+'.so')
            if not alias.exists(): alias.symlink_to(library.name)
    # GCC runtime linking exception and common system licenses are supplied by Arch.
    for name in ('GPL-3.0-only', 'LGPL-2.1-only', 'LGPL-2.1-or-later', 'MIT', 'BSD-3-Clause'):
        path = Path('/usr/share/licenses/spdx')/(name+'.txt')
        if path.is_file(): shutil.copy2(path, notices/path.name)
    (notices/'packages.json').write_text(json.dumps({'libraries':sorted(seen), 'packages':packages,
        'files': sorted(files, key=lambda item: item['filename']),
        'source':'https://gitlab.archlinux.org/archlinux/packaging/packages',
        'note':'Unmodified Arch libraries with file hashes and exact installed package identities. Source bases identify the packaging repositories, including split packages. Repository URLs are not pinned source revisions; archive URLs are derived from installed package identities. GCC runtime exception applies to libgcc_s.'}, indent=2)+'\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--skip-build', action='store_true', help='Package already-built release executables')
    args = parser.parse_args()
    package_version = arch_version(VERSION)
    validate_build_host()
    binaries = ROOT/'tmp/rust-target/release' if args.skip_build else build_rust()
    validate_release_inputs(binaries)
    build, dist = ROOT/'tmp/native-package', ROOT/'tmp/dist'
    build.mkdir(parents=True, exist_ok=True); dist.mkdir(parents=True, exist_ok=True)
    bundle = dist/'cartridge-studio'
    if bundle.exists(): shutil.rmtree(bundle)
    (bundle/'bin').mkdir(parents=True)
    (bundle/'VERSION').write_text(VERSION+'\n')
    for name in PROGRAMS:
        if name == 'cartridge-worker': os.link(bundle/'bin/cartridge', bundle/'bin/cartridge-worker')
        else: shutil.copy2(binaries/name, bundle/'bin'/name)
        launcher = bundle/name
        launcher.write_text('#!/bin/sh\nset -eu\nAPP_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)\nexport LD_LIBRARY_PATH="$APP_DIR/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"\nexec "$APP_DIR/bin/'+name+'" "$@"\n')
        launcher.chmod(0o755)
    (bundle/'licenses').mkdir()
    shutil.copy2(ROOT/'LICENSE', bundle/'licenses/CARTRIDGE-STUDIO-LICENSE.txt')
    shutil.copytree(ROOT/'tmp/rust-licenses', bundle/'licenses/rust')
    bundle_libraries(bundle)
    for name in ('NOTICE.txt', 'LIBRETRO-DATABASE-LICENSE.txt'):
        shutil.copy2(ROOT/'tmp/game-catalog'/name, bundle/'licenses'/name)
    shutil.copy2(ROOT/'desktop/assets/CREDITS.md', bundle/'licenses/PLATFORM-LOGOS.md')
    shutil.copy2(ROOT/'desktop/assets/FIRA-LICENSE.txt', bundle/'licenses/FIRA-LICENSE.txt')
    shutil.copy2(ROOT/'docs/third-party.md', bundle/'licenses/THIRD-PARTY.md')
    shutil.copy2(ROOT/'README.md', bundle/'README.md')
    # Only intentional, versioned documentation; no unrelated local work enters releases.
    for name in DOCUMENTS:
        target = bundle/'docs'/name; target.parent.mkdir(exist_ok=True)
        shutil.copy2(ROOT/'docs'/name, target)
    for name in ('70-cartridge-studio.rules', 'io.github.borjaburgos.CartridgeStudio.desktop', 'io.github.borjaburgos.CartridgeStudio.Terminal.desktop'):
        shutil.copy2(ROOT/'desktop'/name, bundle/name)
    shutil.copy2(ROOT/'desktop/assets/studio.svg', bundle/'io.github.borjaburgos.CartridgeStudio.svg')
    shutil.copy2(ROOT/'scripts/install_native.sh', bundle/'install.sh')
    versions = set()
    for path in [*(bundle/'bin').iterdir(), *(bundle/'lib').iterdir()]:
        text = subprocess.check_output(['readelf', '--version-info', str(path)], text=True)
        versions.update(tuple(map(int, v.split('.'))) for v in re.findall(r'GLIBC_([0-9.]+)', text))
    required_glibc = '.'.join(map(str, max(versions)))
    (bundle/'runtime-requirements.json').write_text(json.dumps({'platform':'Linux','architecture':platform.machine(),'minimum_glibc':required_glibc,'display':'Wayland or X11; terminal interface needs neither','application_runtime':'Rust; no Python, GTK, browser or libusb'},indent=2)+'\n')
    arch = platform.machine()
    packages = build_arch(bundle, build/'arch', dist, VERSION, arch, required_glibc)
    archive = dist/f'cartridge-studio-{VERSION}-linux-{arch}.tar.gz'
    with tarfile.open(archive, 'w:gz') as output:
        output.add(bundle, arcname='cartridge-studio', filter=public_tar_member)
    products = [archive, *packages]
    (dist/'SHA256SUMS').write_text(''.join(f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n' for p in products))
    print(json.dumps({'version':VERSION,'arch_package_version':package_version,'portable':str(archive),'arch_packages':[str(p) for p in packages],'executable':str(bundle/'cartridge-studio'),'runtime':'Rust; bundled desktop integration libraries; host glibc'},indent=2))

if __name__ == '__main__': main()
