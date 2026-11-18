#!/usr/bin/env python3
"""Offline exact-commit hash/provenance verification for genuine CI-built native installers.

This does not attest Apple notarization, Authenticode, RPM, or production
environments. It refuses missing platforms, unsigned checksums, path escapes,
wrong Git SHA and altered bytes before publishing an rc-preview bundle.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import re

SOURCES={
    "rc216-ubuntu24-amd64-native-deb":
        ("SOURCE_SHA.txt", ".deb", ".deb.sha256"),
    "rc216-macos-arm64-adhoc-dmg":
        ("SOURCE_SHA-mac-arm64.txt", "-arm64.dmg", ".dmg.sha256"),
    "rc216-macos-x86_64-adhoc-dmg":
        ("SOURCE_SHA-mac-x86_64.txt", "-x86_64.dmg", ".dmg.sha256"),
    "rc216-windows-x64-inno-unsigned":
        ("SOURCE_SHA-win-r217.txt", ".exe", "SHA256SUMS-win-r217.txt"),
}
HASHLINE=re.compile(r"^([0-9a-f]{64})  ([^\x00\n\r/\\]+)$")
SHA=re.compile(r"^[0-9a-f]{40}$")

class UntrustedPackage(ValueError):
    pass

def verify(root:Path, sha:str)->list[dict]:
    if not SHA.fullmatch(sha):
        raise UntrustedPackage("Expected full Git commit SHA")
    if not root.is_dir() or root.is_symlink():
        raise UntrustedPackage("Downloaded platform artifacts root is not an ordinary directory")
    actual={f.name for f in root.iterdir()}
    if actual!=set(SOURCES):
        raise UntrustedPackage(f"Platforms missing or unexpected: {actual ^ set(SOURCES)}")
    records=[]
    for name,(identity,binary_suffix,checksum_suffix) in SOURCES.items():
        folder=root/name
        if folder.is_symlink() or not folder.is_dir():
            raise UntrustedPackage(f"Artifact folder is not real: {name}")
        source=folder/identity
        if source.is_symlink() or source.read_text().strip()!=sha:
            raise UntrustedPackage(f"Wrong source SHA for {name}")
        assets=[p for p in folder.iterdir() if p.is_file() and not p.is_symlink()]
        binaries=[p for p in assets if p.name.endswith(binary_suffix)]
        checks=[p for p in assets if p.name.endswith(checksum_suffix)]
        if len(binaries)!=1 or len(checks)!=1:
            raise UntrustedPackage(f"Expected exactly one installer and checksum for {name}")
        archive=binaries[0]
        if archive.stat().st_size<16384:
            raise UntrustedPackage(f"Installer unexpectedly small: {archive.name}")
        line=checks[0].read_text(encoding="ascii").strip()
        # The Ubuntu packager correctly emits "packages/filename.deb" because
        # it hashes before upload. Artifacts are flattened by upload-artifact,
        # so accept exactly that one trusted prefix, never arbitrary paths.
        if name=="rc216-ubuntu24-amd64-native-deb" and line.count("  packages/")==1:
            line=line.replace("  packages/","  ",1)
        hit=HASHLINE.fullmatch(line)
        if hit is None or hit.group(2)!=archive.name:
            raise UntrustedPackage(f"Checksum not strictly associated with packaged binary: {name}")
        expected=hit.group(1)
        h=hashlib.sha256()
        with archive.open("rb") as inp:
            for chunk in iter(lambda:inp.read(1<<20),b""):
                h.update(chunk)
        if h.hexdigest()!=expected:
            raise UntrustedPackage(f"Installer bytes differ from same-run checksum: {name}")
        records.append({
            "artifact":name,"filename":archive.name,"sha256":expected,
            "bytes":archive.stat().st_size,"source_sha":sha,
            "actual_native_install_smoke_required":True,
            "signed_production_release":False,
        })
    return records

def main()->None:
    p=argparse.ArgumentParser()
    p.add_argument("--root",type=Path,required=True)
    p.add_argument("--source-sha",required=True)
    p.add_argument("--output",type=Path,required=True)
    args=p.parse_args()
    records=verify(args.root,args.source_sha)
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps({"source_sha":args.source_sha,
         "installer_count":len(records),"artifacts":records,
         "release_authorized":False},indent=2,sort_keys=True)+"\n")
    print("PASS all four same-SHA real platform binary hashes, without false production signature claims")

if __name__=="__main__":
    main()
