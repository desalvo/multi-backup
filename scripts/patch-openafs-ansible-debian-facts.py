#!/usr/bin/env python3
"""Fix Debian 12 OpenAFS real client cacheinfo install-path detection.

The pinned upstream Ansible collection infers viceetcdir from a specific man
page text. Debian 12 may provide a functioning /etc/openafs/cacheinfo but omit
the directory from facts, making openafs_volume fail to create root.afs.
Only use this fallback if the REAL cacheinfo file exists. No simulated tools.
"""
from __future__ import annotations
import argparse
from pathlib import Path

NEEDLE = "        dirs = self.collect_dirs(manpages)\n        cacheinfo = self.collect_cacheinfo(dirs)"
FIX = """        dirs = self.collect_dirs(manpages)
        # Debian 12: packaged cacheinfo man text may not identify viceetcdir.
        # Do not override any path detected by the upstream collector.
        if not dirs.get('viceetcdir') and os.path.isfile('/etc/openafs/cacheinfo'):
            dirs['viceetcdir'] = '/etc/openafs'
        cacheinfo = self.collect_cacheinfo(dirs)"""


def patch(path: Path) -> None:
    source = path.read_text(encoding="utf-8")
    if source.count(FIX) == 1:
        print("PASS: already patched")
        return
    if source.count(NEEDLE) != 1 or "class DebianInstallationFactCollector(" not in source:
        raise RuntimeError("Unexpected upstream collection content; refuse patch")
    amended = source.replace(NEEDLE, FIX)
    compile(amended, str(path), "exec")
    path.write_text(amended, encoding="utf-8")
    print("PASS: path discovery uses real /etc/openafs/cacheinfo only when present")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("module", type=Path)
    args = p.parse_args()
    if args.module.name != "openafs_get_install_paths.py":
        p.error("Expected SHA-pinned upstream install-paths module")
    patch(args.module)
