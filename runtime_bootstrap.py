"""Locate native helper tools bundled beside the desktop app/CLI."""
from __future__ import annotations
import os
import sys
from pathlib import Path


def configure_path():
    roots=[Path(sys.executable).resolve().parent,Path(__file__).resolve().parent]
    for root in list(roots):
        roots.append(root/'../Resources')  # macOS .app
    additions=[]
    for root in roots:
        bin_dir=(root/'bin').resolve()
        if bin_dir.is_dir():additions.append(str(bin_dir))
    if additions:
        os.environ['PATH']=os.pathsep.join(additions+[os.environ.get('PATH','')])
