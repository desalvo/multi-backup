#!/usr/bin/env python3
"""Real disposable Patroni + 3-member TLS etcd + PostgreSQL16 HA acceptance.

Runs ONLY on a GitHub-hosted ephemeral worker. All 3 independent PostgreSQL
processes and 3 real TLS etcd members share the runner's loopback network:
functional leader-election / synchronous replication / fencing test, NOT a
three-host physical or Kubernetes network-partition qualification.
"""
import contextlib
import os
import secrets
from pathlib import Path
import shutil
import ssl
import subprocess
import tempfile
import time
import urllib.error
import urllib.request


ADDRESSES = ["127.0.0.11", "127.0.0.12", "127.0.0.13"]
PGPORTS = [15431, 15432, 15433]
RESTPORTS = [18081, 18082, 18083]
# Ephemeral credentials for each isolated HA CI execution, never reusable.
DB_PASSWORD = secrets.token_urlsafe(32)
REPL_PASSWORD = secrets.token_urlsafe(32)


def isolated_env():
    """Never proxy 127/8 peer, REST or etcdctl mTLS traffic via runner proxies."""
    env = os.environ.copy()
    for key in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY",
                "http_proxy", "https_proxy", "all_proxy"):
        env.pop(key, None)
    excluded = "localhost,127.0.0.1,127.0.0.11,127.0.0.12,127.0.0.13,127.0.0.0/8"
    env["NO_PROXY"] = excluded
    env["no_proxy"] = excluded
    return env


