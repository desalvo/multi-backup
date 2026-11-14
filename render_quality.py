"""High-quality antialiased raster primitives for the Tk desktop interface.

Pillow draws 1px arcs without fractional coverage. Draw at 4x, then use
LANCZOS downsampling, rather than scaling an already-aliased PhotoImage.
Keep text as native Tk text so it remains sharp and accessible.
"""
from __future__ import annotations

import math
from PIL import Image, ImageDraw

SCALE = 4


def _rgba(color):
    if color is None or color == '':
        return None
    if isinstance(color, str):
        rgb = color.lstrip('#')
        if len(rgb) != 6:
            raise ValueError('Expected six-digit RGB colour')
        return tuple(int(rgb[p:p + 2], 16) for p in (0, 2, 4)) + (255,)
    if len(color) == 3:
        return tuple(color) + (255,)
    return tuple(color)


def _blend(a, b, weight):
    t = min(1., max(0., weight))
    return tuple(round(x * (1 - t) + y * t) for x, y in zip(a, b))


def rounded_patch(width, height, rect, radius, *,
                  fill, outline=None, line_width=1, scale=SCALE):
    """Transparent, supersampled patch; rect is in local logical pixels."""
    width, height = int(width), int(height)
    if width <= 0 or height <= 0:
        raise ValueError('Patch dimensions must be positive')
    high = Image.new('RGBA', (width * scale, height * scale), (0, 0, 0, 0))
    painter = ImageDraw.Draw(high)
    rect = tuple(round(float(v) * scale) for v in rect)
    painter.rounded_rectangle(
        rect, radius=round(float(radius) * scale),
        fill=_rgba(fill), outline=_rgba(outline),
        width=max(1, round(float(line_width) * scale)))
    return high.resize((width, height), Image.Resampling.LANCZOS)


def rounded_overlay(image, box, radius, fill, outline=None, line_width=1, *,
                    scale=SCALE):
    """Composite a smooth rounded rectangle into RGBA image, clipping safely."""
    if image.mode != 'RGBA':
        raise ValueError('Rounded overlays need an RGBA destination')
    left, top, right, bottom = map(float, box)
    if right <= left or bottom <= top:
        return
    margin = max(2, math.ceil(line_width))
    x0 = max(0, math.floor(left) - margin)
    y0 = max(0, math.floor(top) - margin)
    x1 = min(image.width, math.ceil(right) + margin)
    y1 = min(image.height, math.ceil(bottom) + margin)
    if x1 <= x0 or y1 <= y0:
        return
    patch = rounded_patch(
        x1 - x0, y1 - y0,
        (left - x0, top - y0, right - x0, bottom - y0),
        radius, fill=fill, outline=outline, line_width=line_width,
        scale=scale)
    image.alpha_composite(patch, (x0, y0))


def glossy_patch(width, height, *, top, bottom, border, radius=18,
                 focused=False, scale=SCALE, shadow=True):
    """Draw a round gradient button, border and delicate curved glint at 4x."""
    width, height = int(width), int(height)
    big_w, big_h = width * scale, height * scale
    high = Image.new('RGBA', (big_w, big_h), (0, 0, 0, 0))
    pen = ImageDraw.Draw(high)
    r = round(min(radius, (height - 5) / 2) * scale)
    if shadow:
        pen.rounded_rectangle(
            (2 * scale, 5 * scale, (width - 3) * scale,
             (height - 1) * scale),
            radius=r, fill=(2, 6, 29, 96))
    gradient = Image.new('RGBA', (big_w, big_h), (0, 0, 0, 0))
    gd = ImageDraw.Draw(gradient)
    first, last = _rgba(top)[:3], _rgba(bottom)[:3]
    start, stop = 2 * scale, max(2 * scale + 1, (height - 4) * scale)
    for y in range(start, stop):
        blend = _blend(first, last, (y - start) / max(1, stop - start))
        gd.line((0, y, big_w - 1, y), fill=(*blend, 255), width=1)
    mask = Image.new('L', (big_w, big_h), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        (2 * scale, 2 * scale, (width - 3) * scale,
         (height - 4) * scale), radius=r, fill=255)
    high.alpha_composite(Image.composite(
        gradient, Image.new('RGBA', gradient.size, (0, 0, 0, 0)), mask))
    pen = ImageDraw.Draw(high)
    pen.rounded_rectangle(
        (2 * scale, 2 * scale, (width - 3) * scale,
         (height - 4) * scale), radius=r, outline=_rgba(border),
        width=scale)
    # A single antialiased glass reflection instead of jagged native arcs.
    if width > 24 and height > 15:
        pen.arc(
            (7 * scale, 4 * scale,
             (width - 8) * scale, (height - 9) * scale),
            200, 338, fill=(245, 249, 255, 97), width=scale)
    if focused:
        pen.rounded_rectangle(
            (scale, scale, (width - 2) * scale,
             (height - 2) * scale), radius=r + scale,
            outline=(114, 205, 255, 255), width=2 * scale)
    return high.resize((width, height), Image.Resampling.LANCZOS)


