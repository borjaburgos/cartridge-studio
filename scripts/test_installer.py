#!/usr/bin/env python3
"""Offline installer integration checks; never open a window or cartridge."""
import itertools
import os
from pathlib import Path
import pty
import select
import shutil
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
PROGRAMS = {'gui': 'cartridge-studio', 'tui': 'cartridge-tui', 'cli': 'cartridge'}
APP_ID = 'io.github.borjaburgos.CartridgeStudio'


class InstallerTests(unittest.TestCase):
    def setUp(self):
        (ROOT / 'tmp').mkdir(exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(prefix='installer-tests-', dir=ROOT / 'tmp')
        self.root = Path(self.temp.name)
        self.bundle = self.root / 'archive'
        self.prefix = self.root / "installer's prefix $literal"
        self.library = self.root / 'existing library'
        self.library.mkdir()
        (self.library / 'backup.gb').write_bytes(b'preserve this existing backup')
        (self.library / 'settings.json').write_text('{"existing":true}')
        self.env = os.environ.copy()
        self.env.pop('XDG_DATA_HOME', None)
        self.env.update(CARTRIDGE_STUDIO_INSTALL_PREFIX=str(self.prefix), CARTRIDGE_STUDIO_DATA_DIR=str(self.library))
        real = os.environ.get('CARTRIDGE_STUDIO_TEST_INSTALL_BUNDLE')
        if real:
            self.bundle = Path(real).resolve()
        else:
            for folder in ('bin', 'lib', 'docs', 'licenses'):
                (self.bundle / folder).mkdir(parents=True)
            for program in [*PROGRAMS.values(), 'cartridge-worker']:
                for path in [self.bundle / program, self.bundle / 'bin' / program]:
                    path.write_text('#!/bin/sh\nprintf "test version\\n"\n')
                    path.chmod(0o755)
            for name in ('libgcc_s.so.1', 'libwayland-client.so.0'):
                (self.bundle / 'lib' / name).write_text('test library')
            (self.bundle / 'lib/libwayland-client.so').symlink_to('libwayland-client.so.0')
            for name in ('README.md', 'runtime-requirements.json', APP_ID + '.svg'):
                (self.bundle / name).write_text('test fixture')
            (self.bundle / 'VERSION').write_text('0.8.2\n')
            for name in ('70-cartridge-studio.rules', APP_ID + '.desktop', APP_ID + '.Terminal.desktop'):
                shutil.copy2(ROOT / 'desktop' / name, self.bundle / name)
            shutil.copy2(ROOT / 'scripts/install_native.sh', self.bundle / 'install.sh')

    def tearDown(self):
        self.temp.cleanup()

    def run_installer(self, *args, success=True):
        result = subprocess.run(['sh', str(self.bundle / 'install.sh'), *args], env=self.env,
                                input='', text=True, capture_output=True, timeout=30)
        if success:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0)
        return result

    def assert_selection(self, selection):
        expected = ','.join(c for c in PROGRAMS if c in selection)
        app = self.prefix / 'share/cartridge-studio'
        self.assertEqual((app / 'components').read_text().strip(), expected)
        release = next(p for p in (app / 'releases').iterdir()
                       if (p / 'components').read_text().strip() == expected)
        for component, name in PROGRAMS.items():
            self.assertEqual((self.prefix / 'bin' / name).exists(), component in selection)
            self.assertEqual((release / 'bin' / name).exists(), component in selection)
            if component in selection:
                result = subprocess.run([str(self.prefix / 'bin' / name), '--version'],
                                        text=True, capture_output=True, timeout=10)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertNotEqual(result.stdout.strip(), '')
        self.assertTrue((release / 'bin/cartridge-worker').is_file())
        self.assertFalse((self.prefix / 'bin/cartridge-worker').exists())
        if 'cli' in selection:
            self.assertEqual((release / 'bin/cartridge').stat().st_ino, (release / 'bin/cartridge-worker').stat().st_ino)
        if 'gui' not in selection:
            self.assertEqual([p.name for p in (release / 'lib').iterdir()], ['libgcc_s.so.1'])
        for component, suffix in [('gui', ''), ('tui', '.Terminal')]:
            entry = self.prefix / 'share/applications' / (APP_ID + suffix + '.desktop')
            self.assertEqual(entry.exists(), component in selection)
            if entry.exists():
                command = next(line for line in entry.read_text().splitlines() if line.startswith('Exec='))
                self.assertIn(PROGRAMS[component], command)
                self.assertNotIn('--tui', command)
        self.assertEqual((self.library / 'backup.gb').read_bytes(), b'preserve this existing backup')
        self.assertEqual((self.library / 'settings.json').read_text(), '{"existing":true}')

    def test_rebrand_removes_only_managed_legacy_launchers_and_preserves_library(self):
        old = self.prefix / 'share/inl-cartridge-studio'
        (old / 'releases/0.8.3').mkdir(parents=True)
        (old / 'releases/0.8.3/keep.txt').write_text('prior release')
        (old / 'components').write_text('tui,cli\n')
        (self.prefix / 'bin').mkdir()
        for name in ('inl-cartridge-studio', 'inl-tui', 'inl-worker'):
            (self.prefix / 'bin' / name).write_text('#!/bin/sh\n# Managed by INL Cartridge Studio installer.\n')
        (self.prefix / 'bin/inl').write_text('unrelated user file')
        menus = self.prefix / 'share/applications'
        menus.mkdir()
        managed = menus / 'io.github.borjaburgos.INLCartridgeStudio.desktop'
        managed.write_text('[Desktop Entry]\nX-INL-Managed=true\n')
        unrelated = menus / 'io.github.borjaburgos.INLCartridgeStudio.Terminal.desktop'
        unrelated.write_text('unrelated user menu')
        self.run_installer('--all')
        self.assert_selection(['gui', 'tui', 'cli'])
        self.assertFalse((self.prefix / 'bin/inl-tui').exists())
        self.assertFalse(managed.exists())
        self.assertEqual(unrelated.read_text(), 'unrelated user menu')
        self.assertEqual((self.prefix / 'bin/inl').read_text(), 'unrelated user file')
        self.assertEqual((old / 'releases/0.8.3/keep.txt').read_text(), 'prior release')

    def test_all_seven_selections_and_reconfiguration(self):
        for count in (3, 2, 1):
            for selection in itertools.combinations(PROGRAMS, count):
                with self.subTest(selection=selection):
                    self.run_installer('--components', ','.join(selection))
                    self.assert_selection(selection)
        self.assertEqual(len(list((self.prefix / 'share/cartridge-studio/releases').iterdir())), 7)
        self.run_installer('--all')
        self.assert_selection(PROGRAMS)

    def test_invalid_and_unattended_choices_change_nothing(self):
        for args in [[], ['--components'], ['--components', ''], ['--components', ','],
                     ['--components', 'gui,'], ['--components', 'gui,,tui'],
                     ['--components', 'not-an-interface'], ['--all', '--components', 'tui']]:
            with self.subTest(args=args):
                result = self.run_installer(*args, success=False)
                self.assertTrue(result.stderr.strip())
                self.assertFalse(self.prefix.exists())
        self.assertIn('GUI', self.run_installer('--help').stdout)
        self.assertFalse(self.prefix.exists())

    def test_names_numbers_and_duplicates(self):
        self.run_installer('--components', ' TUI, 2, CLI, 3 ')
        self.assert_selection(['tui', 'cli'])

    def test_unrelated_launchers_are_preserved(self):
        folder = self.prefix / 'bin'
        folder.mkdir(parents=True)
        other = folder / 'cartridge'
        other.write_text('unrelated user command')
        self.run_installer('--components', 'cli', success=False)
        self.assertEqual(other.read_text(), 'unrelated user command')
        self.run_installer('--components', 'tui')
        self.assertEqual(other.read_text(), 'unrelated user command')

    def test_interactive_choice_and_saved_default(self):
        def interact(answer, expected):
            master, slave = pty.openpty()
            process = subprocess.Popen(['sh', str(self.bundle / 'install.sh')], env=self.env,
                                       stdin=slave, stdout=slave, stderr=slave)
            os.close(slave)
            data = bytearray()
            try:
                deadline = time.monotonic() + 30
                sent = False
                while time.monotonic() < deadline:
                    if select.select([master], [], [], .1)[0]:
                        try:
                            data.extend(os.read(master, 65536))
                        except OSError:
                            break
                    if not sent and b'Install [' in data:
                        self.assertIn(expected.encode(), data)
                        os.write(master, answer.encode() + b'\n')
                        sent = True
                    if process.poll() is not None:
                        break
                self.assertEqual(process.wait(timeout=5), 0, data.decode())
            finally:
                if process.poll() is None:
                    process.terminate(); process.wait(timeout=5)
                os.close(master)
        interact('2', 'Install [gui,tui,cli]')
        self.assert_selection(['tui'])
        interact('', 'Install [tui]')
        self.assert_selection(['tui'])


if __name__ == '__main__':
    unittest.main(verbosity=2)
