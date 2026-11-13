"""Visual quality regressions for rounded Retina-style UI elements."""
from __future__ import annotations

import unittest

from PIL import Image

from render_quality import (
    SCALE, glossy_patch, progress_patch, rounded_overlay,
    rounded_patch, smooth_polyline,
)
from ui_widgets import progress_color


class AntialiasTests(unittest.TestCase):
    def test_supersampled_corner_has_fractional_coverage(self):
        patch = rounded_patch(
            84, 48, (2, 2, 81, 45), 17,
            fill=(112, 80, 216, 255), outline=(231, 216, 255, 255))
        alpha = list(patch.getchannel('A').getdata())
        self.assertGreaterEqual(SCALE, 3)
        self.assertTrue(any(0 < a < 255 for a in alpha),
                        'Rounded corners have no fractional coverage: jaggies returned')
        self.assertEqual(patch.getpixel((0, 0))[3], 0)
        self.assertGreater(patch.getpixel((40, 4))[3], 0)

    def test_glossy_button_gradient_and_curve_antialiased(self):
        button = glossy_patch(220, 48, top='#A875FD',
                              bottom='#305DE3', border='#C6D4FF')
        self.assertEqual(button.size, (220, 48))
        self.assertNotEqual(button.getpixel((110, 10))[:3],
                            button.getpixel((110, 35))[:3])
        alpha = list(button.getchannel('A').getdata())
        self.assertTrue(any(0 < a < 255 for a in alpha),
                        'Button and inner curved reflection are pixelated')

    def test_card_and_badge_blend_without_hard_edge(self):
        frame = Image.new('RGBA', (320, 180), (12, 25, 62, 255))
        rounded_overlay(frame, (4.5, 6.5, 307.5, 167.5),
                        20, (35, 56, 110, 240),
                        outline=(168, 181, 255, 110))
        self.assertEqual(frame.size, (320, 180))
        self.assertNotEqual(frame.getpixel((5, 85))[:3],
                            frame.getpixel((20, 85))[:3])

    def test_progress_and_decorative_lines_smooth(self):
        bar = progress_patch(440, 43, 57, '#F5F8FC', progress_color)
        self.assertEqual(bar.size, (440, 43))
        self.assertNotEqual(bar.getpixel((35, 15)), bar.getpixel((310, 15)))
        background = Image.new('RGBA', (180, 80), (0, 0, 0, 255))
        smooth_polyline(background, [(2, 25), (50, 20.4),
                                     (100, 35.7), (170, 25)],
                        (120, 180, 255, 230), 2)
        colours = set(background.getdata())
        self.assertGreater(len(colours), 5)


if __name__ == '__main__':
    unittest.main()
