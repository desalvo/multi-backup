"""Original Multi Backup artwork has no externally licensed/remote sources."""
from __future__ import annotations
import importlib.util
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "offline_multi_backup_art", ROOT/"scripts"/"create-photo-assets.py")
ART = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ART)

class OriginalArtworkTests(unittest.TestCase):
    def test_programmatic_art_is_original_and_at_expected_sizes(self):
        for size, sidebar in [((1600,420),False),((436,260),True)]:
            result=ART.generate(size,sidebar=sidebar)
            self.assertEqual(result.size,size)
            self.assertEqual(result.mode,"RGB")
            self.assertGreater(len(set(result.resize((80,20)).getdata())),20)

    def test_artwork_does_not_download_others_images_or_logos(self):
        script=(ROOT/"scripts"/"create-photo-assets.py").read_text("utf-8")
        for forbidden in ("images.unsplash.com","imgix", "urllib.request",
                          "SOURCE_URL","https://","http://"):
            self.assertNotIn(forbidden,script)
        self.assertIn('"Multi Backup"',script)
        for name in ("gui.py","premium_dashboard.py","gui_launcher.py"):
            source=(ROOT/name).read_text("utf-8")
            self.assertNotIn("urllib.request.urlopen(",source)

    def test_native_build_wires_original_art_generation(self):
        build=(ROOT/"scripts"/"build-macos-app.sh").read_text("utf-8")
        workflow=(ROOT/".github"/"workflows"/"build-native.yml").read_text("utf-8")
        self.assertIn("python scripts/create-photo-assets.py",build)
        self.assertGreaterEqual(workflow.count("scripts/create-photo-assets.py"),3)

if __name__=="__main__":
    unittest.main()
