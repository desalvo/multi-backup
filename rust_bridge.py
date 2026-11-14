"""Safe, optional Rust-native kernel for the Python-to-Rust transition.

Only bundled paths and this source checkout's own build products are considered.
No network loading, system library lookup or untrusted plugin directories.
If a matching library is absent, legacy Python remains behaviorally identical.
"""
from __future__ import annotations

import ctypes
import sys
from functools import lru_cache
from pathlib import Path


@lru_cache(maxsize=1)
def _native_core():
    if sys.platform == 'darwin':
        name = 'libmulti_backup_core.dylib'
    elif sys.platform == 'win32':
        name = 'multi_backup_core.dll'
    else:
        name = 'libmulti_backup_core.so'

    root = Path(__file__).resolve().parent
    roots = []
    if getattr(sys, '_MEIPASS', None):
        roots.append(Path(sys._MEIPASS) / 'rust-native')
    else:
        roots.extend((root / 'rust' / 'target' / 'release',
                      root / 'rust' / 'target' / 'debug'))
    for directory in roots:
        candidate = directory / name
        if not candidate.is_file() or candidate.is_symlink():
            continue
        try:
            lib = ctypes.CDLL(str(candidate))
            lib.multi_backup_progress_rgb.argtypes = [ctypes.c_double]
            lib.multi_backup_progress_rgb.restype = ctypes.c_uint32
            lib.multi_backup_completion_percent.argtypes = [
                ctypes.c_uint64, ctypes.c_uint64]
            lib.multi_backup_completion_percent.restype = ctypes.c_uint32
            lib.multi_backup_ci_target.argtypes = [
                ctypes.c_uint64, ctypes.c_uint64]
            lib.multi_backup_ci_target.restype = ctypes.c_uint64
            lib.multi_backup_ci_should_schedule.argtypes = [
                ctypes.c_uint64, ctypes.c_uint64, ctypes.c_uint64, ctypes.c_uint64]
            lib.multi_backup_ci_should_schedule.restype = ctypes.c_uint32
            return lib
        except (OSError, AttributeError):
            continue
    return None


def native_progress_color(percent):
    """Return #RRGGBB from bundled Rust, or None when running Python-only."""
    lib = _native_core()
    if lib is None:
        return None
    try:
        return f'#{int(lib.multi_backup_progress_rgb(float(percent))):06X}'
    except (ValueError, TypeError, OverflowError):
        return None


def native_completion_percent(completed, total):
    lib = _native_core()
    if lib is None:
        return None
    try:
        return int(lib.multi_backup_completion_percent(
            max(0, int(completed)), max(0, int(total))))
    except (ValueError, TypeError, OverflowError):
        return None


def native_ci_target_count(available, requested):
    """Use native Rust target policy where bundled; otherwise return None."""
    lib = _native_core()
    if lib is None:
        return None
    try:
        available, requested = int(available), int(requested)
        if min(available, requested) < 0:
            raise ValueError('Run counts cannot be negative')
        return int(lib.multi_backup_ci_target(available, requested))
    except (ValueError, TypeError, OverflowError):
        return None


def native_ci_should_schedule(completed, pending, target, workers):
    """Native conservative CI backfill capacity decision, or None."""
    lib = _native_core()
    if lib is None:
        return None
    try:
        values = [int(x) for x in (completed, pending, target, workers)]
        if any(x < 0 for x in values):
            return None
        return bool(lib.multi_backup_ci_should_schedule(*values))
    except (ValueError, TypeError, OverflowError):
        return None
