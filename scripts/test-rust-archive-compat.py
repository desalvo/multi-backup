#!/usr/bin/env python3
"""Real, offline interoperability between legacy Python archives and Rust.

This test uses *real* age keypairs, authentic age encryption and both independent
archive engines. It does not use fake cryptography, cloud credentials or a real
account and never prints private material.
"""
from __future__ import annotations

import io
import json
import os
import re
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from archive_ops import extract_archive, list_members  # noqa: E402
from github_engine import StreamingArchive  # noqa: E402
from checkpoint import Journal  # noqa: E402


def invoke(*argv, expect_success=True):
    result = subprocess.run(list(map(str, argv)), capture_output=True,
                            text=True, timeout=90, check=False)
    if expect_success and result.returncode:
        raise AssertionError(
            f"Rust interoperability command failed ({result.returncode}): "
            + result.stderr[-1200:])
    if not expect_success and result.returncode == 0:
        raise AssertionError("Unsafe archive was incorrectly accepted")
    return result


def main():
    binary = Path(sys.argv[1]) if len(sys.argv) > 1 else (
        ROOT / "rust/target/release/multi-backup-native")
    if not binary.is_file():
        raise RuntimeError(f"Rust native CLI not compiled: {binary}")
    with tempfile.TemporaryDirectory(prefix="multi-backup-rust-parity-") as d:
        root = Path(d)
        identity = root / "identity.txt"
        keygen = invoke("age-keygen", "-o", identity)
        pub_match = re.search(
            r"age1[023456789acdefghjklmnpqrstuvwxyz]+",
            keygen.stdout + keygen.stderr,
        )
        if not pub_match:
            raise AssertionError("age-keygen produced no public recipient")
        recipient = pub_match.group(0)
        source = root / "source"
        source.mkdir()
        (source / "fichier-été.txt").write_text("Confidential example, not an actual secret\n",
                                              encoding="utf-8")
        (source / "nested").mkdir()
        (source / "nested" / "data.json").write_text('{"ok": true}\n', encoding="utf-8")
        (source / "empty.txt").write_bytes(b"")

        legacy = root / "legacy.tar.gz.age"
        with StreamingArchive(legacy, recipient) as archive:
            archive.add_tree(source, "multi-backup/example")
            archive.add_json("multi-backup/backup_manifest.json",
                             {"schema": 1, "backend": "integration-fixture"})

        rust_list = json.loads(invoke(
            binary, "archive", "list", "--archive", legacy,
            "--identity", identity, "--json").stdout)
        names = {item["path"] for item in rust_list}
        assert "multi-backup/example/nested/data.json" in names, names
        assert "multi-backup/backup_manifest.json" in names, names
        invoke(binary, "archive", "verify", "--archive", legacy, "--identity", identity)
        checkpoint = Journal(root / ".state", {"recipient": recipient}, restart=True)
        checkpoint.record("encrypted", legacy)
        audit = json.loads(invoke(
            binary, "checkpoint", "audit", "--file",
            root / ".state" / "active.json", "--json").stdout)
        assert audit["valid"] == ["encrypted"] and not audit["invalid"]
        rust_dest = root / "restored_by_rust"
        invoke(binary, "archive", "extract", "--archive", legacy,
               "--identity", identity, "--destination", rust_dest)
        assert (rust_dest / "multi-backup/example/nested/data.json").read_bytes() == (
            source / "nested/data.json").read_bytes()
        assert (rust_dest / "multi-backup/example/fichier-été.txt").read_bytes() == (
            source / "fichier-été.txt").read_bytes()
        invoke(binary, "archive", "extract", "--archive", legacy,
               "--identity", identity, "--destination", rust_dest,
               expect_success=False)

        rust_archive = root / "rust.tar.gz.age"
        invoke(binary, "archive", "create", "--source", source,
               "--output", rust_archive, "--recipient", recipient,
               "--prefix", "multi-backup/example")
        rust_checksum = Path(str(rust_archive) + ".sha256")
        assert rust_checksum.is_file()
        members = list_members(rust_archive, identity)
        assert {m["path"] for m in members} >= {
            "multi-backup/example/nested/data.json",
            "multi-backup/example/empty.txt",
        }
        legacy_dest = root / "restored_by_python"
        extract_archive(rust_archive, identity, legacy_dest)
        assert (legacy_dest / "multi-backup/example/nested/data.json").read_bytes() == (
            source / "nested/data.json").read_bytes()

        rust_checksum.write_text("0" * 64 + "  rust.tar.gz.age\n", encoding="ascii")
        invoke(binary, "archive", "verify", "--archive", rust_archive,
               "--identity", identity, expect_success=False)

        # Actual encrypted traversal archive must fail both preflight listing
        # and restore; the destination must remain absent.
        evil = root / "evil.tar.gz.age"
        stream = io.BytesIO()
        with tarfile.open(fileobj=stream, mode="w:gz") as writer:
            data = b"bad"
            item = tarfile.TarInfo("../../escape.txt")
            item.size = len(data)
            writer.addfile(item, io.BytesIO(data))
        with evil.open("wb") as out:
            process = subprocess.run(["age", "-r", recipient], input=stream.getvalue(),
                                     stdout=out, stderr=subprocess.PIPE, timeout=30)
        assert process.returncode == 0
        invoke(binary, "archive", "list", "--archive", evil,
               "--identity", identity, expect_success=False)
        evil_dest = root / "evil_extraction"
        invoke(binary, "archive", "extract", "--archive", evil,
               "--identity", identity, "--destination", evil_dest,
               expect_success=False)
        assert not evil_dest.exists()
        assert not (root.parent / "escape.txt").exists()

        # The read-only Rust checkpoint auditor must detect modified files.
        legacy.write_bytes(b"changed after journal verification")
        invoke(binary, "checkpoint", "audit", "--file",
               root / ".state" / "active.json", "--json",
               expect_success=False)
        # No unencrypted whole-archive staging is permitted.
        assert not list(root.rglob("*.tar.gz"))
    print("REAL age/tar encrypted Rust↔Python interoperability: PASS", flush=True)


if __name__ == "__main__":
    main()
