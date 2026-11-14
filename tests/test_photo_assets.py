"""Photo asset compositions and native build wiring, without network in tests."""
from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path

from PIL import Image, ImageDraw


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    'create_photo_assets', ROOT / 'scripts' / 'create-photo-assets.py')
PHOTO = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PHOTO)


class PhotoCompositionTests(unittest.TestCase):
    def test_panorama_preserves_all_source_subjects_and_dimensions(self):
        source=Image.new('RGB',(400,200),(20,45,90))
        painter=ImageDraw.Draw(source)
        # Four identifying subjects in corners. A destructive cover crop would
        # remove at least one; ImageOps.contain keeps them all.
        corner_colors=((255,20,20),(20,255,20),(20,20,255),(255,255,20))
        positions=((0,0),(382,0),(0,182),(382,182))
        for pos,color in zip(positions,corner_colors):
            painter.rectangle((pos[0],pos[1],pos[0]+17,pos[1]+17),fill=color)
        hero=PHOTO.compose(source,(1600,420))
        sidebar=PHOTO.compose(source,(436,260),sidebar=True)
        self.assertEqual(hero.size,(1600,420))
        self.assertEqual(sidebar.size,(436,260))
        # Both must contain all four distinct corner motifs, allowing for
        # non-destructive translucent colour grading.
        for output in (hero,sidebar):
            pixels=list(output.getdata())
            self.assertTrue(any(r>g*1.3 and r>b*1.3 for r,g,b in pixels))
            self.assertTrue(any(g>r*1.3 and g>b*1.3 for r,g,b in pixels))
            self.assertTrue(any(b>r*1.3 and b>g*1.3 for r,g,b in pixels))
            self.assertTrue(any(r>120 and g>120 and b<100 for r,g,b in pixels))

    def test_photo_not_downloaded_by_installed_application(self):
        for name in ('gui.py','premium_dashboard.py','gui_launcher.py'):
            source=(ROOT/name).read_text('utf-8')
            self.assertNotIn('SOURCE_URL',source)
            self.assertNotIn('urllib.request.urlopen(',source)
        script=(ROOT/'scripts'/'create-photo-assets.py').read_text('utf-8')
        self.assertIn('ImageOps.contain',script)
        self.assertIn('images.unsplash.com',script)
        self.assertIn('ssl.create_default_context()',script)
        self.assertIn('backup-hero.jpg',script)
        self.assertIn('backup-sidebar.jpg',script)

    def test_each_native_platform_generates_both_local_photos(self):
        build=(ROOT/'scripts'/'build-macos-app.sh').read_text('utf-8')
        workflow=(ROOT/'.github'/'workflows'/'build-native.yml').read_text('utf-8')
        self.assertIn('python scripts/create-photo-assets.py',build)
        self.assertGreaterEqual(workflow.count('scripts/create-photo-assets.py'),3)


if __name__ == '__main__':
    unittest.main()
