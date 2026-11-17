# Orca patch

Based on the published Slint 1.18.1 software renderer, with upstream license
headers preserved.

Unscaled RGB/RGBA texture rows use contiguous source slices instead of
calculating a fixed-point source address for every pixel. Clipping, transparency,
tiling, scaling and rotation retain the generic sampler where needed. The pixel
comparison test exercises both paths against the original sampler.

RGBA rows group four opaque pixels for conversion and skip fully transparent
groups. Large repaints of scaled RGB textures reuse horizontal sample positions
across rows; the temporary lookup table is capped at 8 KiB. Small dirty regions
keep the original path. Pixel comparisons include RGB, RGBA and RGB565 targets.

Transparent glyph pixels skip blending. Large unscaled tinted texture repaints
reuse a 1 KiB coverage-to-color table instead of premultiplying each pixel.
Unsupported layouts retain the generic sampler. Pixel comparisons cover tinted
textures, alpha maps, signed distance fields and transparent targets.

The renderer includes optional retained scrolling through `cache-rendering-hint`.
Orca's library grids do not enable it; they use normal partial repainting.
On reused framebuffers, unchanged opaque artwork interiors move in place;
backgrounds, transparent edges and newly exposed areas repaint normally.
Property changes, overlapping UI, new buffers, rotation and fractional movement
fall back to normal painting. The cache stores at most 64 visible-image references
and region metadata, without allocating another pixel surface.
Row recycling invalidates copied pixels while retaining opacity checks for
surviving covers; checks for images no longer drawn are discarded.

Validation from the repository root:

```powershell
cargo test --locked --offline -p i-slint-renderer-software
```

Recheck this patch when upgrading Slint and remove the override when an equivalent
optimization is available upstream.
