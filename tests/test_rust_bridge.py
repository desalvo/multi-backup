"""Verify that the actually compiled Rust ABI matches the legacy progress model."""
import colorsys
import hashlib
import tempfile
import unittest
from pathlib import Path

from rust_bridge import (_native_core, native_progress_color,
                         native_completion_percent, native_ci_target_count,
                         native_ci_should_schedule,
                         native_dropbox_content_hash)


class RustKernelTests(unittest.TestCase):
    def test_rust_round_trip_gradient(self):
        lib = _native_core()
        if lib is None:
            self.skipTest('Rust native kernel is not built in this source environment')
        for percentage in range(101):
            rgb = colorsys.hsv_to_rgb(percentage / 300., .86, .91)
            expected = '#%02X%02X%02X' % tuple(
                round(value * 255) for value in rgb)
            self.assertEqual(native_progress_color(percentage), expected,
                             f'{percentage}% native kernel parity mismatch')

    def test_rust_progress_event_bounded(self):
        if _native_core() is None:
            self.skipTest('Rust native kernel is not built in this source environment')
        for current, total, expected in ((0, 0, 0), (0, 15, 0),
                                         (7, 14, 50), (200, 100, 100)):
            self.assertEqual(native_completion_percent(current, total), expected)

    def test_native_ci_backfill_policy_matches_python(self):
        if _native_core() is None:
            self.skipTest('Rust kernel not built')
        for available in range(12):
            for limit in range(12):
                expected = min(limit, available) if limit else available
                self.assertEqual(native_ci_target_count(available, limit), expected)
        for done in range(6):
            for pending in range(6):
                for target in range(6):
                    for workers in range(1, 5):
                        expected = pending < workers and done + pending < target
                        self.assertEqual(native_ci_should_schedule(
                            done, pending, target, workers), expected)

    def test_rust_dropbox_hash_matches_legacy_four_megabyte_blocks(self):
        if _native_core() is None:
            self.skipTest('Rust native core is not built')
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'données.bin'
            for data in (b'', b'hello', b'abcd' * (1024 * 1024),
                         b'abcd' * (1024 * 1024) + b'1'):
                path.write_bytes(data)
                expected = hashlib.sha256()
                for i in range(0, len(data), 4 * 1024 * 1024):
                    expected.update(hashlib.sha256(
                        data[i:i + 4 * 1024 * 1024]).digest())
                self.assertEqual(native_dropbox_content_hash(path),
                                 expected.hexdigest())

    def test_missing_library_fallback_is_not_implicit_success(self):
        from unittest.mock import patch
        with patch('rust_bridge._native_core', return_value=None):
            self.assertIsNone(native_progress_color(54))
            self.assertIsNone(native_completion_percent(54, 100))
            self.assertIsNone(native_ci_target_count(5, 3))
            self.assertIsNone(native_ci_should_schedule(1, 0, 5, 4))
            self.assertIsNone(native_dropbox_content_hash('missing.bin'))


if __name__ == '__main__':
    unittest.main()
