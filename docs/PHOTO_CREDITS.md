# Multi Backup visual assets — ownership and provenance (r021)

The application is transitioning away from externally licensed third-party
photos (Unsplash/imgix) to original, locally rendered project artwork.

- scripts/create-photo-assets.py draws original hero and sidebar scenes with
  Pillow: only gradients, server rack geometry, typography and project branding.
- The Rust GUI renders its new dashboard hero directly with eframe/egui vector
  primitives; no third-party image URLs or watermark sources are needed.
- Generated imagery reads no URLs and is created offline; its banner reads
  exactly "Multi Backup", never "img IX" or "imgix".
- New artwork source/compositions are offered under CC0-1.0 where possible.
  Application source code remains EUPL-1.2.
- Preexisting brochures, screenshots, old binary assets and historical release
  artifacts **must still be audited and regenerated**. A new source generator
  alone does not retroactively change published images. Do not claim any
  third-party photo is public domain solely because it can be downloaded.

To rebuild original images:
    python scripts/create-photo-assets.py

Manual/screenshot build must run the command before packaging. For legacy
installers, do not distribute the former Unsplash-derived files.
