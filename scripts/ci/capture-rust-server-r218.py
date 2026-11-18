#!/usr/bin/env python3
"""Screenshot the checked-out REAL Rust-server frontend with isolated fake API data.

No fake HTML, CSS or controls: served assets come straight from rust-server/public.
All API fixture data is explicitly synthetic; screenshots are marked as TEST FIXTURE
in PDF captions and names. No live account, credentials or provider calls.
"""
from __future__ import annotations
import functools
import http.server
import json
import os
from pathlib import Path
import sys
import threading
from urllib.parse import urlsplit
from playwright.sync_api import sync_playwright

ROOT=Path(__file__).resolve().parents[2]
PUBLIC=ROOT/"rust-server"/"public"

class Static(http.server.SimpleHTTPRequestHandler):
    def __init__(self,*args,**kwargs):
        super().__init__(*args,directory=str(PUBLIC),**kwargs)
    def log_message(self,*_args):
        pass

def fixture(route):
    path=urlsplit(route.request.url).path
    if path=="/api/backups":
        data={"backups":[
            {"id":"0000000000000000000000000000000000000000000000000000000000000000",
             "label":"DEMO: example backup, not a real job",
             "files":[{"path":"github/example/README.md","size":1024},
                      {"path":"documents/example.txt","size":256}]}]}
        route.fulfill(status=200,content_type="application/json",body=json.dumps(data))
    elif path=="/api/cloud/remotes":
        route.fulfill(status=200,content_type="application/json",body=json.dumps({
          "providers":["dropbox","google-drive","icloud","onedrive","owncloud","nextcloud","box"],
          "write_enabled":False,"delete_enabled":False,"cloud_recovery_enabled":False,
          "mount_is_local_desktop_only":True}))
    elif path=="/api/cloud/list":
        route.fulfill(status=200,content_type="application/json",body=json.dumps({
          "provider":"dropbox","directory":"","entries":[
              {"name":"Project files (DEMO)","is_dir":True,"size":None,"modified":None},
              {"name":"example-backup.age","is_dir":False,"size":2048,"modified":None}]}))
    else:
        route.fulfill(status=403,content_type="application/json",body='{"error":"documentation_fixture_no_real_account"}')

def screenshot(page,section,path):
    page.locator(section).scroll_into_view_if_needed(timeout=10000)
    page.wait_for_timeout(250)
    page.screenshot(path=str(path),full_page=False,animations="disabled")
    if path.stat().st_size<12000:
        raise RuntimeError("Screenshot not actually rendered: "+str(path))

def main():
    if len(sys.argv)!=2:raise SystemExit("Usage: capture-rust-server-r218.py OUTPUT")
    if os.getenv("GITHUB_ACTIONS")!="true":
        raise SystemExit("Refusing untrusted screenshot source outside CI fixture")
    target=Path(sys.argv[1]).resolve();target.mkdir(parents=True,exist_ok=True)
    server=http.server.ThreadingHTTPServer(("127.0.0.1",0),Static)
    thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
    url=f"http://127.0.0.1:{server.server_port}/index.html"
    try:
        with sync_playwright() as playwright:
            browser=playwright.chromium.launch(headless=True,
                args=["--no-sandbox","--disable-dev-shm-usage"])
            try:
                desktop=browser.new_page(viewport={"width":1440,"height":900},
                    device_scale_factor=1)
                desktop.route("**/api/**",fixture)
                desktop.goto(url,wait_until="domcontentloaded")
                desktop.wait_for_timeout(1200)
                screenshot(desktop,"#archives",target/"web-archives-fixture.png")
                screenshot(desktop,"#cloud-drives",target/"web-cloud-drives-fixture.png")
                screenshot(desktop,"#cluster-overview",target/"web-cluster-fixture.png")
                phone=browser.new_page(viewport={"width":390,"height":844},
                    device_scale_factor=1,is_mobile=True,has_touch=True)
                phone.route("**/api/**",fixture)
                phone.goto(url,wait_until="domcontentloaded")
                phone.wait_for_timeout(1000)
                screenshot(phone,"#cloud-drives",target/"mobile-cloud-drive-fixture.png")
                print("CAPTURED authentic Rust server frontend DOM/CSS/JS in documented API fixture",
                      flush=True)
            finally:browser.close()
    finally:
        server.shutdown()
        server.server_close()
if __name__=="__main__":main()
