"""Static isolation invariants; no claim of network-level field testing."""
from importlib.util import module_from_spec,spec_from_file_location
from pathlib import Path
import unittest
spec=spec_from_file_location("isolation",Path(__file__).resolve().parents[1]/
    "scripts/ci/check-private-db-exposure.py")
mod=module_from_spec(spec);spec.loader.exec_module(mod)

class DBIsolation(unittest.TestCase):
    def test_service_never_exposes_postgres_patroni_or_etcd(self):
        good=[{"kind":"Service","spec":{"ports":[{"port":8080,"targetPort":8080}]}}]
        self.assertTrue(mod.check_kubernetes(good))
        for port in (5432,2379,2380,8008):
            bad=[{"kind":"Service","spec":{"ports":[{"port":port}]}}]
            with self.assertRaises(ValueError):mod.check_kubernetes(bad)
        with self.assertRaises(ValueError):
            mod.check_kubernetes([{"kind":"Service","spec":{
                "type":"LoadBalancer","ports":[{"port":8080}]}}])
    def test_k8s_server_denies_db_host_ports(self):
        with self.assertRaises(ValueError):
            mod.check_kubernetes([{"kind":"Deployment","spec":{"template":{"spec":{
                "containers":[{"name":"db","ports":[{"containerPort":5432}]}]}}}}])
        with self.assertRaises(ValueError):
            mod.check_kubernetes([{"kind":"Deployment","spec":{"template":{"spec":{
                "hostNetwork":True,"containers":[]}}}}])
    def test_haproxy_private_listener_required(self):
        root=Path(__file__).resolve().parents[1]
        cfgs=[(root/p).read_text() for p in (
            "packaging/ha/haproxy-postgres.cfg.example",
            "packaging/ha/patroni-node.yaml.example",
            "packaging/ha/etcd-node.env.example",
            "packaging/linux/server.env.example")]
        self.assertTrue(mod.check_private_config(*cfgs))
        with self.assertRaises(ValueError):
            mod.check_private_config(cfgs[0].replace("bind 127.0.0.1:6432",
                         "bind 0.0.0.0:6432"),*cfgs[1:])
if __name__=="__main__":unittest.main()
