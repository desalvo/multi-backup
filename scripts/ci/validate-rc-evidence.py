#!/usr/bin/env python3
"""Fail-closed RC artifact provenance from a completed SAME-SHA production CI.

Only consumes GitHub REST JSON and the existing Rust-only release collector's
authoritative matrix. No file/network mutations and no synthetic PASS.
"""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import runpy
import sys

SOURCE_SHA = "1f1a726a3ffed7b46a7498b99891bfb935570865"
RUN_ID = 43932195582
PREVIEW_SKIP = {
    "Push Multi Backup OCI image to Docker Hub",
    "macOS unsigned app startup (${{ matrix.arch }}, never distributed)",
    "Native packages and publication (after Production Gate and macOS smoke)",
}
CORE = {
    "Production Gate - tests, security and release policy",
    "Rust web server, storage and OCI image build verification",
    "S3 vault - real MinIO Object Lock roundtrip",
    "Standalone Rust-only GUI build and real graphical startup",
    "Native GUI screenshots and EN/IT PDF publication smoke",
    "Real Kubernetes multi-node control plane and PostgreSQL TLS acceptance",
    "Python-free Rust Windows x64 setup",
    "Rust Agent Windows x64 EXE",
    "Python-free Rust macOS DMG (arm64)",
    "Python-free Rust macOS DMG (x86_64)",
    "Rust Agent macOS arm64 PKG",
    "Rust Agent macOS x86_64 PKG",
}
EXPECTED_SUCCESS = CORE | {
    f"{label} Debian {version} {arch} DEB"
    for label in ("Rust", "Rust Agent")
    for version in (12, 13) for arch in ("amd64", "arm64")
} | {
    f"{label} {distro} EL{el} RPM"
    for label in ("Python-free Rust", "Rust Agent")
    for distro in ("rocky", "alma") for el in (8, 9, 10)
}
assert len(EXPECTED_SUCCESS) == 32

def required_artifacts():
    collector = Path(__file__).resolve().with_name("collect-rust-release.py")
    catalog = runpy.run_path(str(collector))
    return set(catalog["GUI"]) | set(catalog["AGENT"])

def validate(run, jobs, artifacts, *, sha=SOURCE_SHA, run_id=RUN_ID):
    if (run.get("id") != run_id or run.get("head_sha") != sha
        or run.get("status") != "completed" or run.get("conclusion") != "success"):
        raise ValueError("Production run is not a completed SUCCESS at pinned SHA")
    names = {}
    for job in jobs.get("jobs", []):
        name = job.get("name")
        if not isinstance(name, str) or name in names:
            raise ValueError("Missing or duplicate CI job name")
        names[name] = job.get("conclusion")
    if set(names) != EXPECTED_SUCCESS | PREVIEW_SKIP:
        raise ValueError(f"CI jobs mismatch: missing={sorted((EXPECTED_SUCCESS | PREVIEW_SKIP)-set(names))} unexpected={sorted(set(names)-(EXPECTED_SUCCESS | PREVIEW_SKIP))}")
    if any(names[name] != "success" for name in EXPECTED_SUCCESS):
        raise ValueError("A required build/security/cluster job is not SUCCESS")
    if any(names[name] != "skipped" for name in PREVIEW_SKIP):
        raise ValueError("Forbidden publishing/legacy job unexpectedly executed")
    found = {}
    for a in artifacts.get("artifacts", []):
        name = a.get("name")
        if not isinstance(name, str) or name in found:
            raise ValueError("Duplicated or absent artifact names")
        if (a.get("expired") is not False or
            a.get("size_in_bytes", 0) <= 0 or
            a.get("workflow_run", {}).get("head_sha") not in (None, sha)):
            raise ValueError(f"Untrusted, empty, stale or expired artifact {name}")
        found[name] = a
    required = required_artifacts()
    if not required <= set(found):
        raise ValueError(f"Incomplete native Rust artifacts: {sorted(required-set(found))}")
    # The only extra in the reference full CI is the publications PDF/screenshot bundle.
    if set(found) - required != {"native-gui-and-about-preview"}:
        raise ValueError(f"Unexpected non-Rust artifacts: {sorted(set(found)-required)}")
    return {"source_sha": sha, "ci_run_id": run_id,
            "required_jobs": len(EXPECTED_SUCCESS),
            "native_artifact_groups": len(required),
            "stable_certified": False, "notarized": False}

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--run", required=True, type=Path)
    p.add_argument("--jobs", required=True, type=Path)
    p.add_argument("--artifacts", required=True, type=Path)
    p.add_argument("--source-sha", default=SOURCE_SHA)
    args = p.parse_args()
    result = validate(*(json.loads(path.read_text(encoding="utf-8"))
                        for path in (args.run, args.jobs, args.artifacts)),
                      sha=args.source_sha)
    print(json.dumps({"rc_provenance": "PASS", **result}, sort_keys=True))

if __name__ == "__main__":
    try:
        main()
    except (OSError, KeyError, TypeError, ValueError) as exc:
        print(f"RC provenance FAIL: {exc}", file=sys.stderr)
        raise SystemExit(1) from None
