#!/usr/bin/env python3
"""Fail-closed documentation coverage for native Rust r038 operations.

This is a source/documentation consistency gate, NOT live-provider or release
certification. Historical r018 manuals are separate from Rust r038 guidance.
"""
from __future__ import annotations
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]

def read(path: str) -> str:
    file = ROOT / path
    if not file.is_file():
        raise AssertionError(f"Missing documentation/source: {path}")
    text = file.read_text(encoding="utf-8")
    if not text.strip():
        raise AssertionError(f"Empty documentation/source: {path}")
    return text

def require(doc: str, *tokens: str) -> None:
    missing = [token for token in tokens if token not in doc]
    if missing:
        raise AssertionError(f"Documentation coverage gap: {missing}")

def main() -> None:
    cli = read("rust/src/bin/multi-backup-native.rs")
    chunk = read("rust/src/chunk_store.rs")
    maint = read("docs/MAINTENANCE_POLICY.md")
    storage = read("docs/CHUNK_STORAGE_V1.md")
    matrix = read("docs/RC_CERTIFICATION_MATRIX.md")
    api = read("docs/PLUGIN_API.md")
    inventory = read("docs/FEATURE_INVENTORY_R038.md")
    index = read("docs/DOCUMENTATION_INDEX_R038.md")
    guide_it = read("docs/OPERATIONS_R038_IT.md")
    guide_en = read("docs/OPERATIONS_R038_EN.md")
    publication = read("scripts/build-publications.py")
    require(cli, "storage init", "storage backup", "storage info",
            "storage verify-chain", "storage delete-backup",
            "storage gc-preview", "storage gc-collect",
            "schedule run", "schedule daemon",
            "restore github", "restore dropbox")
    require(chunk, "pub enum BackupKind", "pub fn delete_backup_rebase",
            "pub fn garbage_collect", "fn repack_mixed_stripes_locked",
            "supersedes: Option<String>", "pub struct GarbageCollectionReport",
            "mixed_stripes_to_repack", "orphan_parity_directories")
    additional_guides = [
        "docs/R038_OPERATIONS_IT.md", "docs/R038_OPERATIONS_EN.md"
    ]
    for path in additional_guides:
        text = read(path)
        require(text, "r038", "repack", "GC")
        if (chr(92)+chr(96)) in text or text.count(chr(96)*3) % 2:
            raise AssertionError(f"Broken Markdown code formatting: {path}")
    for path, guide in [("docs/OPERATIONS_R038_IT.md", guide_it),
                        ("docs/OPERATIONS_R038_EN.md", guide_en)]:
        require(guide, "0.1.0-r038", "storage gc-preview",
                "storage gc-collect", "gc-collect", "age-keygen",
                "storage delete-backup", "storage verify-chain",
                "storage info", "schedule daemon", "schedule run",
                "restore github", "restore dropbox",
                "--confirm ERASE-ORPHANS", "48", "8+2")
        if guide.count(chr(96)*3) % 2 or (chr(92)+chr(96)) in guide:
            raise AssertionError(f"Unclosed or escaped Markdown code fence: {path}")
    for label, doc in [("storage", storage), ("maintenance", maint),
                       ("certification", matrix), ("plugins", api),
                       ("inventory", inventory), ("index", index)]:
        require(doc, "r038", "repack", "GC", "Rust")
    require(index, "OPERATIONS_R038_IT.md", "OPERATIONS_R038_EN.md",
            "FEATURE_INVENTORY_R038.md", "CHUNK_STORAGE_V1.md",
            "RC_CERTIFICATION_MATRIX.md")
    require(inventory, "full", "incremental", "GitHub", "Dropbox",
            "WORM", "not certified", "scheduled")
    require(publication, "RUST_R038_OPERATIONS_APPENDIX",
            "Historical Python/Tk screenshot", "Anteprima Rust r038")
    if len(re.findall(r"\|\s*(?:Implemented|Preview|Planned|Blocked)", inventory)) < 8:
        raise AssertionError("Feature inventory has too few status-backed rows")
    # Broken local hyperlinks are release-blocking documentation defects.
    for path in [
        "docs/OPERATIONS_R038_IT.md", "docs/OPERATIONS_R038_EN.md",
        "docs/R038_OPERATIONS_IT.md", "docs/R038_OPERATIONS_EN.md",
        "docs/DOCUMENTATION_INDEX_R038.md", "docs/FEATURE_INVENTORY_R038.md",
        "docs/CHUNK_STORAGE_V1.md", "docs/MAINTENANCE_POLICY.md",
        "docs/PLUGIN_API.md", "docs/RC_CERTIFICATION_MATRIX.md",
    ]:
        body = read(path)
        for link in re.findall(r"\]\(([^)]+)\)", body):
            value = link.split("#", 1)[0]
            if not value or value.startswith(("https://", "http://", "mailto:", "#")):
                continue
            resolved = (ROOT / Path(path).parent / value).resolve()
            if not resolved.is_relative_to(ROOT.resolve()) or not resolved.exists():
                raise AssertionError(f"Broken local link in {path}: {link}")
    print("Rust r038 documentation contract: PASS")

if __name__ == "__main__":
    main()
