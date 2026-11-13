#!/usr/bin/env python3
"""Capture genuine Tk application screens for the EN/IT manuals and brochure.

Always uses a temporary, isolated user profile with no accounts or credentials.
Real widgets render inside Xvfb; no design mock or inferred backup activity.
"""
from __future__ import annotations

import os
import tempfile
from pathlib import Path


def main():
    from PIL import ImageGrab
    with tempfile.TemporaryDirectory(prefix='multi-backup-publications-') as tmp:
        os.environ['HOME'] = tmp
        os.environ['XDG_CONFIG_HOME'] = str(Path(tmp) / 'config')
        os.environ['APPDATA'] = str(Path(tmp) / 'config')
        from gui import BackupGUI

        # Documentation capture must never open Keychain, request passwords,
        # authenticate providers or issue network requests.
        def readonly_startup(self):
            self._startup_completed = True
            self._locked = False
            self._show('Dashboard')
        BackupGUI._startup_security = readonly_startup

        app = BackupGUI()
        try:
            app.geometry('1480x960+45+45')
            directory = Path(tmp) / 'Empty-Backup-Archive'
            directory.mkdir(mode=0o700)
            app.root_dir.set(str(directory))
            app.update()
            screens = {
                'dashboard.png': 'Dashboard',
                'backup.png': 'Backup',
                'archives.png': 'Archives',
                'restore.png': 'Restore',
                'encryption.png': 'Encryption',
                'plugins.png': 'Plugins',
                'updates.png': 'Updates',
                'settings.png': 'Settings',
            }
            target = Path(__file__).resolve().parents[1] / 'docs' / 'screenshots'
            target.mkdir(parents=True, exist_ok=True)
            for filename, page in screens.items():
                app._show(page)
                app.page_canvas.yview_moveto(0)
                app.update()
                left, top = app.winfo_rootx(), app.winfo_rooty()
                right, bottom = left + app.winfo_width(), top + app.winfo_height()
                if right <= left or bottom <= top:
                    raise RuntimeError('Invalid application screenshot geometry')
                grab = ImageGrab.grab(bbox=(left, top, right, bottom))
                if grab.width < 1100 or grab.height < 700:
                    raise RuntimeError('Application screenshot unexpectedly small')
                grab.convert('RGB').save(target / filename, optimize=True)
                print(filename, grab.size, flush=True)
        finally:
            app.destroy()


if __name__ == '__main__':
    main()
