"""Split native Arch packages; pacman selects interfaces through dependencies."""
import gzip
from pathlib import Path
import os
import re
import shutil
import subprocess
import tempfile


def arch_version(version: str) -> str:
    """Translate supported SemVer prereleases into pacman's version ordering."""
    number = r'(?:0|[1-9][0-9]*)'
    match = re.fullmatch(rf'({number}\.{number}\.{number})(?:-(alpha|beta|rc)\.({number}))?', version)
    if not match:
        raise ValueError(f'Unsupported Arch release version: {version!r}. Use X.Y.Z or X.Y.Z-alpha.N, X.Y.Z-beta.N, X.Y.Z-rc.N.')
    base, channel, iteration = match.groups()
    return base + (channel + iteration if channel else '')


def public_package(original: Path, destination: Path):
    """Omit the private host inventory; preserve the local original separately.

    These local release packages do not publish makepkg's .BUILDINFO or claim
    reproducible-build provenance. Runtime package metadata and the file-integrity
    manifest remain intact, with the omitted entry also removed from .MTREE.
    """
    with tempfile.TemporaryDirectory(prefix='cartridge-studio-public-', dir='/tmp') as temporary:
        stage = Path(temporary)
        subprocess.run(['bsdtar', '-xf', str(original), '--no-same-owner',
            '--same-permissions', '-C', str(stage)], check=True)
        manifest = stage/'.MTREE'
        timestamp = manifest.stat()
        with gzip.open(manifest, 'rt') as source:
            lines = source.readlines()
        public_lines = [line for line in lines if not line.startswith('./.BUILDINFO ')]
        if len(public_lines) != len(lines) - 1:
            raise RuntimeError('Expected exactly one .BUILDINFO entry in the package file manifest.')
        manifest.write_bytes(gzip.compress(''.join(public_lines).encode(), mtime=0))
        os.utime(manifest, ns=(timestamp.st_atime_ns, timestamp.st_mtime_ns))
        (stage/'.BUILDINFO').unlink()
        subprocess.run(['bsdtar', '--zstd', '--uid', '0', '--gid', '0',
            '--uname', 'root', '--gname', 'root', '-cf', str(destination.resolve()),
            *sorted(path.name for path in stage.iterdir())], cwd=stage, check=True)


