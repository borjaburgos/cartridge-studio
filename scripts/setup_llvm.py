#!/usr/bin/env python3
"""Fetch the exact LLVM build used to qualify the on-device helper (development only)."""
import hashlib
import platform
from pathlib import Path
import shutil
import tarfile
import urllib.request
ROOT = Path(__file__).resolve().parents[1]
VERSION = '22.1.8'
ASSETS = {
    ('Linux', 'x86_64'): (
        f'LLVM-{VERSION}-Linux-X64.tar.xz',
        'df0e1ecf16caf3489a272a5eea4eec9b0d82878f6477fa309504f918a0006384',
    ),
    ('Linux', 'aarch64'): (
        f'LLVM-{VERSION}-Linux-ARM64.tar.xz',
        '805efad2bb91cb4967fa569e0881d10c0f69c04461cf671cccbae19f547acc34',
    ),
    ('Darwin', 'arm64'): (
        f'LLVM-{VERSION}-macOS-ARM64.tar.xz',
        'f260f4f7c0d430828a81ae8a3826a1d63fc0963ec2459489308cc23b1f7eab4f',
    ),
}


def release_asset(system=None, machine=None):
    key = (system or platform.system(), machine or platform.machine())
    try:
        archive, digest = ASSETS[key]
    except KeyError as error:
        supported = ', '.join(f'{system}/{machine}' for system, machine in ASSETS)
        raise RuntimeError(
            f'No qualified LLVM {VERSION} helper toolchain for {key[0]}/{key[1]}. '
            f'Supported build hosts: {supported}.'
        ) from error
    url = f'https://github.com/llvm/llvm-project/releases/download/llvmorg-{VERSION}/{archive}'
    return archive, url, digest

def main():
    archive_name, url, expected = release_asset()
    directory = ROOT/'tmp/toolchains'
    output = directory/'llvm'
    if (output/'bin/clang').is_file():
        print(output/'bin'); return
    directory.mkdir(parents=True, exist_ok=True)
    archive = directory/archive_name
    if not archive.is_file():
        temporary = archive.with_suffix('.download')
        print(f'Downloading the official LLVM {VERSION} archive. Only helper build tools are retained.', flush=True)
        with urllib.request.urlopen(url, timeout=60) as response, temporary.open('wb') as dest:
            shutil.copyfileobj(response, dest, 1024*1024)
        temporary.replace(archive)
    with archive.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    if digest != expected:
        raise SystemExit(f'LLVM download checksum failed. Remove {archive} and retry; no downloaded program has been executed.')
    stage = directory/'llvm-extract'
    # A cancelled extraction must never be mistaken for a complete qualified
    # toolchain on the next run.
    if stage.exists():
        shutil.rmtree(stage)
    stage.mkdir()
    wanted = {'clang','clang-22','lld','ld.lld','llvm-objcopy'}
    with tarfile.open(archive, 'r|xz') as source:
        for member in source:
            relative = member.name.partition('/')[2]
            if (relative.startswith('bin/') and Path(relative).name in wanted
                    or relative.startswith(('lib/libLLVM', 'lib/libclang-cpp', 'lib/clang/22/include/'))):
                source.extract(member, stage, filter='data')
    extracted = next(p for p in stage.iterdir() if (p/'bin/clang').is_file())
    if output.exists(): raise SystemExit(f'{output} already exists. Inspect it before replacing this development toolchain.')
    extracted.rename(output)
    archive.unlink()
    print(output/'bin')
if __name__ == '__main__': main()
