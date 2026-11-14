"""Verify that installed apps use only the bundled, checked Rust helper."""
from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import archive_ops
from archive_ops import ArchiveError


class NativeArchiveRoutingTests(unittest.TestCase):
    def test_source_python_does_not_short_circuit_differential_tests(self):
        with patch.object(archive_ops.sys, 'frozen', False, create=True):
            self.assertIsNone(archive_ops._bundled_rust_archive())

    def test_mac_helper_must_be_inside_its_own_app_bundle(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder) / 'Multi Backup.app/Contents'
            exe = root / 'MacOS/Multi Backup'
            helper = root / 'Resources/bin/multi-backup-native'
            exe.parent.mkdir(parents=True)
            helper.parent.mkdir(parents=True)
            exe.write_bytes(b'native app placeholder')
            helper.write_bytes(b'native helper placeholder')
            with (patch.object(archive_ops.sys, 'frozen', True, create=True),
                  patch.object(archive_ops.sys, 'platform', 'darwin'),
                  patch.object(archive_ops.sys, 'executable', str(exe))):
                self.assertEqual(archive_ops._bundled_rust_archive(), helper)
                helper.unlink()
                self.assertIsNone(archive_ops._bundled_rust_archive())
                helper.symlink_to(exe)
                self.assertIsNone(archive_ops._bundled_rust_archive())

    def test_native_errors_are_not_silently_downgraded_to_python(self):
        with patch.object(archive_ops, '_bundled_rust_archive',
                          return_value=Path('/trusted/multi-backup-native')):
            with patch.object(archive_ops.subprocess, 'run',
                              return_value=SimpleNamespace(
                                  returncode=2, stdout='', stderr='Invalid tar member')):
                with self.assertRaisesRegex(ArchiveError, 'Invalid tar member'):
                    archive_ops.list_members(Path('/temp/sample.tar.gz.age'),
                                             Path('/temp/identity'))

    def test_json_archive_list_preserves_current_output_schema(self):
        records = [{'path': 'multi-backup/foo/data.json',
                    'bytes': 17, 'directory': False}]
        with patch.object(archive_ops, '_bundled_rust_archive',
                          return_value=Path('/trusted/multi-backup-native')):
            with patch.object(archive_ops.subprocess, 'run',
                              return_value=SimpleNamespace(
                                  returncode=0, stdout=json.dumps(records), stderr='')) as call:
                result = archive_ops.list_members(Path('/tmp/sample.tar.gz.age'),
                                                  Path('/tmp/key'))
                self.assertEqual(result, records)
                argv = call.call_args.args[0]
                self.assertIn('--json', argv)
                self.assertEqual(argv[1:3], ['archive', 'list'])


if __name__ == '__main__':
    unittest.main()
