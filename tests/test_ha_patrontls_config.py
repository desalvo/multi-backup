#!/usr/bin/env python3
"""No database exposure or plaintext Patroni health checks in templates."""
import pathlib
import unittest

ROOT=pathlib.Path(__file__).resolve().parents[1]
HA=ROOT/"packaging/ha/haproxy-postgres.cfg.example"
PATRONI=ROOT/"packaging/ha/patroni-node.yaml.example"

class HaConfigPolicy(unittest.TestCase):
    def test_db_proxy_only_loopback_and_tls_primary_check(self):
        text=HA.read_text()
        active=[line.strip() for line in text.splitlines()
                if line.strip() and not line.lstrip().startswith("#")]
        self.assertIn("bind 127.0.0.1:6432",active)
        self.assertIn("option httpchk GET /primary",active)
        self.assertIn("http-check expect status 200",active)
        servers=[line for line in active if line.startswith("server mb-node")]
        self.assertEqual(len(servers),3)
        for idx,line in enumerate(servers,start=1):
            with self.subTest(node=idx):
                self.assertIn(f"10.70.0.1{idx}:5432",line)
                self.assertIn("check port 8008 check-ssl",line)
                self.assertIn(f"check-sni mb-node{idx}.internal",line)
                self.assertIn(f"verifyhost mb-node{idx}.internal",line)
                self.assertIn("verify required ca-file",line)
                self.assertNotIn(" ssl ",line)
        self.assertNotIn("bind 0.0.0.0",text)
        self.assertNotIn("option httpchk GET /health",text)

    def test_patroni_rest_is_tls_private(self):
        text=PATRONI.read_text()
        self.assertIn("certfile:",text)
        self.assertIn("keyfile:",text)
        self.assertIn("10.70.0.11:8008",text)
        self.assertIn("listen: 10.70.0.11:5432",text)
        self.assertNotIn("listen: 0.0.0.0:5432",text)

if __name__=="__main__":
    unittest.main()
