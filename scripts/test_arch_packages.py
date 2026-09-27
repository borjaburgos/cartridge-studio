#!/usr/bin/env python3
"""Qualify built split packages without installing system files or opening USB."""
import hashlib
import gzip
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import tomllib
from build_arch import arch_version

ROOT = Path(__file__).resolve().parents[1]
VERSION = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']
DIST = ROOT / 'tmp/dist'


def main():
    package_version = arch_version(VERSION)
    suffix = f'-{package_version}-1-{platform.machine()}.pkg.tar.zst'
    packages = {c: DIST / ('cartridge-studio' + ('-' + c if c else '') + suffix)
                for c in ('', 'core', 'gui', 'tui', 'cli')}
    owned = {}
    for component, package in packages.items():
        listing = subprocess.check_output(['bsdtar', '-tf', str(package)], text=True)
        info = subprocess.check_output(['bsdtar', '-xOf', str(package), '.PKGINFO'], text=True)
        assert f'pkgver = {package_version}-1' in info.splitlines(), info
        assert '.BUILDINFO' not in listing.splitlines(), 'Public package exposes private build-host inventory'
        manifest = gzip.decompress(subprocess.check_output(['bsdtar', '-xOf', str(package), '.MTREE'])).decode()
        assert './.BUILDINFO ' not in manifest, 'File manifest references omitted private inventory'
        subprocess.run(['pacman', '-Qip', str(package)], check=True, stdout=subprocess.DEVNULL)
        for file in listing.splitlines():
            if file.endswith('/') or file.startswith('.'):
                continue
            assert file not in owned, f'Package ownership collision: {file}'
            owned[file] = component
        dependencies = [line for line in info.splitlines() if line.startswith('depend = ')]
        if component in ('gui', 'tui', 'cli'):
            assert dependencies == [f'depend = cartridge-studio-core={package_version}'], dependencies
        if component == '':
            assert set(dependencies) == {f'depend = cartridge-studio-{c}={package_version}'
                for c in ('gui', 'tui', 'cli')}, dependencies
    for line in (DIST / 'SHA256SUMS').read_text().splitlines():
        expected, name = line.split(maxsplit=1)
        assert hashlib.sha256((DIST / name).read_bytes()).hexdigest() == expected, name
    checks = []
    for component, name in [('gui', 'cartridge-studio'), ('tui', 'cartridge-tui'), ('cli', 'cartridge')]:
        with tempfile.TemporaryDirectory(prefix='arch-component-', dir=ROOT / 'tmp') as temporary:
            folder = Path(temporary)
            for key in ('core', component):
                subprocess.run(['bsdtar', '-xf', str(packages[key]), '--no-same-owner', '-C', str(folder)], check=True)
            base = folder / 'opt/cartridge-studio'
            env = os.environ.copy()
            env['LD_LIBRARY_PATH'] = str(base / 'lib')
            env.pop('DISPLAY', None); env.pop('WAYLAND_DISPLAY', None)
            version = subprocess.check_output([str(base / 'bin' / name), '--version'], text=True, env=env)
            assert VERSION in version, version
            result = subprocess.run([str(base / 'bin/cartridge-worker')], input='{"action":"profiles"}\n',
                                    text=True, capture_output=True, check=True, env=env)
            assert json.loads(result.stdout)['result']['version'] == VERSION
            if component != 'gui':
                assert [p.name for p in (base / 'lib').iterdir()] == ['libgcc_s.so.1']
                assert not (base / 'bin/cartridge-studio').exists()
            if component in ('gui', 'cli'):
                args = ['--tui', '--version'] if component == 'gui' else ['tui', '--version']
                missing = subprocess.run([str(base / 'bin' / name), *args], env=env, text=True, capture_output=True)
                assert missing.returncode != 0
                assert 'TUI' in missing.stderr and 'installer' in missing.stderr, missing.stderr
            if component == 'gui':
                missing = subprocess.run([str(base / 'bin' / name), '--cli', '--version'], env=env, text=True, capture_output=True)
                assert missing.returncode != 0 and 'CLI' in missing.stderr and 'installer' in missing.stderr
            checks.append({'component': component, 'launch_version': version.strip(), 'shared_worker': True})
    report = {'version': VERSION, 'arch_package_version': package_version, 'package_file_ownership': 'no overlaps', 'dependency_selection': True,
              'distribution_checksums': True, 'isolated_components': checks, 'hardware_access': False}
    output = ROOT / 'tmp/installer-arch-validation.json'
    output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
