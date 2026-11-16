#!/usr/bin/env python3
"""Assemble only previously built Rust-only native packages from THIS CI run.

No compilation, no Python/Tk/legacy packaging or signing is performed here.
Reject incomplete matrices, unexpected paths, duplicate names and symlinks.
Only an unsigned/ad-hoc-signed EARLY PREVIEW can be assembled.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sys

GUI = {
    "rust-only-desktop-linux-ubuntu24-preview": ".tar.gz",
    "rust-only-macos-arm64-preview": ".dmg",
    "rust-only-macos-x86_64-preview": ".dmg",
    "rust-only-windows-x64-preview": ".exe",
    **{f"rust-only-{d}-el{v}-rpm-preview": ".rpm"
       for d in ("rocky", "alma") for v in (8, 9, 10)},
    **{f"rust-only-debian-{v}-{a}-deb-preview": ".deb"
       for v in (12, 13) for a in ("amd64", "arm64")},
}
AGENT = {
    "rust-agent-macos-arm64-preview": ".pkg",
    "rust-agent-macos-x86_64-preview": ".pkg",
    "rust-agent-windows-x64-preview": ".exe",
    **{f"rust-agent-{d}-el{v}-preview": ".rpm"
       for d in ("rocky", "alma") for v in (8, 9, 10)},
    **{f"rust-agent-debian-{v}-{a}-preview": ".deb"
       for v in (12, 13) for a in ("amd64", "arm64")},
}
ALLOWED = (".dmg", ".exe", ".rpm", ".deb", ".pkg", ".zip", ".tar.gz")


def assemble(gui: Path, agents: Path, out: Path) -> dict:
    if out.exists() and any(out.iterdir()):
        raise ValueError("Release destination must be empty")
    pending = []
    names = set()
    for root, expected in ((gui, GUI), (agents, AGENT)):
        if not root.is_dir() or root.is_symlink():
            raise ValueError("Required artifact collection absent or symlinked")
        observed = {p.name for p in root.iterdir()}
        if observed != set(expected):
            raise ValueError(
                f"Rust artifact matrix mismatch: missing={sorted(set(expected)-observed)} "
                f"extra={sorted(observed-set(expected))}")
        for folder, extension in sorted(expected.items()):
            p = root / folder
            if not p.is_dir() or p.is_symlink():
                raise ValueError("Untrusted artifact directory")
            files = [f for f in p.iterdir() if f.is_file() and not f.is_symlink()]
            archives = [f for f in files if f.name.endswith(ALLOWED)]
            if not any(f.name.endswith(extension) for f in archives):
                raise ValueError(f"Missing required native {extension} in {folder}")
            for file in archives:
                name = file.name
                if (name in names or file.stat().st_size == 0 or
                    not name.startswith(("multi-backup-rust", "multi-backup-agent"))):
                    raise ValueError(f"Duplicate, empty or non-Rust release asset: {name}")
                if any(word in name.lower() for word in ("pyinstaller", "tkinter", "legacy")):
                    raise ValueError("Legacy package is not allowed in Rust-only release")
                names.add(name)
                pending.append(file)
    out.mkdir(parents=True, exist_ok=True)
    results = []
    for file in sorted(pending, key=lambda x:x.name):
        dest = out / file.name
        with file.open("rb") as source, dest.open("xb") as target:
            digest = hashlib.sha256()
            size = 0
            while chunk := source.read(1024 * 1024):
                target.write(chunk)
                digest.update(chunk)
                size += len(chunk)
        results.append({"name":dest.name, "bytes":size, "sha256":digest.hexdigest()})
    return {"matrix_folders":len(GUI)+len(AGENT),
            "assets":results, "python_legacy_included":False, "stable_certified":False}


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--gui",type=Path,required=True)
    parser.add_argument("--agents",type=Path,required=True)
    parser.add_argument("--out",type=Path,required=True)
    args=parser.parse_args()
    result=assemble(args.gui,args.agents,args.out)
    print(json.dumps({"rust_release_collection":"PASS",
        "matrix_folders":result["matrix_folders"],
        "assets":len(result["assets"]),"stable_certified":False},sort_keys=True))


if __name__=="__main__":
    try: main()
    except (OSError, ValueError) as exc:
        print(f"FAIL Rust-only release: {exc}",file=sys.stderr)
        raise SystemExit(1) from None
