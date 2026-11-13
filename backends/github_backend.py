"""GitHub adapter for the existing crash-safe disaster backup engine."""
from __future__ import annotations

def run(args, progress=None):
    from multi_backup import ResumableBackup
    from github_engine import check_prerequisites
    check_prerequisites(args)
    return ResumableBackup(args, progress).run()
