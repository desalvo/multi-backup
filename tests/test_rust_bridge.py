"""Verify that the actually compiled Rust ABI matches the legacy progress model."""
import colorsys
import unittest

from rust_bridge import (_native_core, native_progress_color,
                         native_completion_percent)


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

    def test_missing_library_fallback_is_not_implicit_success(self):
        from unittest.mock import patch
        with patch('rust_bridge._native_core', return_value=None):
            self.assertIsNone(native_progress_color(54))
            self.assertIsNone(native_completion_percent(54, 100))


if __name__ == '__main__':
    unittest.main()
