"""Explicit Dropbox backup session names preserved across Resume operations.

The immutable encrypted object cache is still verified using SHA-256 and
Dropbox revisions. The session tracks only the intended manifest filename,
never access tokens or unencrypted payloads.
"""
from __future__ import annotations

import datetime as dt
import hashlib
import json
import os
import re
from pathlib import Path

from checkpoint import atomic_json, sha256


def _hash_configuration(recipient: str, path: str) -> str:
    return hashlib.sha256(json.dumps(
        {'recipient': recipient, 'path': path}, sort_keys=True
    ).encode('utf-8')).hexdigest()


def start_dropbox_session(output: Path, recipient: str, remote_path: str,
                          *, resume=False, restart=False):
    """Return (stable manifest name, resumed), preserving previous session."""
    if resume and restart:
        raise ValueError('Cannot Start and Resume simultaneously')
    output = Path(output)
    state_dir = output / '.state'
    state_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    active = state_dir / 'dropbox-active.json'
    if active.is_symlink():
        raise RuntimeError('Refusing Dropbox checkpoint symlink')
    digest = _hash_configuration(recipient, remote_path)
    if resume:
        if not active.is_file():
            raise RuntimeError('No incomplete Dropbox backup to resume; use Start.')
        saved = json.loads(active.read_text('utf-8'))
        name = saved.get('manifest_name', '')
        if (saved.get('schema') != 1 or saved.get('config_hash') != digest
                or not isinstance(name, str)
                or not re.fullmatch(r'\d{8}T\d{12}Z\.tar\.gz\.age', name)):
            raise RuntimeError(
                'Dropbox resume checkpoint does not match this source or encryption key.')
        return name, True
    if active.is_file():
        old = json.loads(active.read_text('utf-8'))
        old_name = old.get('manifest_name', '')
        if not isinstance(old_name, str) or not re.fullmatch(
                r'\d{8}T\d{12}Z\.tar\.gz\.age', old_name):
            raise RuntimeError('Invalid previous Dropbox checkpoint; cannot overwrite')
        parked = state_dir / ('dropbox-abandoned-' + old_name + '.json')
        if parked.exists():
            raise RuntimeError('Previous Dropbox checkpoint already parked; manual review required')
        atomic_json(parked, old)
    now = dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
    name = now + '.tar.gz.age'
    atomic_json(active, {
        'schema': 1, 'backend': 'dropbox', 'config_hash': digest,
        'manifest_name': name,
        'started_utc': dt.datetime.now(dt.timezone.utc).isoformat()})
    return name, False


def finalize_dropbox_session(output: Path, manifest: Path) -> None:
    """Only finalize when the intended immutable encrypted manifest verifies."""
    output = Path(output)
    active = output / '.state' / 'dropbox-active.json'
    if not manifest.is_file() or not manifest.with_suffix(
            manifest.suffix + '.sha256').is_file():
        raise RuntimeError('Dropbox manifest is incomplete; checkpoint preserved')
    parts = manifest.with_suffix(manifest.suffix + '.sha256').read_text('utf-8').split()
    if not parts or parts[0] != sha256(manifest):
        raise RuntimeError('Dropbox manifest failed integrity check; checkpoint preserved')
    if active.is_file():
        saved = json.loads(active.read_text('utf-8'))
        if saved.get('manifest_name') != manifest.name:
            raise RuntimeError('Dropbox active manifest identity changed')
        active.unlink()


def dropbox_active_manifest(output: Path) -> str | None:
    """Read-only helper used by UI to show a real existing resume target."""
    active = Path(output) / '.state' / 'dropbox-active.json'
    if active.is_symlink() or not active.is_file():
        return None
    try:
        saved = json.loads(active.read_text('utf-8'))
    except (OSError, ValueError, UnicodeError):
        return None
    return saved.get('manifest_name') if saved.get('schema') == 1 else None
