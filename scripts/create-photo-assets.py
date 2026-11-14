#!/usr/bin/env python3
"""Produce bespoke, aspect-preserving photographic backgrounds for Multi Backup.

Build-time only: the installed app never requests images from the network.
The licensed source is a free Unsplash photo of actual data-center servers.
Hero and sidebar compositions use full-image CONTAIN, never stretch/crop.
"""
from __future__ import annotations

import io
import os
from pathlib import Path
import ssl
import tempfile
import urllib.request

from PIL import Image, ImageDraw, ImageEnhance, ImageOps

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / 'assets'
SOURCE_URL = (
    'https://images.unsplash.com/photo-1506399558188-acca6f8cbf41'
    '?fm=jpg&w=1800&q=82'
)
MAX_BYTES = 9 * 1024 * 1024


def fetch_source():
    """Download a fixed HTTPS Unsplash free-license image at build time only."""
    request = urllib.request.Request(
        SOURCE_URL, headers={
            'User-Agent': 'MultiBackup-assets/1.0 (+https://github.com/desalvo/multi-backup)',
            'Accept': 'image/jpeg'})
    with urllib.request.urlopen(
            request, timeout=30, context=ssl.create_default_context()) as response:  # nosec B310 - fixed HTTPS photo URL
        if not response.url.startswith('https://images.unsplash.com/'):
            raise RuntimeError('Unexpected photo redirect or provider')
        data = response.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise RuntimeError('Photographic source exceeds safe size limit')
    if not data.startswith(b'\xff\xd8\xff'):
        raise RuntimeError('Unsplash returned something other than a JPEG')
    with Image.open(io.BytesIO(data)) as photo:
        photo.load()
        if not 640 <= photo.width <= 12000 or not 320 <= photo.height <= 8000:
            raise RuntimeError('Unexpected photographic source dimensions')
        return photo.convert('RGB').copy()


def compose(source, size, *, sidebar=False):
    """Create two custom canvases without losing any portion of source photo."""
    w, h = size
    result = Image.new('RGB',size,(12,26,62))
    photo = ImageOps.contain(source, (w if sidebar else round(w * .68), h),
                             method=Image.Resampling.LANCZOS)
    photo = ImageEnhance.Color(photo).enhance(1.16)
    x = (w - photo.width)//2 if sidebar else w - photo.width
    y = (h - photo.height)//2
    result.paste(photo,(x,y))
    tint = Image.new('RGBA',size,(0,0,0,0))
    pen=ImageDraw.Draw(tint)
    for xpos in range(w):
        portion=xpos/max(1,w-1)
        if sidebar:
            opacity=int(50 * (1-portion) + 18)
        else:
            opacity=int(236 * (1-portion)**2.1)
        pen.line((xpos,0,xpos,h),fill=(12,23,62,opacity))
    pen.rectangle((0,h-20,w,h),fill=(11,23,59,45))
    return Image.alpha_composite(result.convert('RGBA'),tint).convert('RGB')


def save_atomic(image, target):
    target.parent.mkdir(parents=True,exist_ok=True)
    fd, filename = tempfile.mkstemp(prefix='.photo-',suffix='.jpg',
                                    dir=target.parent)
    os.close(fd)
    tmp=Path(filename)
    try:
        image.save(tmp,'JPEG',quality=85,progressive=True,optimize=True,subsampling=0)
        os.replace(tmp,target)
    finally:
        tmp.unlink(missing_ok=True)


def main():
    hero=ASSETS/'backup-hero.jpg'
    sidebar=ASSETS/'backup-sidebar.jpg'
    if hero.is_file() and sidebar.is_file():
        with Image.open(hero) as photo:photo.verify()
        with Image.open(sidebar) as photo:photo.verify()
        print('Existing bespoke photographic assets verified')
        return
    source=fetch_source()
    save_atomic(compose(source,(1600,420)),hero)
    save_atomic(compose(source,(436,260),sidebar=True),sidebar)
    print('Generated photographic hero/sidebar from licensed server-storage source')


if __name__=='__main__':
    main()