def build_arch(bundle: Path, output: Path, dist: Path, version: str, arch: str, glibc: str):
    package_version = arch_version(version)
    output.mkdir(parents=True, exist_ok=True)
    names = ['cartridge-studio', *('cartridge-studio-' + c for c in ('core', 'gui', 'tui', 'cli'))]
    recipe = f'''pkgbase=cartridge-studio
pkgname=({' '.join(names)})
pkgver={package_version}
pkgrel=1
pkgdesc='Native Rust cartridge preservation tools'
arch=('{arch}')
url='https://github.com/borjaburgos/cartridge-studio'
license=('MIT' 'Apache-2.0' 'CC-BY-SA-4.0' 'GPL-3.0-or-later WITH GCC-exception-3.1' 'OFL-1.1' 'MPL-2.0')
options=('!strip' '!debug')
_bundle="$startdir/bundle"

_launcher() {{
    install -d "$pkgdir/usr/bin"
    printf '#!/bin/sh\\nexec /opt/cartridge-studio/%s "$@"\\n' "$1" > "$pkgdir/usr/bin/$1"
    chmod 755 "$pkgdir/usr/bin/$1"
}}

package_cartridge-studio() {{
    replaces=('inl-cartridge-studio')
    conflicts=('inl-cartridge-studio')
    pkgdesc='Cartridge Studio — all interfaces'
    depends=("cartridge-studio-gui=$pkgver" "cartridge-studio-tui=$pkgver" "cartridge-studio-cli=$pkgver")
}}

package_cartridge-studio-core() {{
    replaces=('inl-cartridge-studio-core')
    conflicts=('inl-cartridge-studio-core')
    pkgdesc='Shared cartridge engine, catalog and reader support'
    depends=('glibc>={glibc}')
    local base="$pkgdir/opt/cartridge-studio"
    install -d "$base/bin" "$base/lib"
    for name in VERSION README.md docs licenses runtime-requirements.json 70-cartridge-studio.rules cartridge-worker; do
        cp -a --no-preserve=ownership "$_bundle/$name" "$base/"
    done
    cp -p --no-preserve=ownership "$_bundle/bin/cartridge-worker" "$base/bin/"
    cp -p --no-preserve=ownership "$_bundle/lib/libgcc_s.so.1" "$base/lib/"
    install -Dm644 "$_bundle/70-cartridge-studio.rules" "$pkgdir/usr/lib/udev/rules.d/70-cartridge-studio.rules"
    install -Dm644 "$_bundle/io.github.borjaburgos.CartridgeStudio.svg" "$pkgdir/usr/share/icons/hicolor/scalable/apps/io.github.borjaburgos.CartridgeStudio.svg"
}}

package_cartridge-studio-gui() {{
    replaces=('inl-cartridge-studio-gui')
    conflicts=('inl-cartridge-studio-gui')
    pkgdesc='Cartridge Studio — native graphical interface'
    depends=("cartridge-studio-core=$pkgver")
    local base="$pkgdir/opt/cartridge-studio"
    install -d "$base/bin" "$base/lib"
    cp -p --no-preserve=ownership "$_bundle/bin/cartridge-studio" "$base/bin/"
    cp -p --no-preserve=ownership "$_bundle/cartridge-studio" "$base/"
    for library in "$_bundle"/lib/*; do
        case "${{library##*/}}" in libgcc_s.so*) continue ;; esac
        cp -a --no-preserve=ownership "$library" "$base/lib/"
    done
    _launcher cartridge-studio
    install -Dm644 "$_bundle/io.github.borjaburgos.CartridgeStudio.desktop" "$pkgdir/usr/share/applications/io.github.borjaburgos.CartridgeStudio.desktop"
}}

package_cartridge-studio-tui() {{
    replaces=('inl-cartridge-studio-tui')
    conflicts=('inl-cartridge-studio-tui')
    pkgdesc='Cartridge Studio — interactive terminal interface'
    depends=("cartridge-studio-core=$pkgver")
    local base="$pkgdir/opt/cartridge-studio"
    install -d "$base/bin"
    cp -p --no-preserve=ownership "$_bundle/bin/cartridge-tui" "$base/bin/"
    cp -p --no-preserve=ownership "$_bundle/cartridge-tui" "$base/"
    _launcher cartridge-tui
    install -Dm644 "$_bundle/io.github.borjaburgos.CartridgeStudio.Terminal.desktop" "$pkgdir/usr/share/applications/io.github.borjaburgos.CartridgeStudio.Terminal.desktop"
}}

package_cartridge-studio-cli() {{
    replaces=('inl-cartridge-studio-cli')
    conflicts=('inl-cartridge-studio-cli')
    pkgdesc='Cartridge Studio — command line and scripting interface'
    depends=("cartridge-studio-core=$pkgver")
    local base="$pkgdir/opt/cartridge-studio"
    install -d "$base/bin"
    ln -s cartridge-worker "$base/bin/cartridge"
    cp -p --no-preserve=ownership "$_bundle/cartridge" "$base/"
    _launcher cartridge
}}
'''
    (output / 'PKGBUILD').write_text(recipe)
    products = []
    # makepkg writes both startdir and builddir into .BUILDINFO. Stage outside
    # the user's checkout so public packages contain no personal home path.
    # Only this temporary staging tree is removed; persistent products remain
    # under the caller's ignored build/dist directories.
    with tempfile.TemporaryDirectory(prefix='cartridge-studio-arch-', dir='/tmp') as temporary:
        stage = Path(temporary)
        (stage/'PKGBUILD').write_text(recipe)
        (stage/'bundle').symlink_to(bundle.resolve(), target_is_directory=True)
        environment = os.environ.copy()
        environment.update(BUILDDIR=str(stage), PKGDEST=str(stage), SRCDEST=str(stage),
            SRCPKGDEST=str(stage), LOGDEST=str(stage), PACKAGER='Unknown Packager')
        subprocess.run(['makepkg', '--config', '/etc/makepkg.conf', '--force', '--nodeps'],
            cwd=stage, env=environment, check=True)
        for name in names:
            product = stage / f'{name}-{package_version}-1-{arch}.pkg.tar.zst'
            shutil.copy2(product, output/product.name)
            target = dist / product.name
            public_package(product, target)
            products.append(target)
    return products
