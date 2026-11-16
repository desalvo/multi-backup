"""Guarded real-pod OSD node scheduling (offline fixtures, not Ceph certification)."""
from importlib.util import module_from_spec,spec_from_file_location
from pathlib import Path
import unittest
spec=spec_from_file_location("node_guard",Path(__file__).resolve().parents[1]/
                              "scripts/ci/ceph-osd-node-align.py")
guard=module_from_spec(spec);spec.loader.exec_module(guard)
B=[("mb-ceph-rgw-worker","loop2"),("mb-ceph-rgw-worker2","loop3"),
   ("mb-ceph-rgw-worker3","loop4")]
def setup():
    status={"quorum_names":["a","b","c"],"osdmap":{"osdmap":{
       "num_osds":3,"num_up_osds":3,"num_in_osds":3}}}
    tree={"nodes":[{"type":"osd","id":i,"status":"up"} for i in range(3)]}
    pods={"items":[{"metadata":{"name":f"rook-ceph-osd-{i}-test"},
        "status":{"phase":"Running"},"spec":{
            "nodeName":B[0][0] if i<2 else B[1][0],
            "containers":[{"name":"osd","env":[
                {"name":"ROOK_OSD_ID","value":str(i)},
                {"name":"ROOK_BLOCK_PATH","value":"/dev/"+B[i][1]},
                {"name":"ROOK_CRUSHMAP_HOSTNAME","value":B[i][0]}]}]}}
        for i in range(3)]}
    return status,tree,pods
class TestPlacement(unittest.TestCase):
    def test_all_mismatched_osds_have_exact_authorized_target(self):
        self.assertEqual(guard.desired_nodes(*setup(),B),
            [(1,B[1][0]),(2,B[2][0])])
        s,t,p=setup()
        for i,item in enumerate(p["items"]):item["spec"]["nodeName"]=B[i][0]
        self.assertEqual(guard.desired_nodes(s,t,p,B),[])
    def test_stale_daemon_crush_hostname_requires_repair(self):
        s,t,p=setup()
        for i,item in enumerate(p["items"]):item["spec"]["nodeName"]=B[i][0]
        p["items"][1]["spec"]["containers"][0]["env"][2]["value"]=B[0][0]
        self.assertEqual(guard.desired_nodes(s,t,p,B),[(1,B[1][0])])
        p["items"][1]["spec"]["containers"][0]["env"][2]["value"]="unsafe-public-host"
        with self.assertRaises(ValueError):guard.desired_nodes(s,t,p,B)
    def test_no_unsafe_or_duplicate_device(self):
        s,t,p=setup()
        p["items"][0]["spec"]["containers"][0]["env"][1]["value"]="/dev/sda"
        with self.assertRaises(ValueError):guard.desired_nodes(s,t,p,B)
        s,t,p=setup()
        p["items"][0]["spec"]["containers"][0]["env"][1]["value"]="/dev/loop3"
        with self.assertRaises(ValueError):guard.desired_nodes(s,t,p,B)
    def test_no_mismatched_osd_name_or_cluster(self):
        s,t,p=setup();p["items"][0]["metadata"]["name"]="rogue"
        with self.assertRaises(ValueError):guard.desired_nodes(s,t,p,B)
        s,t,p=setup();s["quorum_names"]=["a"]
        with self.assertRaises(ValueError):guard.desired_nodes(s,t,p,B)
        s,t,p=setup();p["items"][1]["spec"]["nodeName"]="production-disk-server"
        with self.assertRaises(ValueError):guard.desired_nodes(s,t,p,B)
if __name__=="__main__":unittest.main()
