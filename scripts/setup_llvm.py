#!/usr/bin/env python3
"""Fetch the exact LLVM build used to qualify the on-device helper (development only)."""
import hashlib
from pathlib import Path
import shutil
import tarfile
import urllib.request
ROOT = Path(__file__).resolve().parents[1]
VERSION = '22.1.8'
URL = f'https://github.com/llvm/llvm-project/releases/download/llvmorg-{VERSION}/LLVM-{VERSION}-Linux-X64.tar.xz'
SHA256 = 'df0e1ecf16caf3489a272a5eea4eec9b0d82878f6477fa309504f918a0006384'

def main():
    directory = ROOT/'tmp/toolchains'
    output = directory/'llvm'
    if (output/'bin/clang').is_file():
        print(output/'bin'); return
    directory.mkdir(parents=True, exist_ok=True)
    archive = directory/f'LLVM-{VERSION}-Linux-X64.tar.xz'
    if not archive.is_file():
        temporary = archive.with_suffix('.download')
        print('Downloading the official LLVM 22.1.8 archive (about 1.9 GB). Only helper build tools are retained.', flush=True)
        with urllib.request.urlopen(URL, timeout=60) as response, temporary.open('wb') as dest:
            shutil.copyfileobj(response, dest, 1024*1024)
        temporary.replace(archive)
    with archive.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    if digest != SHA256:
        raise SystemExit(f'LLVM download checksum failed. Remove {archive} and retry; no downloaded program has been executed.')
    stage = directory/'llvm-extract'
    stage.mkdir(exist_ok=True)
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