def run(*cmd, check=True, timeout=20, env=None):
    return subprocess.run(cmd, check=check, timeout=timeout,
                          env=env if env is not None else isolated_env(),
                          text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def issue_cert(directory, index, ca, ca_key):
    address = ADDRESSES[index]
    root = Path(directory)
    key, csr, cert, ext = [root / f"node-{index}.{s}" for s in ("key", "csr", "crt", "ext")]
    ext.write_text("basicConstraints=critical,CA:FALSE\n"
                   "keyUsage=critical,digitalSignature,keyEncipherment\n"
                   "extendedKeyUsage=serverAuth,clientAuth\n"
                   # All three processes share one Linux loopback routing table:
                   # when dialing 127.0.0.1X the source IP is 127.0.0.1.
                   # etcd validates the peer certificate against the SOURCE
                   # IP; add that source SAN only in this isolated CI lab.
                   f"subjectAltName=IP:{address},IP:127.0.0.1,DNS:mb-node{index+1}.internal\n")
    run("openssl", "req", "-new", "-newkey", "rsa:2048", "-nodes",
        "-subj", f"/CN=mb-node{index+1}.internal",
        "-keyout", str(key), "-out", str(csr))
    run("openssl", "x509", "-req", "-sha256", "-days", "1",
        "-in", str(csr), "-CA", str(ca), "-CAkey", str(ca_key),
        "-CAcreateserial", "-extfile", str(ext), "-out", str(cert))
    key.chmod(0o600)
    return cert, key


def wait_for(label, checker, processes, seconds=90):
    deadline = time.monotonic() + seconds
    last = None
    while time.monotonic() < deadline:
        for name, proc in processes.items():
            if proc.poll() is not None:
                raise RuntimeError(f"{label}: {name} exited with {proc.returncode}")
        try:
            value = checker()
            # Node index 0 is a valid leader/synchronous-standby ID.
            # Truthiness would silently discard it and time out; keep
            # boolean False and missing None as the only retry signals.
            if value is not None and value is not False:
                return value
        except (OSError, subprocess.SubprocessError, ValueError) as ex:
            last = str(ex)
        time.sleep(1)
    raise RuntimeError(f"{label} not established; last={last}")


def etcd_command(root, index=0):
    return ["etcdctl", f"--endpoints=https://{ADDRESSES[index]}:2379",
            f"--cacert={root}/ca.crt", f"--cert={root}/node-0.crt",
            f"--key={root}/node-0.key"]


def health(root, indices=(0, 1, 2)):
    result = run(*etcd_command(root), "endpoint", "health", check=False, timeout=6)
    return result.returncode == 0


def role(root, index, endpoint):
    ctx = ssl.create_default_context(cafile=str(root / "ca.crt"))
    try:
        with urllib.request.urlopen(
                f"https://{ADDRESSES[index]}:{RESTPORTS[index]}/{endpoint}",
                context=ctx, timeout=2) as response:
            return response.status == 200
    except (urllib.error.URLError, TimeoutError, OSError):
        return False


def primaries(root, live=(0, 1, 2)):
    return [index for index in live if role(root, index, "primary")]


def dcs_sync_candidate(root, leader):
    """Get an authoritative Patroni DCS-qualified synchronous replica.

    /sync returns HTTP 200 only for a replica that Patroni currently
    recognizes as synchronous. On this real three-member TLS cluster we do
    not infer availability from PostgreSQL sync_state or from /cluster's
    optional role/state representation. Safe shutdown requires this proof.
    """
    candidates = [index for index in range(3)
                  if index != leader and role(root, index, "sync")]
    if not candidates:
        return None
    return candidates[0]

def pg(root, index, sql, *, expect_ok=True):
    env = os.environ.copy()
    env.update({"PGPASSWORD": DB_PASSWORD, "PGSSLMODE": "verify-full",
                "PGSSLROOTCERT": str(root / "ca.crt"), "PGCONNECT_TIMEOUT": "3"})
    args = ["psql", "-X", "-q", "-t", "-A", "-v", "ON_ERROR_STOP=1",
            "-h", ADDRESSES[index], "-p", str(PGPORTS[index]),
            "-U", "postgres", "-d", "postgres", "-c", sql]
    result = run(*args, check=False, timeout=9, env=env)
    if expect_ok and result.returncode != 0:
        raise RuntimeError("TLS PostgreSQL query failed: " + result.stderr[:300])
    return result


def wait_primary(root, live, procs, seconds=110):
    def choose():
        elected = primaries(root, live)
        if len(elected) != 1:
            return None
        candidate = elected[0]
        writer = pg(root, candidate,
                    "SELECT NOT pg_is_in_recovery()",
                    expect_ok=False)
        if writer.returncode == 0 and writer.stdout.strip() == "t":
            return candidate
        return None
    return wait_for("one elected writable Patroni primary", choose, procs, seconds)


def start(log, *cmd):
    log.parent.mkdir(exist_ok=True, parents=True)
    handle = log.open("a")
    try:
        proc = subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=handle,
                                stderr=subprocess.STDOUT, start_new_session=True,
                                env=isolated_env())
    finally:
        handle.close()
    return proc


