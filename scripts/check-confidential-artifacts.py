#!/usr/bin/env python3
"""Reject private architecture artifacts unless the only tracked file is AES256 PDF R6."""
from pathlib import Path
import subprocess
import sys
ROOT=Path(__file__).resolve().parents[1]
ALLOWED="docs/private/architecture-encrypted.pdf"
def tracked_private_paths():
    data=subprocess.run(["git","ls-files","-z","--","docs/private"],cwd=ROOT,
        stdout=subprocess.PIPE,check=True).stdout
    return [v.decode("utf-8") for v in data.split(b"\0") if v]
def validate(paths):
    for name in paths:
        if name!=ALLOWED:
            raise ValueError("Forbidden plaintext/private source path: "+name)
        path=ROOT/name
        if not path.is_file() or path.is_symlink() or path.stat().st_size>15*1024*1024:
            raise ValueError("Private document is missing/unsafe/oversized")
        from pypdf import PdfReader
        doc=PdfReader(str(path),strict=True)
        if not doc.is_encrypted:
            raise ValueError("Unencrypted confidential PDF refused")
        enc=doc.trailer["/Encrypt"].get_object()
        if int(enc.get("/R",0))<6 or int(enc.get("/V",0))<5 or enc.get("/EncryptMetadata") is False:
            raise ValueError("Confidential PDF requires AES-256 R6 and encrypted metadata")
        if doc.decrypt("never-use-this-predictable-check-password-0b8cf")!=0:
            raise ValueError("Confidential PDF can be unlocked by a known bad password")
if __name__=="__main__":
    try:
        paths=tracked_private_paths()
        validate(paths)
        print("Encrypted architecture policy: PASS (%d protected PDFs)"%len(paths))
    except Exception as e:
        print("Encrypted architecture policy: FAIL:",e,file=sys.stderr)
        sys.exit(1)
