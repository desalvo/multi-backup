"""Data integrity regressions for the approved photographic dashboard."""
from __future__ import annotations

import os
import tempfile
import time
import unittest
from pathlib import Path

from premium_dashboard import format_size, summarize_catalog


class DashboardMetricsTests(unittest.TestCase):
    def test_display_sizes_are_not_fake(self):
        self.assertEqual(format_size(0), '0 B')
        self.assertEqual(format_size(None), '—')
        self.assertEqual(format_size(1024), '1.0 KB')
        self.assertEqual(format_size(1024 ** 3), '1.0 GB')

    def test_archives_from_existing_files_only(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / 'protected.age'
            path.write_bytes(b'backup-data')
            timestamp = time.time() - 3600
            os.utime(path, (timestamp, timestamp))
            metrics = summarize_catalog(
                [{'path': str(path), 'relative': 'protected.age',
                  'kind': 'dropbox-file'},
                 {'path': str(Path(root) / 'missing.age'), 'relative': 'missing',
                  'kind': 'github'}],
                root, now=time.time())
            self.assertEqual(metrics['count'], 1)
            self.assertEqual(metrics['recent'][0]['provider'], 'Dropbox')
            self.assertEqual(metrics['recent'][0]['name'], 'protected.age')
            self.assertEqual(metrics['recent'][0]['size'], 11)
            self.assertEqual(sum(metrics['days']), 1)
            self.assertIsNotNone(metrics['disk'])

    def test_empty_catalog_is_not_reported_as_success(self):
        with tempfile.TemporaryDirectory() as directory:
            metrics = summarize_catalog([], directory, now=0)
            self.assertEqual(metrics['count'], 0)
            self.assertEqual(metrics['recent'], [])
            self.assertEqual(metrics['days'], [0] * 7)

    def test_untrusted_catalog_metadata_does_not_break_dashboard(self):
        with tempfile.TemporaryDirectory() as directory:
            result = summarize_catalog(
                [{'kind': 'github'}, {'path': '\0invalid'}, {}],
                directory)
            self.assertEqual(result['count'], 0)


if __name__ == '__main__':
    unittest.main()
