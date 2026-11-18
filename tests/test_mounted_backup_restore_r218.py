"""No symlink traversal, no partial backup publish, and create-only restore."""
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location("mounted_backup",ROOT/"scripts/cloud/mounted_backup_restore.py")
mod=importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)

class SecureMountedBrowse(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root=Path(self.tmp.name)
        self.src=self.root/"cloud"
        (self.src/".multi-backup-v1"/"chunks"/"12").mkdir(parents=True)
        (self.src/".multi-backup-v1"/"config.json.age").write_bytes(b"age-encryption.org/v1\n")
        (self.src/".multi-backup-v1"/"chunks"/"12"/"a.age").write_bytes(b"encrypted chunk")
    def test_regular_age_store_staged_to_private_create_only_cache(self):
        target=self.root/"cache"
        mod.stage(self.src,target)
        self.assertEqual((target/".multi-backup-v1"/"chunks"/"12"/"a.age").read_bytes(),b"encrypted chunk")
        self.assertEqual(target.stat().st_mode & 0o077,0)
        with self.assertRaises(ValueError): mod.stage(self.src,target)
    def test_cloud_store_with_symlink_is_refused(self):
        path=self.src/".multi-backup-v1"/"chunks"/"12"/"evil"
        path.symlink_to("/etc/passwd")
        with self.assertRaisesRegex(ValueError,"Symlink"):
            mod.enumerate_store(self.src)
    def test_size_and_file_count_limits_fail_closed(self):
        with self.assertRaisesRegex(ValueError,"limits"):
            mod.enumerate_store(self.src,max_files=1)
        with self.assertRaisesRegex(ValueError,"limits"):
            mod.enumerate_store(self.src,max_bytes=2)
    def test_path_check_rejects_escape_and_invalid_hash(self):
        for bad in ["../../home","a//b","/etc","a/../b","test:foo","some\\path"]:
            self.assertFalse(mod.relative_path(bad),bad)
        self.assertTrue(mod.relative_path("Projects/2026/file.txt"))
        self.assertFalse(bool(mod.BACKUP_ID.fullmatch("deadbeef")))
    def test_local_directory_without_real_mount_is_refused(self):
        with self.assertRaisesRegex(ValueError,"not an actual mounted"):
            mod.safe_root(self.src,"",False)
    def test_unrecognized_source_special_device_is_rejected(self):
        (self.src/".multi-backup-v1"/"chunks"/"12"/"pipe").touch()
        os.remove(self.src/".multi-backup-v1"/"chunks"/"12"/"pipe")
        os.mkfifo(self.src/".multi-backup-v1"/"chunks"/"12"/"pipe")
        with self.assertRaisesRegex(ValueError,"nonregular"):
            mod.enumerate_store(self.src)
    def test_restore_refuses_nonexistent_native(self):
        with self.assertRaises(ValueError):
            mod.run_native(self.root/"missing","list",self.src,self.src/"missing")
if __name__=="__main__":
    unittest.main()
