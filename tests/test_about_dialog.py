"""Native About, photographic shell and publication pipeline regressions."""
from __future__ import annotations

import unittest
from pathlib import Path
from unittest.mock import patch

from about_dialog import AboutDialog, PROJECT_URL
from version import AUTHOR, BUILD, REPOSITORY, VERSION

ROOT = Path(__file__).resolve().parents[1]


class _FakeClipboard:
    def __init__(self):
        self.value = None
        self.cleared = False
        self.flushed = False

    def clipboard_clear(self):
        self.cleared = True

    def clipboard_append(self, text):
        self.value = text

    def update_idletasks(self):
        self.flushed = True


class AboutDialogTests(unittest.TestCase):
    def test_about_metadata_is_single_source_of_truth(self):
        self.assertEqual(PROJECT_URL, 'https://github.com/' + REPOSITORY)
        self.assertTrue(VERSION.startswith('0.1.0-r'))
        self.assertTrue(BUILD.isdecimal())
        self.assertTrue(AUTHOR)
        source = (ROOT / 'about_dialog.py').read_text('utf-8')
        for token in ('VERSION', 'BUILD', 'AUTHOR', 'EUPL-1.2',
                      'PROJECT_URL', 'ImageOps.contain', 'Escape'):
            self.assertIn(token, source)
        self.assertNotIn('load_vault(', source)
        self.assertNotIn('unlock(', source)

    def test_copy_version_copies_real_build(self):
        fake = _FakeClipboard()
        AboutDialog._copy_version(fake)
        self.assertTrue(fake.cleared)
        self.assertEqual(fake.value, f'Multi Backup {VERSION} (build {BUILD})')
        self.assertTrue(fake.flushed)

    def test_project_website_opens_only_on_explicit_action(self):
        with patch('about_dialog.webbrowser.open') as browser:
            AboutDialog._open_project(_FakeClipboard())
            browser.assert_called_once_with(PROJECT_URL)

    def test_native_about_available_in_toolbar_sidebar_and_os_menu(self):
        gui = (ROOT / 'gui.py').read_text('utf-8')
        self.assertGreaterEqual(gui.count('command=self._show_about'), 3)
        self.assertIn("self.tk.createcommand('tk::mac::ShowAbout'", gui)
        self.assertIn("self.bind('<F1>'", gui)
        self.assertIn("current.winfo_exists()", gui)

    def test_secondary_pages_retain_photo_mock_palette(self):
        gui = (ROOT / 'gui.py').read_text('utf-8')
        self.assertIn("PAGE_BG = '#152753'", gui)
        self.assertIn("self.page_canvas.configure(bg=PAGE_BG)", gui)
        self.assertIn("style.configure('TCheckbutton'", gui)
        self.assertIn("self._build_dashboard(self.pages['Dashboard'])", gui)
        self.assertIn("self._build_backup(self.pages['Backup'])", gui)

    def test_capture_script_is_cwd_independent_and_captures_about(self):
        src = (ROOT / 'scripts' / 'capture-ui.py').read_text('utf-8')
        self.assertLess(src.index('sys.path.insert(0, str(ROOT))'),
                        src.index('from gui import BackupGUI'))
        self.assertIn("target / 'about.png'", src)
        workflow = (ROOT / '.github/workflows/ci.yml').read_text('utf-8')
        self.assertIn('publications-smoke:', workflow)
        self.assertIn('scripts/build-publications.py', workflow)
        self.assertIn('docs/screenshots/about.png', workflow)

    def test_manually_started_backups_and_post_backup_cleanup_are_documented(self):
        src = (ROOT / 'scripts' / 'build-publications.py').read_text('utf-8')
        self.assertIn('About Multi Backup', src)
        self.assertIn("('Informazioni sull’app (About)'", src)
        self.assertIn('runs only AFTER a verified successful', src)
        self.assertIn('--resume', src)


if __name__ == '__main__':
    unittest.main()
