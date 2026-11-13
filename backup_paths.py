"""Prepare a private backup destination without modifying unrelated folder permissions."""
from __future__ import annotations

import os
from pathlib import Path


def prepare_destination(chosen: str | Path) -> Path:
    """Create/use a mode-0700 backup folder; secure child for shared parent folders.

    Never chmod a folder chosen by a user if it might be shared with other
    applications. Prefer a private Multi-Backup child unless the folder is already
    private. Preserve existing backup trees rather than silently relocating them.
    """
    if not str(chosen).strip():
        raise ValueError('Select a backup destination directory')
    path = Path(chosen).expanduser()
    if path.is_symlink():
        raise ValueError('Refusing a symlinked backup destination')
    if path.exists() and not path.is_dir():
        raise ValueError('Backup destination is not a directory')
    if os.name != 'nt' and path.exists() and (path.stat().st_mode & 0o077):
        if any((path / name).exists() for name in ('metadata', 'repositories', '.state')):
            raise PermissionError(
                'Existing backup destination is not private (must be chmod 700). '
                'Its current data has been preserved; choose a private folder instead.'
            )
        path = path / 'Multi-Backup'
        if path.is_symlink():
            raise ValueError('Refusing a symlinked backup subdirectory')
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    if path.is_symlink():
        raise ValueError('Refusing a symlinked backup directory')
    if os.name != 'nt' and path.stat().st_mode & 0o077:
        raise PermissionError('Private backup destination must have mode 0700')
    return path.resolve()
