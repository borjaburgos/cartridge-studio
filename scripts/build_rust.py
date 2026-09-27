#!/usr/bin/env python3
"""Build the complete native Rust application for the current platform."""
import argparse, os, shlex, subprocess
from pathlib import Path
from build_game_catalog import build as build_catalog
from build_helper import build as build_helper
from build_rust_notices import build as build_notices
ROOT = Path(__file__).resolve().parents[1]


def build_environment(release):
    environment = os.environ.copy()
    cargo_home = ROOT/'tmp/toolchains/cargo'
    environment['CARGO_HOME'] = str(cargo_home)
    if release:
        # Encoded flags preserve workspace paths containing spaces. Keep caller
        # flags while removing local checkout/registry paths from file!(), panic
        # locations and compiler debug information in distributable binaries.
        encoded = environment.get('CARGO_ENCODED_RUSTFLAGS')
        flags = encoded.split('\x1f') if encoded is not None else shlex.split(environment.get('RUSTFLAGS', ''))
        flags.extend([
            f'--remap-path-prefix={ROOT}=/cartridge-studio',
            f'--remap-path-prefix={cargo_home}=/cargo',
        ])
        environment['CARGO_ENCODED_RUSTFLAGS'] = '\x1f'.join(flags)
        environment.pop('RUSTFLAGS', None)
    return environment


def build(release=True):
    build_catalog()
    build_helper()
    environment = build_environment(release)
    subprocess.run(['cargo', 'build', '--locked', *(['--release'] if release else [])], cwd=ROOT, env=environment, check=True)
    build_notices()
    directory = ROOT/'tmp/rust-target'/('release' if release else 'debug')
    return directory
if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--debug', action='store_true')
    build(not parser.parse_args().debug)
