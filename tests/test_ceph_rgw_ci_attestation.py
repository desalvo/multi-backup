"""Fail-closed guardrails for interpreting actual Ceph status and CRUSH evidence."""
import importlib.util
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "ceph_assert_cluster",
    ROOT / "scripts" / "ci" / "ceph-assert-cluster.py")
module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(module)


def fixture(mons=3, up=3, hosts=3):
    status = {
        "monmap": {"mons": [{"name": f"mon.{n}"} for n in range(mons)]},
        "quorum": list(range(mons)),
        "quorum_names": [f"mon.{n}" for n in range(mons)],
        "osdmap": {"osdmap": {
            "num_osds": up, "num_up_osds": up, "num_in_osds": up,
        }},
    }
    status["pgmap"] = {"num_pgs": 18, "num_remapped_pgs": 0,
                       "pgs_by_state": [{"state_name": "active+clean", "count": 18}]}
    tree = {"nodes": (
        [{"type": "host", "name": f"kind-worker{n}",
          "children": [n]} for n in range(hosts)]
        + [{"type": "osd", "id": n, "status": "up"} for n in range(up)]
    )}
    return status, tree


class CephRealClusterAttestationTests(unittest.TestCase):
    def test_three_mons_osds_hosts_pass(self):
        value = module.attest(*fixture())
        self.assertEqual(value["quorum"], 3)
        self.assertEqual(len(value["hosts"]), 3)

    def test_single_node_fake_cluster_is_not_pass(self):
        with self.assertRaises(ValueError):
            module.attest(*fixture(hosts=1))

    def test_mon_quorum_not_enough(self):
        with self.assertRaises(ValueError):
            module.attest(*fixture(mons=2))

    def test_missing_osd_fails(self):
        with self.assertRaises(ValueError):
            module.attest(*fixture(up=2))

    def test_three_hosts_with_remapped_pgs_fail(self):
        status, tree = fixture()
        status["pgmap"]["num_remapped_pgs"] = 9
        status["pgmap"]["pgs_by_state"] = [
            {"state_name": "active+clean+remapped", "count": 18}]
        with self.assertRaises(ValueError):
            module.attest(status, tree)

    def test_three_hosts_with_degraded_pgs_fail(self):
        status, tree = fixture()
        status["pgmap"]["pgs_by_state"] = [
            {"state_name": "active+clean+degraded", "count": 18}]
        with self.assertRaises(ValueError):
            module.attest(status, tree)

    def test_osd_down_on_one_host_fails(self):
        status, tree = fixture()
        tree["nodes"][-1]["status"] = "down"
        with self.assertRaises(ValueError):
            module.attest(status, tree)


if __name__ == "__main__":
    unittest.main()
