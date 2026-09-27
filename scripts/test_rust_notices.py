#!/usr/bin/env python3
"""Offline regression checks for the release license gate; no hardware access."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import build_rust_notices as notices


class LicenseNotices(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.output = self.root / 'output'
        self.output.mkdir()

    def package(self, name='example', version='1.0.0', **extra):
        source = self.root / f'{name}-{version}'
        source.mkdir()
        (source / 'Cargo.toml').write_text('[package]\nname="' + name + '"\nlicense="MIT"\n')
        return {'name': name, 'version': version, 'license': 'MIT',
                'source': 'registry+https://github.com/rust-lang/crates.io-index',
                'manifest_path': str(source / 'Cargo.toml'), 'repository': None,
                **extra}

    def test_lowercase_notice_is_preserved_with_provenance(self):
        package = self.package()
        source = Path(package['manifest_path']).parent
        data = b'Test fixture notice.\n'
        (source / 'license-mit').write_bytes(data)
        records = notices.collect([package], {'example-1.0.0'}, self.output, offline=True)
        self.assertEqual(records[0]['notice_status'], 'complete')
        self.assertEqual(records[0]['notices'][0]['sha256'], hashlib.sha256(data).hexdigest())
        self.assertEqual((self.output / 'example-1.0.0/license-mit').read_bytes(), data)

    def test_unresolved_release_dependency_blocks_packaging(self):
        package = self.package()
        with self.assertRaisesRegex(RuntimeError, 'Cannot package.*example-1.0.0'):
            notices.collect([package], {'example-1.0.0'}, self.output, offline=True)

    def test_unresolved_optional_dependency_is_explicit(self):
        records = notices.collect([self.package()], set(), self.output, offline=True)
        self.assertEqual(records[0]['notice_status'], 'missing')
        self.assertFalse(records[0]['release_dependency'])
        self.assertIn('not a replacement', records[0]['notices'][0]['note'])

    def test_upstream_declaration_is_not_a_complete_license(self):
        package = self.package('dispatch', '0.2.0')
        (Path(package['manifest_path']).parent / 'LICENSE').write_text('License declaration only')
        with self.assertRaisesRegex(RuntimeError, 'dispatch-0.2.0'):
            notices.collect([package], {'dispatch-0.2.0'}, self.output, offline=True)

    def test_authors_file_counts_only_for_reviewed_r_efi_versions(self):
        package = self.package('r-efi', '5.3.0')
        (Path(package['manifest_path']).parent / 'AUTHORS').write_text('Test fixture upstream MIT notice')
        records = notices.collect([package], {'r-efi-5.3.0'}, self.output, offline=True)
        self.assertEqual(records[0]['license_choice'], 'MIT')
        self.assertTrue(records[0]['notices'][0]['file'].endswith('/AUTHORS'))

    def test_changed_upstream_revision_requires_review(self):
        package = self.package('iced_core', '0.14.0', repository='https://github.com/iced-rs/iced')
        (Path(package['manifest_path']).parent / '.cargo_vcs_info.json').write_text(
            json.dumps({'git': {'sha1': '0' * 40}}))
        with self.assertRaisesRegex(RuntimeError, 'Upstream source changed'):
            notices.collect([package], {'iced_core-0.14.0'}, self.output, offline=True)

    def test_same_upstream_reuse_requires_matching_author(self):
        donor = self.package('winapi', '0.3.9', repository='https://github.com/retep998/winapi-rs', authors=['Author'])
        (Path(donor['manifest_path']).parent / 'LICENSE').write_text('Test fixture notice')
        recipient = self.package('winapi-i686-pc-windows-gnu', '0.4.0',
                                 repository=donor['repository'], authors=['Different author'])
        with self.assertRaisesRegex(RuntimeError, 'author changed'):
            notices.collect([donor, recipient], set(), self.output, offline=True)

    def test_poisoned_cache_is_rejected_without_network(self):
        expected = hashlib.sha256(b'expected fixture notice').hexdigest()
        (self.root / expected).write_bytes(b'wrong fixture notice')
        with patch.object(notices.urllib.request, 'urlopen', side_effect=AssertionError('network forbidden')):
            with self.assertRaisesRegex(RuntimeError, 'License checksum mismatch'):
                notices.fetch_notice('https://example.invalid/LICENSE', expected, offline=True, cache=self.root)

    def test_absent_cache_is_actionable_and_offline(self):
        with patch.object(notices.urllib.request, 'urlopen', side_effect=AssertionError('network forbidden')):
            with self.assertRaisesRegex(RuntimeError, 'Run the notice builder online once'):
                notices.fetch_notice('https://example.invalid/LICENSE', '0' * 64, offline=True, cache=self.root)


if __name__ == '__main__':
    unittest.main()
