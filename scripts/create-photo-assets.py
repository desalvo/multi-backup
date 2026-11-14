#!/usr/bin/env python3
"""Generate original Multi Backup artwork. No network, stock photos or third-party logos.
Artwork is programmatically composed for this project and may be dedicated to CC0
where copyright protection applies. Pillow is a build-time dependency only.
"""
from __future__ import annotations
import os
import tempfile
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "assets"
SCALE = 2


def font(size: int):
    try:
        return ImageFont.truetype("DejaVuSans-Bold.ttf", size)
    except OSError:
        return ImageFont.load_default()


def generate(size: tuple[int, int], sidebar: bool = False) -> Image.Image:
    w, h = size
    W, H = w * SCALE, h * SCALE
    canvas = Image.new("RGB", (W, H))
    ink = ImageDraw.Draw(canvas)
    for y in range(H):
        f = y / max(1, H - 1)
        ink.line((0, y, W, y),
                 fill=(int(10 + 12*f), int(21 + 10*f), int(53 + 28*f)))
    # Original illuminated server aisles, rendered entirely from primitives.
    edge = max(16, W // 90)
    bays = 5 if sidebar else 8
    for aisle in range(2):
        for i in range(bays):
            base_x = i * int(W * (0.44 / bays))
            x = base_x if aisle == 0 else W - base_x - int(W * .44 / bays) + 2
            top = edge + int(i * H * .006)
            bottom = H - edge - int(i * H * .008)
            rack_width = max(8, int(W * .43 / bays) - edge // 3)
            ink.rounded_rectangle((x, top, x+rack_width, bottom),
                                  radius=max(3, edge//4),
                                  fill=(13, 24, 57), outline=(48, 74, 135), width=2*SCALE)
            for slot in range(8):
                sy = top + int((bottom - top) * (slot + 1) / 10)
                ink.rounded_rectangle((x + edge//2, sy,
                                       x + rack_width - edge//2, sy + 5*SCALE),
                                      radius=2*SCALE, fill=(29, 43, 85))
                ink.ellipse((x+edge//2,sy,x+edge//2+3*SCALE,sy+3*SCALE),
                            fill=(30, 194, 239) if slot % 3 else (131, 96, 255))
    # Intentionally overlay a central translucent navy glass panel so every
    # text glyph is legible. No source photograph is cropped or watermarked.
    pw = int(W * (.84 if sidebar else .65))
    ph = int(H * .54)
    left, top = (W-pw)//2, (H-ph)//2
    ink.rounded_rectangle((left, top, left+pw, top+ph),
                          radius=20*SCALE, fill=(19, 30, 83),
                          outline=(75, 204, 245), width=3*SCALE)
    ink.arc((left+18*SCALE, top+17*SCALE, left+65*SCALE, top+64*SCALE),
            start=200, end=510, fill=(78, 224, 255), width=5*SCALE)
    title_size = int((30 if sidebar else 70) * SCALE)
    title = "Multi Backup"
    f = font(title_size)
    box = ink.textbbox((0,0), title, font=f)
    width = box[2]-box[0]
    ink.text(((W-width)//2, top+int(ph*.28)), title, font=f,
             fill=(240, 247, 255), stroke_width=0)
    # Encrypted deduplicated blocks, and a cyan active transfer indicator.
    progress_y = top+int(ph*.78)
    px0 = left+int(pw*.15)
    px1 = left+int(pw*.85)
    ink.rounded_rectangle((px0, progress_y, px1, progress_y+8*SCALE),
                          radius=4*SCALE, fill=(38, 51, 112))
    ink.rounded_rectangle((px0, progress_y, px0+int((px1-px0)*.67),
                           progress_y+8*SCALE),
                          radius=4*SCALE, fill=(43, 216, 239))
    return canvas.resize((w, h), Image.Resampling.LANCZOS)


def save_atomic(picture: Image.Image, destination: Path):
    destination.parent.mkdir(parents=True, exist_ok=True)
    fd, path = tempfile.mkstemp(prefix=".original-art-", suffix=".jpg",
                                dir=destination.parent)
    os.close(fd)
    try:
        picture.save(path, "JPEG", quality=87, optimize=True, progressive=True)
        os.replace(path, destination)
    finally:
        Path(path).unlink(missing_ok=True)


def main():
    # Always regenerate: never silently reuse copyrighted/old Unsplash assets.
    save_atomic(generate((1600, 420)), ASSETS / "backup-hero.jpg")
    save_atomic(generate((436, 260), sidebar=True), ASSETS / "backup-sidebar.jpg")
    save_atomic(generate((1600, 420)), ASSETS / "hero.jpg")
    print("Generated original offline Multi Backup imagery; no remote sources")


if __name__ == "__main__":
    main()
