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
    from premium_dashboard import format_size, summarize_catalog
    assert format_size(0) == '0 B'
    assert summarize_catalog([], '.', now=0)['count'] == 0
    from runtime_bootstrap import configure_path
    from updater import _verified_https_context
    import certifi
    configure_path()
    assert Path(certifi.where()).is_file(), 'Missing frozen CA certificate bundle'
    tls=_verified_https_context()
    assert tls.check_hostname, 'TLS verification must be enabled'
    for filename in ('backup-hero.jpg', 'backup-sidebar.jpg', 'hero.jpg',
                     'multi-backup.png', 'icon-64.png'):
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
        assert application.start_button.caption == 'Start · Nuovo backup'
        assert application.resume_button.caption == 'Resume · Riprendi backup'
        assert callable(application.start_button.command)
        assert callable(application.resume_button.command)
        assert application.menu_button.caption == 'Nascondi navigazione'
        assert not application.ignore_ci_artifact_errors.get()
        application._startup_security()
        assert not application._locked
        assert len(application.nav) == 8
        application._show('Dashboard')
        application.dashboard.redraw()
        assert application.dashboard._hotspots, 'Premium dashboard actions are missing'
        assert application.dashboard._drawn_height >= 900
        application._show('Backup')
        application._show('Restore')
        application._show('Encryption')
        application._toggle_sidebar()
        assert not application.sidebar_visible
        assert application.menu_button.caption == 'Mostra navigazione'
        application._toggle_sidebar()
        assert application.sidebar_visible
        assert application.menu_button.caption == 'Nascondi navigazione'
        application.progress.set_running(True)
        application.progress.note_activity()
        assert application.progress.pulse.running
        application.progress.set_percent(50)
        application.progress.set_running(False)
        assert not application.progress.pulse.running
        assert application.progress._timer is None
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
