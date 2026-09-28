#!/usr/bin/env python3
"""Create a deterministic, non-commercial Game Boy fixture for release QA."""

import argparse
from pathlib import Path

LOGO = bytes([
    0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B, 0x03, 0x73, 0x00, 0x83,
    0x00, 0x0C, 0x00, 0x0D, 0x00, 0x08, 0x11, 0x1F, 0x88, 0x89, 0x00, 0x0E,
    0xDC, 0xCC, 0x6E, 0xE6, 0xDD, 0xDD, 0xD9, 0x99, 0xBB, 0xBB, 0x67, 0x63,
    0x6E, 0x0E, 0xEC, 0xCC, 0xDD, 0xDC, 0x99, 0x9F, 0xBB, 0xB9, 0x33, 0x3E,
])


def build() -> bytes:
    rom = bytearray(32 * 1024)
    rom[0x104:0x134] = LOGO
    rom[0x134:0x143] = b"CART STUDIO RC\0"
    rom[0x143] = 0x80  # Game Boy Color-compatible, not Color-only.
    rom[0x146] = 0x00
    rom[0x147] = 0x00  # ROM-only: this fixture cannot represent writable flash wiring.
    rom[0x148] = 0x00
    rom[0x149] = 0x00  # No save RAM; ROM backup and saved-game backup stay distinct.
    rom[0x14A] = 0x01
    rom[0x14C] = 0x00
    rom[0x14D] = sum((-byte - 1) for byte in rom[0x134:0x14D]) & 0xFF
    checksum = sum(rom) & 0xFFFF
    rom[0x14E:0x150] = checksum.to_bytes(2, "big")
    return bytes(rom)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", nargs="?", type=Path, default=Path("tmp/qa/synthetic-homebrew.gbc"))
    output = parser.parse_args().output
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(build())
    print(output.resolve())


if __name__ == "__main__":
    main()
