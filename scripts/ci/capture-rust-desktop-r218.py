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
                path=output/name
                # Software GL drivers can require several frames to initialize.
                # Retry rendering, but NEVER accept a blank or fabricated image.
                ready=False
                for attempt in range(4):
                    time.sleep(3.2 if attempt==0 else 2.0)
                    if proc.poll() is not None:
                        raise RuntimeError(f"Rust UI exited on page {page}: {proc.stderr.read()[-2000:]!r}")
                    subprocess.run(["import","-window","root","-silent",str(path)],
                        check=True,timeout=25)
                    with Image.open(path) as image:
                        if image.width<1000 or image.height<700:
                            raise RuntimeError(f"Unexpected small Xvfb screenshot: {path}")
                        gray=image.convert("L").resize((60,40))
                        # Count distinct pixels without deprecated Image.getdata.
                        colors=gray.getcolors(maxcolors=256)
                        ready=colors is not None and len(colors)>=14
                    if ready:
                        break
                if not ready:
                    proc.terminate()
                    try:
                        _,diagnostic=proc.communicate(timeout=6)
                    except subprocess.TimeoutExpired:
                        proc.kill()
                        _,diagnostic=proc.communicate(timeout=6)
                    raise RuntimeError(
                        f"Blank/unrendered real Rust UI page {page} after retries: "
                        f"{diagnostic[-3500:]!r}")
                print("CAPTURED real Rust desktop",page,path,flush=True)
            finally:
                proc.terminate()
                try:proc.communicate(timeout=6)
                except subprocess.TimeoutExpired:
                    proc.kill();proc.communicate(timeout=6)
if __name__=="__main__":main()
