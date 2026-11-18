"""Cloud drive mount capability and path confinement (no cloud credentials)."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest

MODULE=Path(__file__).resolve().parents[1]/"scripts/cloud/cloud_mount.py"
spec=importlib.util.spec_from_file_location("cloud_mount",MODULE)
mount=importlib.util.module_from_spec(spec)
spec.loader.exec_module(mount)

class CloudMount(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        base=Path(self.tmp.name)
        self.rclone=base/"rclone"
        self.rclone.write_text("#!/bin/sh\nexit 0\n")
        self.rclone.chmod(0o700)
        self.target=base/"mnt"
        self.target.mkdir()
        self.rclone_config=base/"rclone.conf"
        self.rclone_config.write_text("[drive]\ntype = alias\nremote = /tmp/mock\n")
        self.rclone_config.chmod(0o600)
        self.registry=base/"profiles.json"
        self.profiles=[
            {"id":"drive","provider":"google-drive","remote":"drive",
             "prefix":"Team","allow_upload":True,"allow_delete":False},
            {"id":"apple","provider":"icloud","remote":"apple",
             "allow_upload":False,"allow_delete":False},
            {"id":"next","provider":"nextcloud","remote":"next","prefix":"Archive"}
        ]
        self.write()
    def write(self):
        self.registry.write_text(json.dumps({
            "rclone_config":str(self.rclone_config),"profiles":self.profiles
        }))
        self.registry.chmod(0o600)
    def command(self,ident):
        return mount.mount_command(self.registry,ident,str(self.target),str(self.rclone))
    def test_rw_mount_is_scoped_and_without_admin_listener(self):
        cmd=self.command("drive")
        self.assertIn("drive:Team",cmd)
        self.assertNotIn("--read-only",cmd)
        self.assertNotIn("--rc",cmd)
        self.assertNotIn("--allow-other",cmd)
        self.assertEqual(cmd[-1],"1m")
    def test_default_and_icloud_are_readonly(self):
        for ident in ("next","apple"):
            self.assertIn("--read-only",self.command(ident))
    def test_insecure_config_symlink_and_path_refused(self):
        self.registry.chmod(0o644)
        with self.assertRaises(ValueError):self.command("drive")
        self.registry.chmod(0o600)
        self.profiles[0]["prefix"]="../untrusted"
        self.write()
        with self.assertRaises(ValueError):self.command("drive")
    def test_icloud_cannot_advertise_remote_write(self):
        self.profiles[1]["allow_upload"]=True
        self.write()
        with self.assertRaises(ValueError):self.command("apple")
    def test_mountpoint_must_exist_and_be_empty(self):
        (self.target/"item").write_bytes(b"data")
        with self.assertRaises(ValueError):self.command("drive")
        (self.target/"item").unlink()
        with self.assertRaises(ValueError):
            mount.mount_command(self.registry,"drive","/does/not/exist",str(self.rclone))
    def test_no_unbounded_dynamic_backends(self):
        self.profiles[0]["remote"]=":local:/etc"
        self.write()
        with self.assertRaises(ValueError):self.command("drive")
    def test_all_targeted_cloud_families_are_explicitly_supported(self):
        self.assertGreaterEqual(len(mount.ALLOWED),9)
        for x in ["dropbox","google-drive","icloud","onedrive","owncloud","nextcloud"]:
            self.assertIn(x,mount.ALLOWED)
    def test_validation_handles_malicious_segments(self):
        for path in ["../../x","/etc","a//b","a/./b","a\\b","bad:remote","a/\nother"]:
            self.assertFalse(mount.relative(path),path)
        self.assertTrue(mount.relative("Docs/2026 backup/file.tar.gz.age"))
if __name__=="__main__":
    unittest.main()
