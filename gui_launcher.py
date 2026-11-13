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


def main():
    if len(sys.argv)==2 and sys.argv[1]=='--self-test':
        return bundle_self_test()
    from runtime_bootstrap import configure_path
    configure_path()
    from gui import run_gui
    return run_gui()

if __name__ == '__main__':
    raise SystemExit(main())
