"""Ensure Windows native artifact nesting never weakens release checks."""
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
SCRIPT=ROOT/"scripts/ci/normalize-rust-native-artifacts-r175.sh"

class NormalizeWindowsArchiveTests(unittest.TestCase):
    def setup(self,base):
        gui=base/"gui";agent=base/"agents"
        expected=[
            (gui/"rust-only-windows-x64-preview","multi-backup-rust-preview-windows-x64.exe"),
            (agent/"rust-agent-windows-x64-preview","multi-backup-agent-preview-windows-x64.exe"),
        ]
        for folder,name in expected:
            (folder/"installers").mkdir(parents=True)
            (folder/"installers"/name).write_bytes(b"MZ\x00\x90fake CI-only fixture")
        return gui,agent,expected

    def test_byte_identical_flatten_and_no_overwrite(self):
        with tempfile.TemporaryDirectory() as d:
            gui,agent,expected=self.setup(Path(d))
            a=subprocess.run(["bash",str(SCRIPT),str(gui),str(agent)],capture_output=True)
            self.assertEqual(a.returncode,0,a.stderr)
            for folder,name in expected:
                self.assertEqual((folder/name).read_bytes(),(folder/"installers"/name).read_bytes())
            b=subprocess.run(["bash",str(SCRIPT),str(gui),str(agent)],capture_output=True)
            self.assertNotEqual(b.returncode,0,"No overwrite on retry")

    def test_already_flat_windows_agent_is_passed_without_copy(self):
        with tempfile.TemporaryDirectory() as d:
            gui,agent,expected=self.setup(Path(d))
            folder,name=expected[1]
            (folder/"installers"/name).rename(folder/name)
            original=(folder/name).read_bytes()
            p=subprocess.run(["bash",str(SCRIPT),str(gui),str(agent)],capture_output=True)
            self.assertEqual(p.returncode,0,p.stderr)
            self.assertEqual((folder/name).read_bytes(),original)
            self.assertIn(b"already-flat",p.stdout)

    def test_symlink_input_is_denied(self):
        with tempfile.TemporaryDirectory() as d:
            gui,agent,expected=self.setup(Path(d))
            file=expected[0][0]/"installers"/expected[0][1]
            orig=file.with_suffix(".fixture")
            file.rename(orig)
            file.symlink_to(orig.name)
            r=subprocess.run(["bash",str(SCRIPT),str(gui),str(agent)],capture_output=True)
            self.assertNotEqual(r.returncode,0)

if __name__=="__main__":
    unittest.main()
