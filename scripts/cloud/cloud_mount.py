#!/usr/bin/env python3
"""Supervised, foreground desktop cloud mounts via existing rclone profiles.

No credential editing, no rclone RC, no daemon, no shell, no arbitrary
backend spec. The GUI/agent should own this child's lifecycle and terminate
it before deleting cached private data. Provider POSIX semantics are limited.
"""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ALLOWED = {"dropbox", "google-drive", "onedrive", "owncloud", "nextcloud",
           "icloud", "webdav", "s3", "other"}
NAME = re.compile(r"^[A-Za-z0-9_-]{1,64}$")


def relative(value: str, empty: bool = True) -> bool:
    if not isinstance(value, str) or len(value) > 1024:
        return False
    if not value:
        return empty
    return not (value.startswith("/") or "\\" in value or ":" in value or
                any(ord(c) < 32 or ord(c) == 127 for c in value) or
                len(value.split("/")) > 40 or
                any(p in ("", ".", "..") or p.startswith("~")
                    for p in value.split("/")))


def private_file(path: Path) -> None:
    meta = path.lstat()
    if not path.is_file() or path.is_symlink():
        raise ValueError(f"Not a trusted regular file: {path}")
    if os.name != "nt" and (meta.st_mode & 0o077):
        raise ValueError(f"Cloud configuration must be 0600: {path}")


def configured(registry: Path, provider_id: str) -> tuple[dict, dict]:
    private_file(registry)
    obj = json.loads(registry.read_text(encoding="utf-8"))
    if set(obj) != {"rclone_config", "profiles"}:
        raise ValueError("Unknown or missing registry fields")
    cfg = Path(obj["rclone_config"])
    if not cfg.is_absolute():
        raise ValueError("Rclone config must be an absolute, private path")
    private_file(cfg)
    profiles = obj["profiles"]
    if not isinstance(profiles, list) or len(profiles) > 32:
        raise ValueError("Invalid profile catalog")
    ids: set[str] = set()
    selected = None
    for p in profiles:
        if not isinstance(p, dict) or not {"id", "provider", "remote"} <= set(p):
            raise ValueError("Invalid cloud profile")
        if not NAME.fullmatch(p["id"]) or not NAME.fullmatch(p["remote"]) or p["id"] in ids:
            raise ValueError("Duplicate profile or unsafe cloud remote")
        if p["provider"] not in ALLOWED or not relative(p.get("prefix", "")):
            raise ValueError("Invalid cloud provider/prefix")
        if any(type(p.get(k, False)) is not bool for k in ("allow_upload", "allow_delete")):
            raise ValueError("Invalid capability flags")
        if p["provider"] == "icloud" and (p.get("allow_upload") or p.get("allow_delete")):
            raise ValueError("iCloud remote mount cannot advertise writes")
        ids.add(p["id"])
        if p["id"] == provider_id:
            selected = p
    if selected is None:
        raise ValueError("Unknown cloud profile")
    return obj, selected


def mount_command(registry: Path, profile_id: str, mount: str,
                  rclone: str) -> list[str]:
    obj, p = configured(registry, profile_id)
    if not Path(rclone).is_absolute():
        raise ValueError("Executable must have an explicit absolute path")
    if not Path(rclone).is_file() or Path(rclone).is_symlink():
        raise ValueError("Rclone executable unavailable")
    if os.name == "nt" and re.fullmatch(r"[A-Za-z]:", mount):
        destination = mount  # WinFsp unused drive letter
    else:
        target = Path(mount)
        if not target.is_absolute() or not target.is_dir() or target.is_symlink():
            raise ValueError("Mount target must be a pre-existing real directory")
        if any(target.iterdir()):
            raise ValueError("Mount target must be empty")
        destination = str(target.resolve())
    prefix = p.get("prefix", "")
    remote = p["remote"] + ":" + prefix
    # Prefer read-only by default. iCloud rclone is read-only even when
    # File Provider on the signed-in Mac supports coordinated file writes.
    readonly = p["provider"] == "icloud" or not p.get("allow_upload", False)
    cmd = [rclone, "--config", obj["rclone_config"], "--ask-password=false",
           "mount", remote, destination, "--vfs-cache-mode", "full",
           "--vfs-cache-max-size", "2G", "--dir-cache-time", "30s",
           "--poll-interval", "1m"]
    if readonly:
        cmd.append("--read-only")
    return cmd


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--registry", type=Path, required=True)
    parser.add_argument("--profile", required=True)
    parser.add_argument("--mount-point", required=True)
    parser.add_argument("--rclone", default="/usr/bin/rclone")
    parser.add_argument("--dry-run", action="store_true")
    opts = parser.parse_args()
    try:
        cmd = mount_command(opts.registry, opts.profile,
                            opts.mount_point, opts.rclone)
        if opts.dry_run:
            # Never echo configured credentials, auth cookies or rclone config contents.
            print(json.dumps({"remote_profile": opts.profile, "mount": cmd[5],
                              "readonly": "--read-only" in cmd,
                              "rclone": Path(cmd[0]).name}))
            return 0
        # Foreground: GUI/service manager supervises this process; failures
        # propagate. No rc admin listener and no privileged --allow-other.
        return subprocess.run(cmd, check=False).returncode
    except (OSError, ValueError, TypeError, KeyError, json.JSONDecodeError) as exc:
        print(f"Cloud mount refused: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