def conf(root, index):
    ip = ADDRESSES[index]
    number = index + 1
    data = root / f"postgres-{number}"
    patroni_dir = root / f"patroni-{number}"
    patroni_dir.mkdir(mode=0o700, exist_ok=True)
    ca = root / "ca.crt"
    cert = root / f"node-{index}.crt"
    key = root / f"node-{index}.key"
    path = root / f"patroni-{number}.yaml"
    path.write_text(f"""scope: multi-backup-disposable-ha
namespace: /ci-ha/
name: mb-node{number}
restapi:
  listen: {ip}:{RESTPORTS[index]}
  connect_address: {ip}:{RESTPORTS[index]}
  certfile: {cert}
  keyfile: {key}
  cafile: {ca}
etcd3:
  hosts: {','.join(f'{host}:2379' for host in ADDRESSES)}
  protocol: https
  cacert: {ca}
  cert: {cert}
  key: {key}
bootstrap:
  dcs:
    ttl: 15
    loop_wait: 2
    retry_timeout: 3
    maximum_lag_on_failover: 0
    synchronous_mode: true
    synchronous_mode_strict: true
    synchronous_node_count: 1
    postgresql:
      use_pg_rewind: true
      use_slots: true
      parameters:
        wal_level: replica
        max_wal_senders: 10
        max_replication_slots: 10
        hot_standby: 'on'
        synchronous_commit: remote_apply
        ssl: 'on'
  initdb:
    - encoding: UTF8
    - data-checksums
  pg_hba:
    - local all all trust
    - hostssl replication repl 127.0.0.0/8 scram-sha-256
    - hostssl all all 127.0.0.0/8 scram-sha-256
postgresql:
  listen: {ip}:{PGPORTS[index]}
  connect_address: {ip}:{PGPORTS[index]}
  data_dir: {data}
  bin_dir: /usr/lib/postgresql/16/bin
  pgpass: {patroni_dir}/.pgpass
  authentication:
    replication:
      username: repl
      password: {REPL_PASSWORD}
    superuser:
      username: postgres
      password: {DB_PASSWORD}
    rewind:
      username: postgres
      password: {DB_PASSWORD}
  parameters:
    unix_socket_directories: {patroni_dir}
    ssl: 'on'
    ssl_cert_file: {cert}
    ssl_key_file: {key}
    ssl_ca_file: {ca}
    password_encryption: scram-sha-256
""")
    path.chmod(0o600)
    return path, data


