"""Safety gates only: NOT a Ceph integration or a certification test."""
from importlib.util import module_from_spec,spec_from_file_location
from pathlib import Path
import unittest
spec=spec_from_file_location("crush",Path(__file__).resolve().parents[1]/
                             "scripts/ci/ceph-osd-crush-align.py")
mod=module_from_spec(spec);spec.loader.exec_module(mod)
B=[("mb-ceph-rgw-worker","loop2"),("mb-ceph-rgw-worker2","loop3"),
   ("mb-ceph-rgw-worker3","loop4")]
def inputs():
    status={"quorum_names":["a","b","c"],
     "osdmap":{"osdmap":{"num_osds":3,"num_up_osds":3,"num_in_osds":3}}}
    tree={"nodes":[{"type":"osd","id":i,"status":"up"} for i in range(3)]+
          [{"type":"host","name":B[0][0],"children":[0,1,2]}]}
    pods={"items":[{"spec":{"nodeName":n,"containers":[{"env":[
       {"name":"ROOK_OSD_ID","value":str(i)},
       {"name":"ROOK_BLOCK_PATH","value":"/dev/"+d},
       {"name":"ROOK_CRUSHMAP_HOSTNAME","value":n}]}]},
       "status":{"phase":"Running","containerStatuses":[
           {"name":"osd","ready":True,"state":{"running":{"startedAt":"2026-11-18T00:00:00Z"}}}
       ]}} for i,(n,d) in enumerate(B)]}
    return status,tree,pods
class Safety(unittest.TestCase):
    def test_real_pod_evidence_required_for_move(self):
        self.assertEqual(mod.reconcile(*inputs(),B),[(1,B[1][0]),(2,B[2][0])])
        s,t,p=inputs()
        t["nodes"]=t["nodes"][:3]+[{"type":"host","name":n,"children":[i]}
                                    for i,(n,_) in enumerate(B)]
        self.assertEqual(mod.reconcile(s,t,p,B),[])
    def test_refuses_duplicate_pod_device_or_worker(self):
        s,t,p=inputs();p["items"][2]["spec"]["nodeName"]=B[1][0]
        with self.assertRaises(ValueError):mod.reconcile(s,t,p,B)
        s,t,p=inputs();p["items"][1]["spec"]["containers"][0]["env"][1]["value"]="/dev/sda"
        with self.assertRaises(ValueError):mod.reconcile(s,t,p,B)
        with self.assertRaises(ValueError):mod.reconcile(*inputs(),[B[0],B[0],B[2]])
    def test_refuses_stale_daemon_crush_host_even_on_correct_worker(self):
        s,t,p=inputs()
        p["items"][1]["spec"]["containers"][0]["env"][2]["value"]=B[0][0]
        with self.assertRaises(ValueError):mod.reconcile(s,t,p,B)
    def test_refuses_pods_not_yet_ready_or_restarting(self):
        s,t,p=inputs()
        p["items"][2]["status"]["containerStatuses"][0]["ready"]=False
        with self.assertRaisesRegex(ValueError,"not ready"):
            mod.reconcile(s,t,p,B)
        s,t,p=inputs()
        p["items"][2]["status"]["containerStatuses"][0]["state"]={"waiting":{"reason":"ContainerCreating"}}
        with self.assertRaisesRegex(ValueError,"not ready"):
            mod.reconcile(s,t,p,B)
        s,t,p=inputs()
        del p["items"][2]["status"]["containerStatuses"]
        with self.assertRaisesRegex(ValueError,"not ready"):
            mod.reconcile(s,t,p,B)
    def test_refuses_missing_quorum_or_osd(self):
        s,t,p=inputs();s["osdmap"]["osdmap"]["num_in_osds"]=2
        with self.assertRaises(ValueError):mod.reconcile(s,t,p,B)
        s,t,p=inputs();s["quorum_names"]=["a","b"]
        with self.assertRaises(ValueError):mod.reconcile(s,t,p,B)
if __name__=="__main__":unittest.main()
