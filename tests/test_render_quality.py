"""Visual quality regressions for rounded Retina-style UI elements."""
from __future__ import annotations

import unittest

from PIL import Image

from render_quality import (
    SCALE, glossy_patch, progress_patch, rounded_overlay,
    rounded_patch, smooth_polyline, activity_patch,
)
from ui_widgets import progress_color, ActivityPulse, HamburgerButton


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

    def test_progress_rainbow_endpoints_and_continuous_gradient(self):
        from colorsys import hsv_to_rgb
        for step in range(101):
            red, green, blue = hsv_to_rgb(step / 300., .86, .91)
            expected = '#%02X%02X%02X' % (round(red * 255),
                round(green * 255), round(blue * 255))
            self.assertEqual(progress_color(step), expected)
        self.assertEqual(progress_color(0), '#E82020')
        self.assertEqual(progress_color(100), '#20E820')
        self.assertEqual(progress_color(float('nan')), progress_color(0))
        self.assertEqual(progress_color(float('inf')), progress_color(0))
        bar = progress_patch(440, 43, 100, '#152753', progress_color)
        r, g, b = bar.getpixel((25, 20))[:3]
        self.assertGreater(r, g * 1.35)
        r, g, b = bar.getpixel((420, 20))[:3]
        self.assertGreater(g, r * 1.35)
        for x in (40, 90, 180, 260, 350):
            a = bar.getpixel((x, 20))
            z = bar.getpixel((x + 1, 20))
            self.assertLess(max(abs(a[c] - z[c]) for c in range(3)), 11)

    def test_indicator_has_subpixel_coverage(self):
        dot = activity_patch()
        self.assertEqual(dot.size, (28, 28))
        self.assertTrue(any(0 < a < 255 for a in dot.getchannel('A').getdata()))

    def test_activity_model_animates_only_on_fresh_events(self):
        activity = ActivityPulse(idle_after=5)
        self.assertFalse(activity.advance(now=0))
        activity.set_running(True, now=10)
        self.assertTrue(activity.advance(now=12))
        phase = activity.phase
        self.assertFalse(activity.advance(now=16))
        self.assertEqual(activity.phase, phase)
        activity.note_activity(now=20)
        self.assertTrue(activity.advance(now=21))
        activity.set_running(False, now=21)
        phase = activity.phase
        self.assertFalse(activity.advance(now=22))
        self.assertEqual(activity.phase, phase)

    def test_hamburger_is_a_real_keyboard_control(self):
        self.assertTrue(issubclass(HamburgerButton, __import__('tkinter').Canvas))
        import inspect
        source = inspect.getsource(HamburgerButton)
        self.assertIn('Image.Resampling.LANCZOS', source)
        self.assertIn('self._width = 56', source)
        self.assertIn('set_expanded', source)

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