def main():
    if (os.environ.get("GITHUB_ACTIONS") != "true" or
            os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted" or
            os.geteuid() == 0):
        raise SystemExit("REFUSED: only disposable unprivileged GitHub-hosted CI")
    for executable in ("etcd", "etcdctl", "patroni", "psql", "openssl"):
        if not shutil.which(executable):
            raise SystemExit(f"Missing HA lab executable: {executable}")
    if not Path("/usr/lib/postgresql/16/bin/pg_ctl").exists():
        raise SystemExit("PostgreSQL16 server binary missing")
    # Also disable proxies for the in-process urllib Patroni REST probe.
    clean = isolated_env()
    for key in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY",
                "http_proxy", "https_proxy", "all_proxy"):
        os.environ.pop(key, None)
    os.environ["NO_PROXY"] = clean["NO_PROXY"]
    os.environ["no_proxy"] = clean["no_proxy"]
    with tempfile.TemporaryDirectory(prefix="mb-real-ha-") as tmp:
        root = Path(tmp)
        processes = {}
        etcd = {}
        patroni = {}
        data_dirs = {}
        try:
            ca, ca_key = root / "ca.crt", root / "ca.key"
            run("openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes",
                "-sha256", "-days", "1", "-subj", "/CN=MultiBackupEphemeralHA-CA",
                "-addext", "basicConstraints=critical,CA:TRUE",
                "-addext", "keyUsage=critical,keyCertSign,cRLSign",
                "-keyout", str(ca_key), "-out", str(ca))
            for i in range(3):
                issue_cert(root, i, ca, ca_key)
            cluster = ",".join(
                f"mb-etcd{i+1}=https://{ADDRESSES[i]}:2380" for i in range(3))
            for i in range(3):
                ip = ADDRESSES[i]
                name = f"mb-etcd{i+1}"
                args = ["etcd", f"--name={name}",
                        f"--data-dir={root / name}",
                        f"--listen-client-urls=https://{ip}:2379",
                        f"--advertise-client-urls=https://{ip}:2379",
                        f"--listen-peer-urls=https://{ip}:2380",
                        f"--initial-advertise-peer-urls=https://{ip}:2380",
                        f"--initial-cluster={cluster}",
                        "--initial-cluster-state=new",
                        "--initial-cluster-token=multi-backup-disposable-ci",
                        "--client-cert-auth=true", f"--trusted-ca-file={ca}",
                        f"--cert-file={root / f'node-{i}.crt'}",
                        f"--key-file={root / f'node-{i}.key'}",
                        "--peer-client-cert-auth=true",
                        f"--peer-trusted-ca-file={ca}",
                        f"--peer-cert-file={root / f'node-{i}.crt'}",
                        f"--peer-key-file={root / f'node-{i}.key'}",
                        "--log-level=warn"]
                etcd[i] = start(root / f"etcd-{i}.log", *args)
                processes[f"etcd{i}"] = etcd[i]
            # Check genuine peer TLS handshakes before waiting for Raft
            # to elect a leader. Diagnostics contain no production secrets.
            time.sleep(3)
            for i, ip in enumerate(ADDRESSES):
                probe = run("openssl", "s_client", "-connect", f"{ip}:2380",
                    "-cert", str(root / "node-0.crt"),
                    "-key", str(root / "node-0.key"),
                    "-CAfile", str(ca), "-verify_return_error", "-brief",
                    check=False, timeout=6)
                print(f"CI etcd peer {i+1} handshake: exit={probe.returncode}; "
                      f"details={probe.stderr[-350:]}", flush=True)
            wait_for("genuine 3-member TLS etcd quorum", lambda: health(root),
                     processes, 75)
            for i in range(3):
                path, data = conf(root, i)
                data_dirs[i] = data
                patroni[i] = start(root / f"patroni-{i}.log", "patroni", str(path))
                processes[f"patroni{i}"] = patroni[i]
            initial = wait_primary(root, (0, 1, 2), processes, 130)
            candidate = wait_for("DCS registered synchronous failover candidate",
                lambda: dcs_sync_candidate(root, initial), processes, 110)
            print(f"Patroni sync candidate acknowledged in DCS: mb-node{candidate+1}",
                  flush=True)
            # Validate that this is an actual PostgreSQL synchronous sender,
            # not just a stale DCS state; do not crash an unqualified leader.
            confirmed = pg(root, initial,
                "SELECT count(*) FROM pg_stat_replication WHERE sync_state='sync'")
            if not confirmed.stdout.strip().isdigit() or int(confirmed.stdout.strip()) < 1:
                raise RuntimeError("DCS candidate has no PostgreSQL synchronous sender")
            pg(root, initial, "CREATE TABLE ha_witness(id integer PRIMARY KEY, payload text NOT NULL);")
            pg(root, initial, "INSERT INTO ha_witness VALUES (1,'acknowledged-before-failover');")
            # Ensure DCS still owns the candidate after the committed write;
            # a transient pg_stat_replication 'sync' does not prove safe
            # promotion on a Patroni 3.2.2 loop interval.
            if dcs_sync_candidate(root, initial) != candidate:
                raise RuntimeError("DCS sync candidate changed; refuse unsafe leader kill")
            print("Witness committed on DCS-confirmed synchronous primary", flush=True)
            # Crash the original leader and its database, simulating node loss.
            patroni[initial].kill()
            patroni[initial].wait(timeout=6)
            processes.pop(f"patroni{initial}", None)
            run("/usr/lib/postgresql/16/bin/pg_ctl", "-D",
                str(data_dirs[initial]), "stop", "-m", "immediate",
                check=False, timeout=20)
            survivors = tuple(i for i in range(3) if i != initial)
            successor = wait_primary(root, survivors, processes, 90)
            if successor == initial:
                raise RuntimeError("Original dead primary was not fenced")
            # Proven committed data MUST exist on the automatically
            # promoted primary, independently of whether it already has a
            # replacement synchronous sender (strict mode may block writes).
            if pg(root, successor, "SELECT payload FROM ha_witness WHERE id=1").stdout.strip() != (
                    "acknowledged-before-failover"):
                raise RuntimeError("Acknowledged synchronous transaction missing after promotion")
            print("PASS: acknowledged PG row survived automatic primary promotion",
                  flush=True)
            if pg(root, initial, "SELECT 1", expect_ok=False).returncode == 0:
                raise RuntimeError("Terminated primary still accepts clients")
            # The previous check raced with a returning synchronous standby:
            # /sync can be absent at T0, but PostgreSQL may have a new sync
            # sender by T1, legitimately completing the write. That is NOT
            # a strict-mode violation. Stop the OTHER surviving standby and
            # its PG postmaster as well so the new primary has ZERO peers.
            other = next(i for i in survivors if i != successor)
            patroni[other].terminate()
            patroni[other].wait(timeout=7)
            processes.pop(f"patroni{other}", None)
            run("/usr/lib/postgresql/16/bin/pg_ctl", "-D",
                str(data_dirs[other]), "stop", "-m", "immediate",
                check=False, timeout=20)
            def no_sync_peer():
                if dcs_sync_candidate(root, successor) is not None:
                    return None
                observed = pg(root, successor,
                    "SELECT count(*) FROM pg_stat_replication "
                    "WHERE state='streaming' AND sync_state='sync'",
                    expect_ok=False)
                return observed.returncode == 0 and observed.stdout.strip() == "0"
            wait_for("proven absence of ALL synchronous PostgreSQL peers",
                     no_sync_peer, processes, 75)
            print("Both original PG standbys stopped; no live synchronous sender",
                  flush=True)
            sync_settings = pg(root, successor,
                "SELECT current_setting('synchronous_standby_names'), "
                "current_setting('synchronous_commit')", expect_ok=False)
            print(f"Strict PostgreSQL after peer loss: "
                  f"{sync_settings.stdout.strip()[:150]}", flush=True)
            # Both other PostgreSQL servers stay stopped during the ENTIRE
            # two-second commit attempt, so a successful INSERT is a true
            # safety violation rather than a promotion/replication race.
            result = pg(root, successor,
                "SET statement_timeout='2000ms'; "
                "INSERT INTO ha_witness VALUES (9,'unsafe-without-sync');",
                expect_ok=False)
            if result.returncode == 0:
                raise RuntimeError("UNSAFE: strict primary committed with BOTH standby servers stopped")
            print("PASS: strict primary refused a commit without either standby",
                  flush=True)
            # Rejoin the former leader: Patroni must rewind or rebuild it as a standby.
            path, _ = conf(root, initial)
            patroni[initial] = start(root / f"patroni-{initial}-restart.log",
                                     "patroni", str(path))
            processes[f"patroni{initial}"] = patroni[initial]
            def rejoined():
                return role(root, initial, "replica") and (
                    pg(root, initial, "SELECT pg_is_in_recovery()",
                       expect_ok=False).stdout.strip() == "t")
            wait_for("old primary reintegrated as standby", rejoined, processes, 110)
            if len(primaries(root)) != 1:
                raise RuntimeError("Two Patroni primaries visible after rejoin")
            new_sync = wait_for("new leader has DCS-confirmed synchronous standby after rejoin",
                lambda: dcs_sync_candidate(root, successor), processes, 90)
            if new_sync == successor or new_sync not in (0, 1, 2):
                raise RuntimeError("New primary lacks an independent synchronous standby")
            pg(root, successor, "INSERT INTO ha_witness VALUES (2,'committed-after-rejoin');")
            confirmed = pg(root, successor,
                "SELECT payload FROM ha_witness WHERE id IN (1,2) ORDER BY id")
            if confirmed.stdout.strip().splitlines() != [
                "acknowledged-before-failover", "committed-after-rejoin"]:
                raise RuntimeError("HA failover or post-rejoin write verification mismatch")
            # Return the second standby: both must be healthy before a
            # separate DCS-quorum-loss drill can count as complete.
            other_path, _ = conf(root, other)
            patroni[other] = start(root / f"patroni-{other}-second-rejoin.log",
                                   "patroni", str(other_path))
            processes[f"patroni{other}"] = patroni[other]
            wait_for("second standby rejoined", lambda: role(root, other, "replica"),
                     processes, 110)
            print("PASS: both original standby nodes reintegrated", flush=True)
            # Remove etcd majority. All surviving Patroni nodes must lose
            # writable primary status rather than continue with a stale lease.
            for i in (1, 2):
                etcd[i].terminate()
                etcd[i].wait(timeout=7)
                processes.pop(f"etcd{i}", None)
            wait_for("fencing on DCS quorum loss",
                     lambda: len(primaries(root)) == 0, processes, 65)
            # A stale client write is forbidden when etcd no longer has quorum.
            for i in range(3):
                result = pg(root, i,
                    "SET statement_timeout='2000ms'; "
                    "INSERT INTO ha_witness VALUES (3,'must-not-write-without-quorum');",
                    expect_ok=False)
                if result.returncode == 0:
                    raise RuntimeError("UNSAFE: database accepted write without DCS quorum")
            print("PASS real 3-Patroni / 3-etcd TLS quorum, synchronous replication, "
                  "primary crash, promotion, durable data, standby rejoin and DCS fencing")
            print("Scope: separate processes on one ephemeral runner, not "
                  "independent physical hosts or network-namespace split-brain test")
        except Exception:
            # Diagnostic is read-only and does not expose secret keys:
            # it distinguishes DCS sync-role delay from actual PostgreSQL
            # WAL replication failure and from TLS/REST connectivity.
            for i in range(3):
                try:
                    ctx = ssl.create_default_context(cafile=str(root / "ca.crt"))
                    url = f"https://{ADDRESSES[i]}:{RESTPORTS[i]}/cluster"
                    with urllib.request.urlopen(url, context=ctx, timeout=2) as reply:
                        import json
                        view = json.load(reply)
                    identities = [(m.get("name"), m.get("role"), m.get("state"))
                                  for m in view.get("members", [])]
                    print(f"Patroni /cluster[{i}]: {identities}", flush=True)
                except Exception as error:
                    print(f"Patroni /cluster[{i}] unavailable: {type(error).__name__}",
                          flush=True)
                try:
                    result = pg(root, i,
                        "SELECT application_name,sync_state,state FROM pg_stat_replication",
                        expect_ok=False)
                    print(f"PG replication[{i}]: code={result.returncode}; "
                          f"{result.stdout[:300]}", flush=True)
                except Exception as error:
                    print(f"PG replication[{i}] unavailable: {type(error).__name__}",
                          flush=True)
            for i in range(3):
                check = run(*etcd_command(root, i), "endpoint", "health",
                    check=False, timeout=6)
                print(f"etcdctl endpoint-{i+1}: code={check.returncode}; "
                      f"{check.stderr[-300:]} {check.stdout[-300:]}")
            for name in ("etcd-0.log", "etcd-1.log", "etcd-2.log",
                         "patroni-0.log", "patroni-1.log", "patroni-2.log"):
                path = root / name
                if path.is_file():
                    print(f"CI HA diagnostics {name}:")
                    lines = path.read_text(errors="replace").splitlines()
                    print("Startup evidence:\n" + "\n".join(lines[:35]))
                    print("Tail evidence:\n" + "\n".join(lines[-18:]))
                    errors = [line for line in lines if any(term in line.lower()
                        for term in ("tls", "certificate", "handshake", "connection refused",
                                     "peer rejected", "remote error"))]
                    print("TLS peer evidence:\n" + "\n".join(errors[-15:]))
            raise
        finally:
            for proc in list(patroni.values()) + list(etcd.values()):
                if proc.poll() is None:
                    proc.terminate()
            for proc in list(patroni.values()) + list(etcd.values()):
                if proc.poll() is None:
                    try:
                        proc.wait(timeout=4)
                    except subprocess.TimeoutExpired:
                        proc.kill()
                        proc.wait(timeout=4)
            for data in data_dirs.values():
                if data.exists():
                    run("/usr/lib/postgresql/16/bin/pg_ctl", "-D",
                        str(data), "stop", "-m", "immediate",
                        check=False, timeout=12)


if __name__ == "__main__":
    main()