def progress_patch(width, height, percent, background, gradient_at, *,
                   scale=SCALE):
    """Antialias the track, fill corners, highlight and percentage capsule."""
    width, height = int(width), int(height)
    w, h = width * scale, height * scale
    high = Image.new('RGBA', (w, h), _rgba(background))
    pen = ImageDraw.Draw(high)
    x1, y1, x2, y2 = 3, 6, width - 4, 35
    def rect(points):
        return tuple(round(value * scale) for value in points)
    pen.rounded_rectangle(rect((x1, y1 + 3, x2, y2 + 3)),
                          radius=13 * scale, fill=(19, 44, 74, 53))
    pen.rounded_rectangle(rect((x1, y1, x2, y2)),
                          radius=13 * scale,
                          fill=(24, 48, 76, 255),
                          outline=(109, 139, 167, 255), width=2 * scale)
    full = max(1, x2 - x1 - 4)
    fill_width = max(10, round(full * max(0, min(100, percent)) / 100))
    grad = Image.new('RGBA', (w, h), (0, 0, 0, 0))
    gp = ImageDraw.Draw(grad)
    for xpos in range((x1 + 2) * scale, min((x2 - 1) * scale,
                                           (x1 + 2 + fill_width) * scale)):
        pos = (xpos / scale - x1 - 2) / full * 100
        rgb = _rgba(gradient_at(pos))[:3]
        gp.line((xpos, (y1 + 2) * scale, xpos, (y2 - 2) * scale),
                fill=(*rgb, 255), width=1)
        gp.line((xpos, (y1 + 3) * scale, xpos, (y1 + 10) * scale),
                fill=(*_blend(rgb, (255, 255, 255), .35), 255), width=1)
    mask = Image.new('L', (w, h), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        rect((x1 + 2, y1 + 2, min(x2 - 2, x1 + 2 + fill_width),
              y2 - 2)), radius=10 * scale, fill=255)
    high.alpha_composite(Image.composite(
        grad, Image.new('RGBA', grad.size, (0, 0, 0, 0)), mask))
    pen = ImageDraw.Draw(high)
    pen.arc(rect((x1 + 2, y1 + 1, x2 - 2, y2 - 4)),
            195, 345, fill=(255, 255, 255, 130), width=scale)
    pen.rounded_rectangle(
        rect((max(x1 + 8, x2 - 61), 10, x2 - 7, 31)),
        radius=10 * scale, fill=(15, 33, 55, 229))
    return high.resize((width, height), Image.Resampling.LANCZOS)


def activity_patch(size=28, *, scale=SCALE):
    """Premultiplied-looking translucent busy dot rasterized at Retina density.

    Draw the indicator once, then move the existing Tk image by coordinates:
    no costly full-width bar re-render and no pixelated Canvas oval edges.
    """
    side = int(size)
    if side < 12:
        raise ValueError('Activity indicator is too small')
    high = Image.new('RGBA', (side * scale, side * scale), (0, 0, 0, 0))
    pen = ImageDraw.Draw(high)
    center = side * scale / 2
    for radius, fill in ((12.0, (103, 194, 255, 28)),
                         (9.0, (110, 215, 255, 55)),
                         (6.0, (15, 33, 67, 240)),
                         (4.5, (244, 250, 255, 255))):
        rr = radius * scale
        pen.ellipse((round(center - rr), round(center - rr),
                     round(center + rr), round(center + rr)), fill=fill)
    pen.arc((round(center - 5 * scale), round(center - 5 * scale),
             round(center + 5 * scale), round(center + 5 * scale)),
            195, 348, fill=(109, 243, 255, 255), width=scale)
    return high.resize((side, side), Image.Resampling.LANCZOS)


def smooth_polyline(image, points, colour, line_width=2, *, scale=SCALE):
    """Downsample only the local bounds of a decorative curved line."""
    points = list(points)
    if len(points) < 2:
        return
    bound = max(line_width, 2) + 3
    left = max(0, math.floor(min(x for x, _ in points) - bound))
    top = max(0, math.floor(min(y for _, y in points) - bound))
    right = min(image.width, math.ceil(max(x for x, _ in points) + bound))
    bottom = min(image.height, math.ceil(max(y for _, y in points) + bound))
    if right <= left or bottom <= top:
        return
    patch = Image.new('RGBA', ((right - left) * scale,
                                (bottom - top) * scale), (0, 0, 0, 0))
    ImageDraw.Draw(patch).line(
        [(round((x - left) * scale), round((y - top) * scale))
         for x, y in points],
        fill=_rgba(colour), width=max(1, round(line_width * scale)),
        joint='curve')
    image.alpha_composite(
        patch.resize((right - left, bottom - top), Image.Resampling.LANCZOS),
        (left, top))
