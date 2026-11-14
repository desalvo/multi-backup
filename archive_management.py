"""Guarded local encrypted archive deletion with on-disk audit and no remote writes.

Delete only an exact archive in the locally enumerated catalog and its matching
same-directory integrity/metadata sidecars. Never follow symlinks, delete
directories, use shell commands or touch remote provider APIs.
"""
from __future__ import annotations

import datetime as dt
import json
import os
from pathlib import Path

from archive_ops import catalog


class ArchiveDeletionError(RuntimeError):
    pass


def deletion_plan(output: Path, archive: Path):
    """Return a validated list of local files; never mutate in this function."""
    output = Path(output).expanduser().absolute()
    if output.is_symlink() or not output.is_dir():
        raise ArchiveDeletionError('Invalid local archive root')
    archive = Path(archive).expanduser().absolute()
    try:
        relative = archive.relative_to(output)
    except ValueError:
        raise ArchiveDeletionError('Refusing to delete outside backup root') from None
    if not relative.parts or archive.suffixes[-3:] != ['.tar', '.gz', '.age']:
        raise ArchiveDeletionError('Expected a registered encrypted .tar.gz.age archive')
    if any((output.joinpath(*relative.parts[:index])).is_symlink()
           for index in range(1, len(relative.parts) + 1)):
        raise ArchiveDeletionError('Refusing symbolic links in archive path')
    if not archive.is_file():
        raise ArchiveDeletionError('Archive does not exist')
    matches = [Path(item['path']).absolute()
               for item in catalog(output)]
    if archive not in matches:
        raise ArchiveDeletionError('Archive is not in the current local catalog')
    active = output / '.state' / 'active.json'
    if active.is_symlink():
        raise ArchiveDeletionError('Backup checkpoint is a symbolic link')
    if active.is_file():
        try:
            state = json.loads(active.read_text('utf-8'))
            steps = state.get('steps', {})
            if not isinstance(steps, dict):
                raise ValueError('Invalid checkpoint')
            if any(Path(v.get('path', '')).absolute() == archive
                   for v in steps.values() if isinstance(v, dict)):
                raise ArchiveDeletionError(
                    'This archive belongs to an unfinished backup. Resume or Start a '
                    'new cycle before deleting the archived file.')
        except (OSError, ValueError, TypeError) as exc:
            raise ArchiveDeletionError('Cannot safely inspect active checkpoint') from exc
    result = [archive]
    for suffix in ('.sha256', '.json', '.artifact-omissions.json'):
        companion = archive.with_name(archive.name + suffix)
        if companion.is_symlink():
            raise ArchiveDeletionError('Refusing symbolic-link companion metadata')
        if companion.is_file():
            result.append(companion)
    # No directories, shared caches, or remote files are added.
    return result


def delete_local_archive(output: Path, archive: Path, *,
                         confirmation: str, in_use=False):
    """Delete exact named local archive after explicit typed confirmation."""
    if in_use:
        raise ArchiveDeletionError('Cannot delete archives while a backup is running')
    archive = Path(archive).expanduser().absolute()
    if not isinstance(confirmation, str) or confirmation != archive.name:
        raise ArchiveDeletionError('Confirmation must exactly match the archive filename')
    files = deletion_plan(output, archive)
    output = Path(output).expanduser().absolute()
    directory = output / '.state'
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    log = directory / 'archive-deletions.jsonl'
    if log.is_symlink():
        raise ArchiveDeletionError('Refusing deletion audit log symlink')
    removed = []
    try:
        for item in files:
            item.unlink()
            removed.append(item.relative_to(output).as_posix())
    finally:
        # Record every removed file even if an unexpected OS error interrupted
        # removal. Flush auditable deletion details before returning.
        event = {'utc':dt.datetime.now(dt.timezone.utc).isoformat(),
                 'requested':archive.relative_to(output).as_posix(),
                 'removed':removed,'complete':len(removed)==len(files),
                 'operation':'local-delete-only'}
        fd = os.open(log, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
        try:
            with os.fdopen(fd, 'a', encoding='utf-8') as stream:
                stream.write(json.dumps(event, ensure_ascii=False) + '\n')
                stream.flush()
                os.fsync(stream.fileno())
        except BaseException:
            raise
    return removed
