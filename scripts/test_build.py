#!/usr/bin/env python3
"""Offline release-preflight checks; never package files or access hardware."""
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import build_desktop
import build_rust
import setup_llvm
from build_arch import arch_version, build_arch


class ReleaseVersionTests(unittest.TestCase):
    def test_stable_version_is_unchanged(self):
        self.assertEqual(arch_version('0.13.0'), '0.13.0')

    def test_beta_version_is_safe_for_arch_filenames(self):
        self.assertEqual(arch_version('0.1.0-beta.1'), '0.1.0beta1')
        self.assertEqual(arch_version('0.1.0-rc.2'), '0.1.0rc2')
        self.assertEqual(arch_version('0.1.0-alpha.0'), '0.1.0alpha0')

    def test_unrecognized_prerelease_fails_explicitly(self):
        with self.assertRaisesRegex(ValueError, 'Unsupported Arch release version'):
            arch_version('0.1.0-preview.some-build')

    @unittest.skipUnless(shutil.which('vercmp'), 'Arch version comparator is not installed')
    def test_pacman_orders_beta_before_next_beta_and_final(self):
        beta = arch_version('0.1.0-beta.1')
        for later in (arch_version('0.1.0-beta.2'), arch_version('0.1.0')):
            self.assertEqual(subprocess.check_output(['vercmp', beta, later], text=True).strip(), '-1')


class HelperToolchainTests(unittest.TestCase):
    def test_qualified_macos_arm64_asset_is_pinned(self):
        archive, url, digest = setup_llvm.release_asset('Darwin', 'arm64')
        self.assertEqual(archive, 'LLVM-22.1.8-macOS-ARM64.tar.xz')
        self.assertTrue(url.endswith('/' + archive))
        self.assertEqual(digest, 'f260f4f7c0d430828a81ae8a3826a1d63fc0963ec2459489308cc23b1f7eab4f')

    def test_unqualified_helper_host_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError, 'No qualified LLVM 22.1.8'):
            setup_llvm.release_asset('Darwin', 'x86_64')


class ReleasePreflightTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='cartridge-release-test-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.binaries = self.root/'bin'
        self.binaries.mkdir()
        for name in build_desktop.PROGRAMS:
            if name != 'cartridge-worker':
                executable = self.binaries/name
                executable.write_text('#!/bin/sh\nprintf "Cartridge Studio 1.2.3 · Rust\\n"\n')
                executable.chmod(0o755)
        files = ['LICENSE', 'tmp/rust-licenses/rust-components.json',
            'tmp/game-catalog/NOTICE.txt', 'tmp/game-catalog/LIBRETRO-DATABASE-LICENSE.txt',
            *('docs/'+name for name in build_desktop.DOCUMENTS)]
        for name in files:
            target = self.root/name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text('release fixture\n')
        self.root_patch = patch.object(build_desktop, 'ROOT', self.root)
        self.version_patch = patch.object(build_desktop, 'VERSION', '1.2.3')
        self.root_patch.start()
        self.version_patch.start()
        self.addCleanup(self.root_patch.stop)
        self.addCleanup(self.version_patch.stop)

    def test_complete_current_release_is_accepted(self):
        build_desktop.validate_release_inputs(self.binaries)

    def test_stale_binary_is_rejected_before_packaging(self):
        (self.binaries/'cartridge-tui').write_text('#!/bin/sh\nprintf "Cartridge Studio 1.2.2\\n"\n')
        with self.assertRaisesRegex(SystemExit, 'version mismatch.*cartridge-tui'):
            build_desktop.validate_release_inputs(self.binaries)

    def test_version_must_match_a_complete_token(self):
        (self.binaries/'cartridge').write_text('#!/bin/sh\nprintf "cartridge 1.2.30\\n"\n')
        with self.assertRaisesRegex(SystemExit, 'version mismatch'):
            build_desktop.validate_release_inputs(self.binaries)

    def test_missing_license_is_rejected(self):
        (self.root/'LICENSE').unlink()
        with self.assertRaisesRegex(SystemExit, 'Missing release input:.*LICENSE'):
            build_desktop.validate_release_inputs(self.binaries)

    def test_missing_binary_has_build_guidance(self):
        (self.binaries/'cartridge').unlink()
        with self.assertRaisesRegex(SystemExit, 'Missing release executable:.*build_rust.py'):
            build_desktop.validate_release_inputs(self.binaries)

    def test_non_arch_host_has_actionable_guidance(self):
        with patch.object(build_desktop.platform, 'system', return_value='Linux'), \
                patch.object(build_desktop.shutil, 'which', return_value=None):
            with self.assertRaisesRegex(SystemExit, 'Arch Linux build host.*pacman'):
                build_desktop.validate_build_host()


