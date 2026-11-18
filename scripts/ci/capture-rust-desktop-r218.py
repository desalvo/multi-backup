#!/usr/bin/env python3
"""Capture pixels of the ACTUAL current Rust desktop egui process in isolated Xvfb.

Never alter these screenshots by drawing synthetic controls or fake job states.
GUI is launched with no credentials, temp HOME and network secrets absent.
Requires Xvfb display, ImageMagick import and built Rust desktop binary.
"""
import os
import pathlib
import subprocess
import sys
import tempfile
import time
from PIL import Image

ROOT=pathlib.Path(__file__).resolve().parents[2]
BIN=ROOT/"rust-desktop/target/debug/multi-backup-desktop"
PAGES={"Dashboard":"desktop-dashboard.png",
       "Backup":"desktop-backup.png",
       "Archivi":"desktop-archives.png",
       "Ripristina":"desktop-restore.png",
       "Plugin":"desktop-plugins.png",
       "Server e agenti":"desktop-server.png",
       "Vault e chiavi":"desktop-vault.png"}
def main():
    if len(sys.argv)!=2:
        raise SystemExit("Usage: capture-rust-desktop-r218.py SCREENSHOT_DIR")
    if os.getenv("GITHUB_ACTIONS")!="true" or os.geteuid()==0:
        raise SystemExit("Refusing non-CI or root documentation capture")
    if not BIN.is_file() or not os.getenv("DISPLAY"):
        raise SystemExit("Needs genuine Rust binary and running Xvfb")
    output=pathlib.Path(sys.argv[1]).resolve()
    output.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="mb-docs-profile-") as directory:
        home=pathlib.Path(directory)/"home";home.mkdir(mode=0o700)
        cfg=pathlib.Path(directory)/"config";cfg.mkdir(mode=0o700)
        env=os.environ.copy()
        env.update(HOME=str(home),XDG_CONFIG_HOME=str(cfg),
                   MULTI_BACKUP_DOCUMENTATION_CAPTURE="1",
                   LIBGL_ALWAYS_SOFTWARE="1",NO_AT_BRIDGE="1")
        for page,name in PAGES.items():
            proc=subprocess.Popen([str(BIN),"--docs-page",page],env=env,
                stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
            try:
                time.sleep(3.2)
                if proc.poll() is not None:
                    raise RuntimeError(f"Rust UI exited on page {page}: {proc.stderr.read()[-2000:]!r}")
                path=output/name
                subprocess.run(["import","-window","root","-silent",str(path)],
                    check=True,timeout=25)
                with Image.open(path) as image:
                    if image.width<1000 or image.height<700:
                        raise RuntimeError(f"Unexpected small Xvfb screenshot: {path}")
                    # Reject all-blank output, not merely existence of a PNG.
                    gray=image.convert("L").resize((60,40))
                    if len(set(gray.getdata()))<14:
                        raise RuntimeError(f"Blank/uncaptured GUI surface: {path}")
                print("CAPTURED real Rust desktop",page,path,flush=True)
            finally:
                proc.terminate()
                try:proc.communicate(timeout=6)
                except subprocess.TimeoutExpired:
                    proc.kill();proc.communicate(timeout=6)
if __name__=="__main__":main()
