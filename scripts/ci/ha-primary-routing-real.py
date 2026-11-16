#!/usr/bin/env python3
"""Real loopback HAProxy /primary TLS-routing regression.

Uses real TLS, signed/mismatched X.509 leaf certificates, HTTP probes and TCP
backends, but synthetic Patroni responses. This does NOT certify Patroni/etcd
leader fencing or distributed storage.
"""
import http.server
from pathlib import Path
import socket
import socketserver
import ssl
import subprocess
import tempfile
import threading
import time


ROOT = Path(__file__).resolve().parents[2]
TEMPLATE = ROOT / "packaging/ha/haproxy-postgres.cfg.example"


class ThreadHTTP(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True
    allow_reuse_address = True


class ThreadTCP(socketserver.ThreadingMixIn, socketserver.TCPServer):
    daemon_threads = True
    allow_reuse_address = True


def command(*args):
    subprocess.run(args, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)


def make_leaf(root, index, ca_cert, ca_key, valid):
    name = f"mb-node{index}.internal"
    key = root / f"node{index}.key"
    csr = root / f"node{index}.csr"
    cert = root / f"node{index}.crt"
    ext = root / f"node{index}.ext"
    ext.write_text(
        "basicConstraints=critical,CA:FALSE\n"
        "keyUsage=critical,digitalSignature,keyEncipherment\n"
        "extendedKeyUsage=serverAuth\n"
        f"subjectAltName=DNS:{name if valid else 'wrong-san.invalid'}\n"
    )
    command("openssl", "req", "-new", "-newkey", "rsa:2048", "-nodes",
            "-subj", f"/CN={name}", "-keyout", str(key), "-out", str(csr))
    command("openssl", "x509", "-req", "-in", str(csr),
            "-CA", str(ca_cert), "-CAkey", str(ca_key), "-CAcreateserial",
            "-days", "1", "-sha256", "-extfile", str(ext), "-out", str(cert))
    return cert, key


def start_servers(root, primary):
    ca_key, ca_cert = root / "ca.key", root / "ca.crt"
    command("openssl", "req", "-new", "-x509", "-newkey", "rsa:2048", "-nodes",
            "-sha256", "-days", "1", "-subj", "/CN=Multi Backup HA CI CA",
            "-addext", "basicConstraints=critical,CA:TRUE",
            "-addext", "keyUsage=critical,keyCertSign,cRLSign",
            "-keyout", str(ca_key), "-out", str(ca_cert))
    servers = []
    backends = []
    checks = []
    for index in (1, 2, 3):
        cert, key = make_leaf(root, index, ca_cert, ca_key, index != 3)

        def handler_for(node):
            class Health(http.server.BaseHTTPRequestHandler):
                def do_GET(self):
                    good = self.path == "/primary" and primary[0] == node
                    # All nodes report 200 for /health, intentionally proving
                    # the proxy must use /primary, never generic liveness.
                    status = 200 if good or self.path == "/health" else 503
                    self.send_response(status)
                    self.end_headers()
                    self.wfile.write(b"primary" if good else b"standby")

                def log_message(self, *_args):
                    return

            return Health

        def backend_for(node):
            class Backend(socketserver.BaseRequestHandler):
                def handle(self):
                    self.request.settimeout(3)
                    self.request.recv(64)
                    self.request.sendall(f"node-{node}\n".encode())

            return Backend

        health = ThreadHTTP(("127.0.0.1", 0), handler_for(index))
        ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        ctx.minimum_version = ssl.TLSVersion.TLSv1_2
        ctx.load_cert_chain(str(cert), str(key))
        health.socket = ctx.wrap_socket(health.socket, server_side=True)
        backend = ThreadTCP(("127.0.0.1", 0), backend_for(index))
        for server in (health, backend):
            servers.append(server)
            threading.Thread(target=server.serve_forever, daemon=True).start()
        checks.append(health.server_address[1])
        backends.append(backend.server_address[1])
    return ca_cert, servers, backends, checks


def available_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def render_config(root, ca_cert, backends, checks, port):
    original = TEMPLATE.read_text()
    assert original.count("bind 127.0.0.1:6432") >= 1
    config = original.replace("bind 127.0.0.1:6432",
                              f"bind 127.0.0.1:{port}")
    config = config.replace("/etc/multi-backup/secrets/patroni-ca.pem",
                            str(ca_cert))
    config = config.replace("inter 2s fall 3 rise 2",
                            "inter 250ms fall 2 rise 2")
    lines = []
    for line in config.splitlines():
        for i in (1, 2, 3):
            if line.lstrip().startswith(f"server mb-node{i} "):
                line = line.replace(f"10.70.0.1{i}:5432",
                                    f"127.0.0.1:{backends[i - 1]}")
                line = line.replace("check port 8008",
                                    f"check port {checks[i - 1]}")
        lines.append(line)
    target = root / "haproxy-ci.cfg"
    target.write_text("\n".join(lines) + "\n")
    command("haproxy", "-c", "-f", str(target))
    return target


def tcp_probe(port):
    try:
        with socket.create_connection(("127.0.0.1", port), timeout=1.5) as sock:
            sock.settimeout(1.5)
            sock.sendall(b"HA client probe\n")
            message = sock.recv(64)
            return message.decode().strip() if message else None
    except OSError:
        return None


def await_route(port, expected, proxy, seconds=12):
    stop = time.monotonic() + seconds
    observed = None
    while time.monotonic() < stop:
        if proxy.poll() is not None:
            raise AssertionError(f"HAProxy exited early with {proxy.returncode}")
        observed = tcp_probe(port)
        if observed == expected:
            return
        time.sleep(0.25)
    raise AssertionError(f"Expected {expected!r} but HAProxy routed to {observed!r}")


def main():
    if not TEMPLATE.is_file():
        raise RuntimeError("Missing production HAProxy configuration template")
    with tempfile.TemporaryDirectory(prefix="mb-haproxy-ci-") as tmp:
        root = Path(tmp)
        primary = [1]
        servers = []
        proxy = None
        try:
            ca, servers, backends, checks = start_servers(root, primary)
            port = available_port()
            cfg = render_config(root, ca, backends, checks, port)
            proxy = subprocess.Popen(
                ["haproxy", "-db", "-f", str(cfg)],
                stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
            await_route(port, "node-1", proxy)
            primary[0] = 2
            await_route(port, "node-2", proxy)
            primary[0] = 0
            await_route(port, None, proxy)
            primary[0] = 3
            # node3 deliberately presents a CA-signed cert with the WRONG SAN.
            # Even when its /primary returns HTTP 200, the proxy must refuse.
            await_route(port, None, proxy)
            primary[0] = 1
            await_route(port, "node-1", proxy)
            print("PASS real HAProxy: TLS-authenticated /primary routing, "
                  "primary failover, no writable fallback and wrong-SAN refusal")
            print("Scope: synthetic Patroni health responders; real HA requires "
                  "3-node etcd quorum, PostgreSQL promotion and fencing")
        finally:
            if proxy is not None:
                proxy.terminate()
                try:
                    proxy.communicate(timeout=5)
                except subprocess.TimeoutExpired:
                    proxy.kill()
                    proxy.communicate()
            for server in servers:
                server.shutdown()
                server.server_close()


if __name__ == "__main__":
    main()