class ReleasePrivacyTests(unittest.TestCase):
    def test_portable_archive_uses_neutral_ownership(self):
        member = tarfile.TarInfo('cartridge-studio/bin/cartridge')
        member.uid, member.gid = 1234, 5678
        member.uname, member.gname = 'private-builder', 'private-group'
        member = build_desktop.public_tar_member(member)
        self.assertEqual((member.uid, member.gid, member.uname, member.gname), (0, 0, 'root', 'root'))

    def test_split_system_package_uses_its_source_base(self):
        with tempfile.TemporaryDirectory(prefix='cartridge-provenance-test-') as temporary:
            database = Path(temporary)
            package = database/'libgcc-16.2.1+abc-1'
            package.mkdir()
            (package/'desc').write_text('%NAME%\nlibgcc\n\n%VERSION%\n16.2.1+abc-1\n\n'
                '%BASE%\ngcc\n\n%ARCH%\nx86_64\n\n%URL%\nhttps://gcc.gnu.org\n\n'
                '%LICENSE%\nGPL-3.0-or-later WITH GCC-exception-3.1\n\n')
            with patch.object(build_desktop.subprocess, 'check_output', return_value='libgcc 16.2.1+abc-1\n'):
                record = build_desktop.system_package_record('libgcc', database)
            self.assertEqual(record['source_base'], 'gcc')
            self.assertTrue(record['packaging_repository'].endswith('/gcc'))
            self.assertIn('16.2.1%2Babc-1', record['binary_archive_url'])
            self.assertEqual(record['upstream_url'], 'https://gcc.gnu.org')
            self.assertEqual(record['licenses'], ['GPL-3.0-or-later WITH GCC-exception-3.1'])

    def test_remapping_preserves_spaces_and_existing_flags(self):
        root = Path('/home/private person/My Projects/Cartridge Studio')
        with patch.object(build_rust, 'ROOT', root), \
                patch.dict(build_rust.os.environ, {'RUSTFLAGS': '-C target-cpu=x86-64'}, clear=True):
            environment = build_rust.build_environment(True)
        flags = environment['CARGO_ENCODED_RUSTFLAGS'].split('\x1f')
        self.assertEqual(flags[:2], ['-C', 'target-cpu=x86-64'])
        self.assertIn(f'--remap-path-prefix={root}=/cartridge-studio', flags)
        self.assertIn(f'--remap-path-prefix={root}/tmp/toolchains/cargo=/cargo', flags)
        self.assertNotIn('RUSTFLAGS', environment)

    def test_existing_encoded_flags_take_precedence(self):
        with patch.dict(build_rust.os.environ,
                {'CARGO_ENCODED_RUSTFLAGS': '-C\x1flink-arg=with spaces', 'RUSTFLAGS': 'ignored'}, clear=True):
            environment = build_rust.build_environment(True)
        self.assertEqual(environment['CARGO_ENCODED_RUSTFLAGS'].split('\x1f')[:2],
            ['-C', 'link-arg=with spaces'])

    def test_debug_build_keeps_local_diagnostic_paths(self):
        with patch.dict(build_rust.os.environ, {'RUSTFLAGS': '-C debuginfo=2'}, clear=True):
            environment = build_rust.build_environment(False)
        self.assertEqual(environment['RUSTFLAGS'], '-C debuginfo=2')
        self.assertNotIn('CARGO_ENCODED_RUSTFLAGS', environment)

    def test_arch_staging_is_neutral_and_products_survive_cleanup(self):
        with tempfile.TemporaryDirectory(prefix='cartridge-package-test-') as temporary:
            root = Path(temporary)
            bundle, output, dist = (root/name for name in ('private source', 'output', 'dist'))
            bundle.mkdir()
            dist.mkdir()
            stages = []

            def fake_makepkg(command, *, cwd, env, check):
                stages.append(cwd)
                self.assertEqual(cwd.parent.resolve(), Path('/tmp').resolve())
                self.assertEqual(env['BUILDDIR'], str(cwd))
                self.assertEqual(env['PKGDEST'], str(cwd))
                self.assertEqual(env['PACKAGER'], 'Unknown Packager')
                self.assertNotIn(str(bundle), (cwd/'PKGBUILD').read_text())
                # macOS reports the same temporary volume as both /var and
                # /private/var; compare canonical paths rather than spellings.
                self.assertEqual((cwd/'bundle').resolve(), bundle.resolve())
                for suffix in ('', '-core', '-gui', '-tui', '-cli'):
                    (cwd/f'cartridge-studio{suffix}-1.2.3-1-x86_64.pkg.tar.zst').write_bytes(b'package')

            with patch('build_arch.subprocess.run', side_effect=fake_makepkg), \
                    patch('build_arch.public_package', side_effect=lambda source, target: target.write_bytes(source.read_bytes())):
                products = build_arch(bundle, output, dist, '1.2.3', 'x86_64', '2.39')
            self.assertEqual(len(products), 5)
            self.assertTrue(all(path.read_bytes() == b'package' for path in products))
            self.assertTrue(all((output/path.name).is_file() for path in products))
            self.assertFalse(stages[0].exists())


if __name__ == '__main__':
    unittest.main(verbosity=2)
