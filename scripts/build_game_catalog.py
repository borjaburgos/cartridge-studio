#!/usr/bin/env python3
"""Build our offline SQLite index from pinned, checksum-verified Libretro RDBs.

Only public metadata is downloaded. Generated inputs and outputs stay in tmp/.
The small MessagePack reader is build-time code; installed apps use SQLite only.
"""
from contextlib import closing
import hashlib
import json
from pathlib import Path
import sqlite3
import struct
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
COMMIT = '92a7c5adf8c8362d7b88a35042f0211ae3d88316'
FILES = {
    'Nintendo - Game Boy Advance': ('gba', 'b28249739d43572cb741fe011daba524c91c4719d2500505337412453ea1e2d9'),
    'Nintendo - Game Boy': ('gb', 'eb2dfd45f1945503723d9a1f6ead1a4a2cce9450f548fd458ec79da6b7d40098'),
    'Nintendo - Game Boy Color': ('gbc', 'aef888050d0186ce5952d1c89e5e83d8e1760f885c7f99796b35489967e76aea'),
    'Nintendo - Nintendo Entertainment System': ('nes', 'bec747484a932486feca5d66c2a46f252e41dd1961b026cb5042c1876704de83'),
}
LICENSE_HASH = '23ee78c8bae49cf08ea2f0c84945c66b987ebe4520881fb51b3dad4fb43d07c2'


class MessagePack:
    def __init__(self, data):
        self.data, self.pos = data, 16

    def take(self, n):
        if n < 0 or self.pos + n > len(self.data):
            raise ValueError('Truncated RDB')
        value = self.data[self.pos:self.pos+n]
        self.pos += n
        return value

    def number(self, n):
        return int.from_bytes(self.take(n), 'big')

    def read(self, depth=0):
        if depth > 16:
            raise ValueError('Excessively nested RDB')
        tag = self.number(1)
        if tag < 0x80:
            return tag
        if tag >= 0xe0:
            return tag - 256
        if tag == 0xc0:
            return None
        if tag in (0xc2, 0xc3):
            return tag == 0xc3
        if 0xcc <= tag <= 0xcf:
            return self.number(1 << (tag - 0xcc))
        if 0xd0 <= tag <= 0xd3:
            return int.from_bytes(self.take(1 << (tag - 0xd0)), 'big', signed=True)
        if tag in (0xca, 0xcb):
            return struct.unpack('>f' if tag == 0xca else '>d', self.take(4 if tag == 0xca else 8))[0]
        if 0xa0 <= tag <= 0xbf or tag in (0xd9, 0xda, 0xdb, 0xc4, 0xc5, 0xc6):
            binary = tag in (0xc4, 0xc5, 0xc6)
            n = tag & 31 if 0xa0 <= tag <= 0xbf else self.number(1 << (tag - (0xc4 if binary else 0xd9)))
            value = self.take(n)
            return value if binary else value.decode('utf-8')
        if 0x80 <= tag <= 0x9f or tag in (0xdc, 0xdd, 0xde, 0xdf):
            mapping = tag < 0x90 or tag in (0xde, 0xdf)
            n = tag & 15 if tag < 0xa0 else self.number(2 if tag % 2 == 0 else 4)
            if n > 10000:
                raise ValueError('Excessive RDB container size')
            if mapping:
                return {self.read(depth+1): self.read(depth+1) for _ in range(n)}
            return [self.read(depth+1) for _ in range(n)]
        raise ValueError(f'Unsupported MessagePack tag {tag:#x}')


def records(path):
    data = path.read_bytes()
    if data[:8] != b'RARCHDB\0':
        raise ValueError('Invalid RDB signature')
    offset = int.from_bytes(data[8:16], 'big')
    reader = MessagePack(data)
    result = []
    while reader.pos < offset:
        row = reader.read()
        if row is None:
            break
        if not isinstance(row, dict):
            raise ValueError('Invalid RDB record')
        result.append(row)
    if reader.pos != offset or reader.read()['count'] != len(result):
        raise ValueError('RDB record count/offset mismatch')
    return result


def fetch(name, expected, target):
    if not target.exists():
        url = f'https://raw.githubusercontent.com/libretro/libretro-database/{COMMIT}/{urllib.parse.quote(name)}'
        with urllib.request.urlopen(url, timeout=30) as response:
            data = response.read(16 * 1024 * 1024 + 1)
        if hashlib.sha256(data).hexdigest() != expected:
            raise ValueError(f'Catalog checksum mismatch: {name}')
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    if hashlib.sha256(target.read_bytes()).hexdigest() != expected:
        raise ValueError(f'Cached catalog checksum mismatch: {target}; remove it and rebuild.')


def build():
    directory = ROOT/'tmp/game-catalog'
    directory.mkdir(parents=True, exist_ok=True)
    output = directory/'games.sqlite3'
    fetch('LICENSE', LICENSE_HASH, directory/'upstream/LICENSE')
    all_rows = []
    for system, (platform, digest) in FILES.items():
        path = directory/'upstream'/f'{system}.rdb'
        fetch(f'rdb/{system}.rdb', digest, path)
        all_rows.extend((platform, system, row) for row in records(path))
    temporary = output.with_suffix('.tmp')
    temporary.unlink(missing_ok=True)
    with closing(sqlite3.connect(temporary)) as db, db:
        db.executescript('''
            PRAGMA user_version=1;
            CREATE TABLE catalog (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE games (id TEXT PRIMARY KEY, platform TEXT NOT NULL, name TEXT NOT NULL,
                size INTEGER NOT NULL, sha1 TEXT NOT NULL, metadata TEXT NOT NULL);
            CREATE INDEX game_hash ON games(platform, size, sha1);
            CREATE INDEX game_name ON games(name COLLATE NOCASE);
        ''')
        db.executemany('INSERT INTO catalog VALUES (?,?)', [
            ('commit', COMMIT), ('provider', 'Libretro database'), ('license', 'CC BY-SA 4.0'),
            ('url', 'https://github.com/libretro/libretro-database'),
        ])
        for platform, system, row in all_rows:
            sha1 = row.get('sha1', b'')
            if len(sha1) != 20 or not row.get('size'):
                continue
            info = {key: value.hex() if isinstance(value, bytes) else value for key, value in row.items()}
            info.update(system=system, platform=platform)
            identity = hashlib.sha256(f'{platform}\0{row["name"]}\0{sha1.hex()}'.encode()).hexdigest()
            db.execute('INSERT OR IGNORE INTO games VALUES (?,?,?,?,?,?)',
                       (identity, platform, row['name'], row['size'], sha1.hex(), json.dumps(info, ensure_ascii=False)))
        count = db.execute('SELECT count(*) FROM games').fetchone()[0]
    temporary.replace(output)
    (directory/'LIBRETRO-DATABASE-LICENSE.txt').write_bytes((directory/'upstream/LICENSE').read_bytes())
    (directory/'NOTICE.txt').write_text(
        'Game metadata: Libretro database and its credited upstream contributors, including No-Intro.\n'
        f'https://github.com/libretro/libretro-database/tree/{COMMIT}\n'
        'Licensed under Creative Commons Attribution-ShareAlike 4.0 International.\n'
        'See LIBRETRO-DATABASE-LICENSE.txt. Converted from RDB to SQLite and limited to GB, GBC, GBA and NES.\n'
        'The database license does not license the games or separately downloaded artwork.\n')
    print(f'Built offline game catalog: {count:,} records, {output.stat().st_size:,} bytes')
    return directory


if __name__ == '__main__':
    build()
