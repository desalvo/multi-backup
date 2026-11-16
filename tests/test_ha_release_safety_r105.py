"""Cheap regression guards for the real Patroni/etcd HA acceptance lab.

These enforce that CI cannot silently weaken TLS, quorum or the production
private-only wiring. They are NOT substitutes for the live HA failure drill.
"""
from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[1]


class HASafetyPolicy(unittest.TestCase):
    def read(self, path):
        return (ROOT / path).read_text(encoding="utf-8")

    def test_ha_lab_only_on_disposable_github_runner(self):
        source = self.read("scripts/ci/patroni-etcd-real-ha.py")
        self.assertIn('os.environ.get("GITHUB_ACTIONS") != "true"', source)
        self.assertIn('os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted"', source)
        self.assertIn("os.geteuid() == 0", source)
        self.assertIn("TemporaryDirectory(prefix=", source)
        for unsafe in ("--insecure-skip-tls-verify", "--peer-auto-tls",
                       "CERT_NONE", "verify=False", "--client-cert-auth=false"):
            self.assertNotIn(unsafe, source)
        self.assertIn('--client-cert-auth=true', source)
        self.assertIn('--peer-client-cert-auth=true', source)
        self.assertIn('IP:127.0.0.1', source)
        self.assertIn("dcs_sync_candidate(root, initial)", source)
        self.assertIn("Witness committed on DCS-confirmed synchronous primary", source)
        self.assertIn("UNSAFE: database accepted write without DCS quorum", source)

    def test_real_ha_ci_requires_explicit_one_shot_request(self):
        workflow = self.read(".github/workflows/patroni-etcd-real-ha-r093.yml")
        self.assertIn("workflow_dispatch:", workflow)
        self.assertIn("contents: read", workflow)
        self.assertIn("runs-on: ubuntu-24.04", workflow)
        self.assertNotIn("branches: [main]", workflow)
        self.assertNotIn("push:\n    branches: [main]", workflow)
        self.assertIn(".github/ci-requests/patroni-ha-", workflow)
        self.assertIn("scripts/ci/patroni-etcd-real-ha.py", workflow)

    def test_production_consensus_and_database_are_private_and_synchronous(self):
        patroni = self.read("packaging/ha/patroni-node.yaml.example")
        etcd = self.read("packaging/ha/etcd-node.env.example")
        proxy = self.read("packaging/ha/haproxy-postgres.cfg.example")
        self.assertIn("synchronous_mode_strict: true", patroni)
        self.assertIn("synchronous_node_count: 1", patroni)
        self.assertRegex(patroni, r"(?m)^\s*ssl: 'on'$")
        self.assertIn("ETCD_PEER_CLIENT_CERT_AUTH=true", etcd)
        self.assertIn("ETCD_CLIENT_CERT_AUTH=true", etcd)
        self.assertIn("ETCD_LISTEN_PEER_URLS=https://10.70.0.", etcd)
        self.assertIn("bind 127.0.0.1:6432", proxy)
        self.assertIn("option httpchk GET /primary", proxy)
        active = [line.strip() for line in proxy.splitlines()
                  if line.strip() and not line.lstrip().startswith("#")]
        self.assertEqual(sum(line.startswith("server mb-node") for line in active), 3)
        for line in active:
            if line.startswith("server mb-node"):
                self.assertIn("verify required ca-file", line)
                self.assertIn("check-ssl", line)
                self.assertNotRegex(line, r"(?<![a-z])verify none\b")
        self.assertNotIn("0.0.0.0:5432", patroni)
        self.assertNotIn("0.0.0.0:6432", proxy)


if __name__ == "__main__":
    unittest.main()
