"""Native GUI entry point for PyInstaller and bundled application smoke tests."""
from __future__ import annotations
import sys


def bundle_self_test():
    """Check resources and runtime imports without opening a window in CI."""
    from pathlib import Path
    import tkinter
    from PIL import Image, ImageTk
    import backends.github_backend
    import backends.dropbox_backend
    from gui import asset
    from runtime_bootstrap import configure_path
    configure_path()
    for filename in ('hero.jpg', 'multi-backup.png', 'icon-64.png'):
        path=Path(asset(filename))
        if not path.is_file():
            raise RuntimeError('Missing bundled image: '+filename)
        with Image.open(path) as picture:
            picture.verify()
    print('Multi Backup GUI bundle self-test OK')
    return 0


def gui_smoke():
    """Exercise actual native Tk controls, not merely imports or bundled assets."""
    from gui import BackupGUI
    application=BackupGUI()
    try:
        application.update_idletasks()
        assert application.start_button.caption == 'Start / resume backup'
        assert application.menu_button.caption == 'Hide navigation'
        application._startup_security()
        assert not application._locked
        assert len(application.nav) == 8
        application._show('Backup')
        application._show('Restore')
        application._show('Encryption')
        application._toggle_sidebar()
        assert not application.sidebar_visible
        application._toggle_sidebar()
        assert application.sidebar_visible
        application.progress.set_percent(50)
        application.update_idletasks()
    finally:
        application.destroy()
    print('Multi Backup real native GUI startup and navigation smoke OK')
    return 0


def main():
    if len(sys.argv)==2 and sys.argv[1]=='--self-test':
        return bundle_self_test()
    if len(sys.argv)==2 and sys.argv[1]=='--gui-smoke':
        return gui_smoke()
    from runtime_bootstrap import configure_path
    configure_path()
    from gui import run_gui
    return run_gui()

if __name__ == '__main__':
    raise SystemExit(main())
