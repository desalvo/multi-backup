"""Backend plugin API. Third-party plugins can be added via a signed distribution."""
from __future__ import annotations
from dataclasses import dataclass
from typing import Callable

@dataclass(frozen=True)
class BackendDescriptor:
    id: str
    title: str
    description: str
    supports_incremental: bool
    destructive_actions: bool
    default_enabled: bool
    handler: Callable


def available():
    from backends.github_backend import run as github_run
    from backends.dropbox_backend import run as dropbox_run
    return {
        'github': BackendDescriptor('github', 'GitHub',
            'Repositories, metadata, CI, Git mirrors and releases', True, True, True, github_run),
        'dropbox': BackendDescriptor('dropbox', 'Dropbox',
            'Incremental file revisions; remote source is never modified', True, False, False, dropbox_run),
    }
