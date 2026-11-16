"""Cheap Rust-only release artifact-selection tests; do not build packages."""
from pathlib import Path
from tempfile import TemporaryDirectory
from importlib.util import module_from_spec, spec_from_file_location
import unittest

spec=spec_from_file_location("rust_release",Path(__file__).resolve().parents[1]/
    "scripts/ci/collect-rust-release.py")
mod=module_from_spec(spec)
spec.loader.exec_module(mod)

def prepare(base):
    gui=base/"gui"
    agent=base/"agent"
    for folder,expected in ((gui,mod.GUI),(agent,mod.AGENT)):
        for name,ext in expected.items():
            item=folder/name
            item.mkdir(parents=True,exist_ok=True)
            prefix="multi-backup-rust" if name.startswith("rust-only") else "multi-backup-agent"
            (item/(prefix+"-"+name+ext)).write_bytes(b"test-placeholder-only")
    return gui,agent

class TestRustReleaseGate(unittest.TestCase):
    def test_requires_all_gui_agent_artifacts_and_no_legacy(self):
        with TemporaryDirectory() as temp:
            base=Path(temp)
            gui,agent=prepare(base)
            result=mod.assemble(gui,agent,base/"release")
            self.assertEqual(result["matrix_folders"],len(mod.GUI)+len(mod.AGENT))
            self.assertFalse(result["python_legacy_included"])
            self.assertFalse(result["stable_certified"])
            self.assertGreaterEqual(len(result["assets"]),len(mod.GUI)+len(mod.AGENT))
            with self.assertRaises(ValueError):
                mod.assemble(gui,agent,base/"release")
    def test_refuses_missing_partial_and_legacy(self):
        with TemporaryDirectory() as temp:
            base=Path(temp)
            gui,agent=prepare(base)
            missing=gui/next(iter(mod.GUI))
            for file in missing.iterdir():file.unlink()
            with self.assertRaises(ValueError):
                mod.assemble(gui,agent,base/"no")
        with TemporaryDirectory() as temp:
            base=Path(temp)
            gui,agent=prepare(base)
            (gui/next(iter(mod.GUI))/"multi-backup-legacy.exe").write_bytes(b"legacy")
            with self.assertRaises(ValueError):
                mod.assemble(gui,agent,base/"no")
    def test_rejects_duplicate_filename_or_symlinked_file(self):
        with TemporaryDirectory() as temp:
            base=Path(temp)
            gui,agent=prepare(base)
            dirs=list(gui.iterdir())
            f=next(dirs[0].iterdir())
            other=dirs[1]
            (other/f.name).write_bytes(b"collision")
            with self.assertRaises(ValueError):
                mod.assemble(gui,agent,base/"no")
        with TemporaryDirectory() as temp:
            base=Path(temp)
            gui,agent=prepare(base)
            d=next(gui.iterdir())
            f=next(d.iterdir())
            f.unlink()
            f.symlink_to("/etc/passwd")
            with self.assertRaises(ValueError):
                mod.assemble(gui,agent,base/"no")
if __name__=="__main__":unittest.main()
