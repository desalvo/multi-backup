#!/usr/bin/env python3
"""Verified mounted-cloud logical backup browser and create-only restore.

Copies an independently quiesced age chunk store from an approved FUSE or
macOS File Provider mount into a private local staging directory. Rust
decrypts/manifests/verifies and then restores a selection into a NEW path.
The mount alone is never treated as a coherent atomic snapshot.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tempfile

BACKUP_ID = re.compile(r"^[0-9a-f]{64}$")
SNAPSHOT_ACK = "YES_SOURCE_QUIESCED"
CHUNK = 1024 * 1024


def relative_path(value: str) -> bool:
    return isinstance(value, str) and (not value or (
        len(value) < 1024 and not value.startswith("/") and
        not any(c in value for c in ("\\", ":", "\0", "\n", "\r")) and
        all(x not in ("", ".", "..") and not x.startswith("~")
            for x in value.split("/"))))


def safe_root(mount: Path, subdir: str, mac_file_provider: bool = False) -> Path:
    if not relative_path(subdir):
        raise ValueError("Invalid remote store relative path")
    if not mount.is_absolute() or mount.is_symlink() or not mount.is_dir():
        raise ValueError("Cloud mount must be an existing real directory")
    root = mount.resolve(strict=True)
    if mac_file_provider:
        if sys.platform != "darwin":
            raise ValueError("Mac File Provider is only allowed on macOS")
        expected = (Path.home() /
                    "Library/Mobile Documents/com~apple~CloudDocs").resolve(strict=True)
        if root != expected and expected not in root.parents:
            raise ValueError("Cloud File Provider path is not in authorized iCloud Drive")
    elif not os.path.ismount(mount):
        raise ValueError("The source is not an actual mounted cloud filesystem")
    output = mount
    for segment in subdir.split("/") if subdir else []:
        output = output / segment
        if output.is_symlink():
            raise ValueError("Symlink inside cloud store path refused")
    result = output.resolve(strict=True)
    if result != root and root not in result.parents:
        raise ValueError("Cloud store escapes mount root")
    if not (result / ".multi-backup-v1").is_dir() or (result / ".multi-backup-v1").is_symlink():
        raise ValueError("No genuine Multi Backup encrypted store found")
    return result


def enumerate_store(source: Path, max_files: int = 100000,
                    max_bytes: int = 32 * 1024**3) -> tuple[list[tuple[Path, Path, int]], int]:
    """Inspect before copying. Reject special files and symlinks anywhere."""
    base = source / ".multi-backup-v1"
    files: list[tuple[Path, Path, int]] = []
    size = 0
    if not base.is_dir() or base.is_symlink():
        raise ValueError("Missing verified age chunk-store layout")
    for directory, dirs, names in os.walk(base, topdown=True, followlinks=False):
        parent = Path(directory)
        for item in dirs:
            p = parent / item
            if p.is_symlink() or not stat.S_ISDIR(p.lstat().st_mode):
                raise ValueError("Symlink or special cloud directory refused")
        for item in names:
            p = parent / item
            mode = p.lstat().st_mode
            if not stat.S_ISREG(mode) or stat.S_ISLNK(mode):
                raise ValueError("Symlink or nonregular cloud file refused")
            length = p.stat(follow_symlinks=False).st_size
            if length < 0:
                raise ValueError("Invalid file length")
            size += length
            if len(files) >= max_files or size > max_bytes:
                raise ValueError("Cloud store exceeds explicit staging limits")
            files.append((p, p.relative_to(source), length))
    if not (base / "config.json.age").is_file():
        raise ValueError("Missing age-authenticated store configuration")
    return files, size


def copy_stable(src: Path, target: Path, expected_size: int) -> None:
    """No symlink following on source and create-only local destination."""
    target.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    source_flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    destination_flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0)
    with os.fdopen(os.open(src, source_flags), "rb") as input_file:
        start = os.fstat(input_file.fileno())
        if not stat.S_ISREG(start.st_mode) or start.st_size != expected_size:
            raise ValueError("Cloud source changed before copy")
        digest = hashlib.sha256()
        with os.fdopen(os.open(target, destination_flags, 0o600), "wb") as output:
            total = 0
            while True:
                block = input_file.read(CHUNK)
                if not block:
                    break
                total += len(block)
                if total > expected_size:
                    raise ValueError("Cloud source grew while staging")
                digest.update(block)
                output.write(block)
            output.flush()
            os.fsync(output.fileno())
        finish = os.fstat(input_file.fileno())
        if total != expected_size or start.st_mtime_ns != finish.st_mtime_ns or (
            start.st_size != finish.st_size):
            raise ValueError("Cloud source mutated while copying")
    # Re-open source independently; this is a race check, NOT proof of a
    # provider-wide atomic snapshot across multiple objects.
    source_digest = hashlib.sha256()
    with os.fdopen(os.open(src, source_flags), "rb") as source_check:
        while (block := source_check.read(CHUNK)):
            source_digest.update(block)
    if digest.digest() != source_digest.digest():
        raise ValueError("Cloud file changed during double-read verification")


def stage(source: Path, private_cache: Path, max_files: int = 100000,
          max_bytes: int = 32 * 1024**3) -> Path:
    if private_cache.exists():
        raise ValueError("Fresh cache destination required; refusing overwrite")
    if private_cache.parent.is_symlink() or not private_cache.parent.is_dir():
        raise ValueError("Cache parent must be a real directory")
    files, total_bytes = enumerate_store(source, max_files, max_bytes)
    private_cache.mkdir(mode=0o700)
    copied = 0
    try:
        for index, (path, rel, size) in enumerate(files, 1):
            copy_stable(path, private_cache / rel, size)
            copied += size
            percent = round(100 * copied / total_bytes) if total_bytes else round(100 * index / len(files))
            width = min(24, percent * 24 // 100)
            print("\rCache cloud [" + "█" * width + "░" * (24 - width) +
                  f"] {percent}% · {index}/{len(files)} oggetti", end="", file=sys.stderr)
        print(file=sys.stderr)
    except Exception:
        shutil.rmtree(private_cache, ignore_errors=True)
        raise
    return private_cache


def run_native(native: Path, command: str, root: Path, identity: Path,
               backup_id: str | None = None, dest: Path | None = None,
               selection: str | None = None) -> str:
    if not native.is_file() or native.is_symlink():
        raise ValueError("Rust native executable missing or a symlink")
    args = [str(native), "storage", command, "--root", str(root),
            "--identity", str(identity)]
    if backup_id:
        if not BACKUP_ID.fullmatch(backup_id):
            raise ValueError("Invalid manifest content hash")
        args.extend(["--id", backup_id])
    if dest:
        if dest.exists() or dest.is_symlink() or not dest.parent.is_dir():
            raise ValueError("Restore destination must be a new path with an existing parent")
        args.extend(["--destination", str(dest)])
    if selection is not None:
        if not relative_path(selection) or not selection:
            raise ValueError("Invalid backup selection")
        args.extend(["--selection", selection])
    if command == "list":
        args.append("--json")
    if command == "restore" and (not backup_id or dest is None):
        raise ValueError("Restore requires a valid ID and new destination")
    if command not in ("list", "verify-chain", "restore"):
        raise ValueError("Unsupported native operation")
    result = subprocess.run(args, check=True, capture_output=True, text=True)
    return result.stdout


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--mount-point", required=True, type=Path)
    p.add_argument("--cloud-store-subdir", default="")
    p.add_argument("--file-provider-macos", action="store_true")
    p.add_argument("--source-quiesced", required=True, help="Enter " + SNAPSHOT_ACK)
    p.add_argument("--cache-parent", required=True, type=Path)
    p.add_argument("--age-identity", required=True, type=Path)
    p.add_argument("--native", required=True, type=Path)
    p.add_argument("--mode", choices=("list", "restore"), default="list")
    p.add_argument("--id")
    p.add_argument("--selection")
    p.add_argument("--destination", type=Path)
    args = p.parse_args()
    try:
        if args.source_quiesced != SNAPSHOT_ACK:
            raise ValueError("Source snapshot/quiesce must be explicitly attested")
        source = safe_root(args.mount_point, args.cloud_store_subdir, args.file_provider_macos)
        identity = args.age_identity.resolve(strict=True)
        if not identity.is_file() or identity.is_symlink() or (
            identity == source or source in identity.parents):
            raise ValueError("Age identity must be a regular LOCAL file outside the cloud")
        if args.mode == "restore" and (not args.id or not args.destination):
            raise ValueError("Restore requires --id and --destination")
        if args.mode == "list" and (args.id or args.destination or args.selection):
            raise ValueError("List never accepts mutating restore parameters")
        if args.cache_parent.is_symlink() or not args.cache_parent.is_dir():
            raise ValueError("Private cache parent must exist")
        with tempfile.TemporaryDirectory(prefix="multi-backup-cloud-", dir=args.cache_parent) as cache:
            destination = Path(cache) / "storage"
            stage(source, destination)
            if args.mode == "restore":
                run_native(args.native, "verify-chain", destination, identity, args.id)
                print(run_native(args.native, "restore", destination, identity,
                                 args.id, args.destination, args.selection))
            else:
                print(run_native(args.native, "list", destination, identity))
        return 0
    except (OSError, ValueError, subprocess.CalledProcessError) as ex:
        print(f"DENIED mounted-cloud backup access: {ex}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
