"""RC216 installer validation intentionally rejects different SHA, missing OS, altered installer."""
import hashlib
import importlib.util
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

SCRIPT=Path(__file__).resolve().parents[1]/"scripts/ci/verify-platform-artifacts-r217.py"
spec=importlib.util.spec_from_file_location("art216",SCRIPT)
art=importlib.util.module_from_spec(spec)
spec.loader.exec_module(art)
SHA="a"*40

def create_fixture(root:Path):
    for group,(source,suffix,checksum) in art.SOURCES.items():
        folder=root/group
        folder.mkdir(parents=True,exist_ok=False)
        (folder/source).write_text(SHA+"\n")
        name=group+suffix
        output=folder/name
        output.write_bytes(b"binary-preview"*1500)
        hashval=hashlib.sha256(output.read_bytes()).hexdigest()
        check=folder/(name+checksum if checksum!="SHA256SUMS-win-r217.txt" else checksum)
        if not check.name.endswith(checksum):
            check=folder/(name+checksum)
        recorded=f"packages/{name}" if group=="rc216-ubuntu24-amd64-native-deb" else name
        check.write_text(f"{hashval}  {recorded}\n")

class InstallerProof(unittest.TestCase):
    def test_valid_four_full_artifacts(self):
        with TemporaryDirectory() as temp:
            root=Path(temp)
            create_fixture(root)
            items=art.verify(root,SHA)
            self.assertEqual(len(items),4)
            self.assertFalse(any(p["signed_production_release"] for p in items))

    def test_missing_platform_and_changed_bytes_denied(self):
        with TemporaryDirectory() as temp:
            root=Path(temp)
            create_fixture(root)
            (root/"rc216-macos-x86_64-adhoc-dmg"/
             "rc216-macos-x86_64-adhoc-dmg-x86_64.dmg").unlink()
            with self.assertRaises(art.UntrustedPackage):
                art.verify(root,SHA)
        with TemporaryDirectory() as temp:
            root=Path(temp)
            create_fixture(root)
            target=root/"rc216-ubuntu24-amd64-native-deb"/"rc216-ubuntu24-amd64-native-deb.deb"
            target.write_bytes(b"tampered"+target.read_bytes())
            with self.assertRaises(art.UntrustedPackage):
                art.verify(root,SHA)

    def test_wrong_source_sha_denied(self):
        with TemporaryDirectory() as temp:
            root=Path(temp)
            create_fixture(root)
            path=root/"rc216-windows-x64-inno-unsigned"/"SOURCE_SHA-win-r217.txt"
            path.write_text("b"*40)
            with self.assertRaises(art.UntrustedPackage):
                art.verify(root,SHA)

if __name__=="__main__":
    unittest.main()
