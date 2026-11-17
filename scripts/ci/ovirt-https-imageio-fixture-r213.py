#!/usr/bin/env python3
"""Disposable TLS oVirt Engine+ImageIO protocol fixture; NOT a real Engine."""
import argparse
import json
import os
import re
import ssl
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlsplit

UUID = re.compile(r"^[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}$")

def run():
    p = argparse.ArgumentParser()
    for name in ("cert", "key", "data", "state", "token"):
        p.add_argument("--" + name, required=True)
    p.add_argument("--port", type=int, default=18734)
    a = p.parse_args()
    if not 1024 < a.port < 65535 or len(a.token) < 16:
        raise SystemExit("Bad oVirt fixture port/token")
    payload = Path(a.data).read_bytes()
    if not 0 < len(payload) <= 8 * 1024 * 1024:
        raise SystemExit("Invalid test disk image")

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass  # Never print request headers or secret bearer tokens.

        def mode(self):
            state = json.loads(Path(a.state).read_text())
            assert set(state) <= {"mode"}
            return state.get("mode", "correct")

        def reply(self, code, body, kind="application/json"):
            self.send_response(code)
            self.send_header("Content-Type", kind)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(body)

        def respond(self, obj):
            self.reply(200, json.dumps(obj, sort_keys=True).encode())

        def do_GET(self):
            u = urlsplit(self.path)
            if u.query or u.fragment:
                self.reply(400, b"bad query", "text/plain")
                return
            mode = self.mode()
            image_url = re.fullmatch(r"/images/([0-9a-f-]{36})", u.path)
            if image_url:
                # IMAGEIO must NEVER receive the Engine bearer token.
                if self.headers.get("Authorization"):
                    self.reply(403, b"bearer leak", "text/plain")
                    return
                data = (payload[:-7] if mode == "short-body" else
                        bytes(len(payload)) if mode == "bad-body" else payload)
                self.reply(200, data, "application/octet-stream")
                return
            if (self.headers.get("Authorization") != "Bearer " + a.token or
                self.headers.get("Version") != "4" or
                self.headers.get("Accept") != "application/json"):
                self.reply(401, b"bad authorization", "text/plain")
                return
            backup = re.fullmatch(
                r"/ovirt-engine/api/vms/([0-9a-f-]{36})/backups/([0-9a-f-]{36})",
                u.path)
            if backup:
                vm, backup_id = backup.groups()
                if not UUID.fullmatch(vm) or not UUID.fullmatch(backup_id):
                    self.reply(404, b"bad UUID", "text/plain")
                    return
                disk, image = os.environ["MB_OVIRT_DISK"], os.environ["MB_OVIRT_IMAGE"]
                result = {"vm_backup": {
                    "id": backup_id,
                    "phase": "initializing" if mode == "not-ready" else "ready",
                    "disks": {"disk": [{"id": image if mode == "wrong-disk" else disk,
                                       "image_id": image}]}
                }}
                if mode == "incomplete-chain":
                    result["vm_backup"]["from_checkpoint_id"] = image
                self.respond(result)
                return
            transfer = re.fullmatch(
                r"/ovirt-engine/api/imagetransfers/([0-9a-f-]{36})", u.path)
            if transfer:
                tid = transfer.group(1)
                if not UUID.fullmatch(tid):
                    self.reply(404, b"bad UUID", "text/plain")
                    return
                self.respond({"image_transfer": {
                    "id": tid,
                    "transfer_url": f"https://localhost:{a.port}/images/{tid}",
                    "direction": "upload" if mode == "wrong-direction" else "download",
                    "phase": "transferring",
                    "disk": {"id": os.environ["MB_OVIRT_DISK"]},
                    "backup": {"id": os.environ["MB_OVIRT_BACKUP"]},
                    "format": "raw"
                }})
                return
            self.reply(404, b"no route", "text/plain")

    server = ThreadingHTTPServer(("127.0.0.1", a.port), Handler)
    tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    tls.minimum_version = ssl.TLSVersion.TLSv1_2
    tls.load_cert_chain(a.cert, a.key)
    server.socket = tls.wrap_socket(server.socket, server_side=True)
    server.serve_forever()

if __name__ == "__main__":
    run()
