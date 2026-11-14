# Photographic artwork provenance and usage

Multi Backup r016 uses purpose-built, locally bundled photographic composites:

- assets/backup-hero.jpg — panoramic backup/data-center hero, 1600 × 420
- assets/backup-sidebar.jpg — compact backup/server-photo insert, 436 × 260

The images are generated at **build time only** by scripts/create-photo-assets.py,
using a freely licensed photo of data-center servers from Unsplash / imgix:

- Original: https://unsplash.com/photos/server-racks-in-data-center-klWUhr-wPJ8
- Fixed HTTPS source: https://images.unsplash.com/photo-1506399558188-acca6f8cbf41?fm=jpg&w=1800&q=82
- License at time of integration: https://unsplash.com/license (Unsplash License)
- Subject: racks of servers and computer data storage

The layout, navy/violet translucent masks, background gradients, dimensions
and placement are customized specifically for Multi Backup. The original photo
is not geometrically stretched, and neither banner nor sidebar image uses
automatic cover cropping. The full source composition is first resized with
Pillow ImageOps.contain, then placed on a separate themed canvas; the runtime
also uses contain to scale to the actual viewport.

No photo fetching, telemetry, URL requests or Unsplash dependency occurs when
users run the installed app. GitHub CI downloads the license-confirmed source
during reproducible package builds; the output JPEGs are then bundled locally.
Do not re-license the third-party source photograph as EUPL; EUPL applies to the
application code. Preserve this attribution when publishing the brochures.

A separate original generated artistic concept from the user design iteration
may be retained as a reference; it is not the source of the distributed
licensed-photo composites.
