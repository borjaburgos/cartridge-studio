"""Development-only compiler/relocation proof for the unchanged on-device helper."""
import hashlib, json, os, re, shutil, struct, subprocess
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
DATA_DIR = ROOT/'tmp'
CAPACITY = 524288
SOURCES = [ROOT/'embedded/gb-helper'/('inlretro_flash_helper'+suffix) for suffix in ('.S','.c','.ld')]
def sha(data): return hashlib.sha256(data).hexdigest()
def relocate_helper(code, manifest, base):
    if base % 4 or not 0x20000000 <= base <= 0x20001600:
        raise ValueError('Unsafe programmer helper address.')
    if (sha(code) != manifest['sha256'] or len(code) != 384
            or code[:2] != b'\x5f\xa0'
            or manifest['source_sha256'] != sha(b''.join(p.read_bytes() for p in SOURCES))):
        raise ValueError('Bundled programmer helper failed integrity checks. Reinstall the application; cartridge not erased.')
    relocated = bytearray(code)
    for offset in manifest['relocations']:
        value = struct.unpack_from('<I', code, offset)[0]
        if offset % 4 or not manifest['link_base'] <= value < manifest['link_base'] + 384:
            raise ValueError('Invalid bundled helper relocation; cartridge not erased.')
        struct.pack_into('<I', relocated, offset, value + base - manifest['link_base'])
    return bytes(relocated)


def compile_helper(base):
    # The packaged helper carries validated absolute-code-pointer relocations.
    # Its relocated bytes are compared with independently linked ELF builds.
    compiler = os.environ.get('CARTRIDGE_STUDIO_CLANG', 'clang')
    objcopy = os.environ.get('CARTRIDGE_STUDIO_OBJCOPY', 'llvm-objcopy')
    if not shutil.which(compiler) or not shutil.which(objcopy):
        raise RuntimeError('Building the embedded helper requires clang with its ARM/LLD backend and llvm-objcopy.')
    version = subprocess.check_output([compiler, '--version'], text=True)
    if not re.search(r'clang version 22\.1\.8(?:\s|$)', version):
        raise RuntimeError('The qualified helper requires LLVM 22.1.8. On a supported Linux or Apple Silicon macOS build host, run python3 scripts/setup_llvm.py and add tmp/toolchains/llvm/bin to PATH. Do not change the helper integrity manifest to bypass this check.')
    digest = sha(b''.join(path.read_bytes() for path in SOURCES))[:16]
    directory = DATA_DIR/'toolchains/inlretro-writer'/f'{base:08x}-{digest}'
    directory.mkdir(parents=True, exist_ok=True)
    elf, binary = directory/'helper.elf', directory/'helper.bin'
    command = [compiler, '--target=arm-none-eabi', '-mcpu=cortex-m0', '-mthumb',
                    '-Oz', '-ffreestanding', '-fno-builtin', '-fomit-frame-pointer',
                    '-fno-unwind-tables', '-fno-asynchronous-unwind-tables', '-nostdlib',
                    '-fuse-ld=lld', f'-Wl,--defsym,HELPER_BASE={base}', f'-Wl,-T,{SOURCES[2]}',
                    str(SOURCES[0]), str(SOURCES[1]), '-o', str(elf)]
    try:
        subprocess.run(command, check=True)
        subprocess.run([objcopy, '-O', 'binary', str(elf), str(binary)], check=True)
    except subprocess.CalledProcessError as error:
        raise RuntimeError('Could not compile the temporary RAM helper; cartridge not erased.') from error
    code = binary.read_bytes()
    if not 4 <= len(code) <= 384 or code[:2] != b'\x5f\xa0':
        raise ValueError('Compiled RAM helper does not fit its allocated buffers.')
    return code.ljust(384, b'\0')

def build():
    bases = [0x20000000,0x200004f8,0x20000500,0x20001600]
    codes = [compile_helper(base) for base in bases]
    manifest = json.loads((ROOT/'rust/cartridge-core/assets/gb-helper.json').read_text())
    for base, code in zip(bases,codes):
        if relocate_helper(codes[0],manifest,base) != code:
            raise RuntimeError('Helper relocation differs from the independently linked build.')
    directory=ROOT/'tmp/studio-build'
    directory.mkdir(parents=True,exist_ok=True)
    (directory/'helper.bin').write_bytes(codes[0])
    (directory/'helper.json').write_text(json.dumps(manifest,indent=2))
    return directory
if __name__ == '__main__': build()
