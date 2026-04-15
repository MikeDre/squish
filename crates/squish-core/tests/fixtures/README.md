# Test fixtures

Small sample files used by per-format round-trip tests. Each sample must be:

1. **Real-world** — synthetic patterns often hit encoder pathological cases.
2. **Compressible** — at encoder quality 90+ (for lossy) or uncompressed (for raster) so tests can assert output < input.
3. **Small** — keep each under 1MB to keep the repo light.

| File | Source | License |
|---|---|---|
| sample.png | picsum.photos (640x480 photo) → PNG via ImageMagick | Public domain (picsum.photos CC0) |
| sample.jpg | picsum.photos (640x480 photo) → JPEG q95 via ImageMagick | Public domain (picsum.photos CC0) |
| sample.webp | picsum.photos master → WebP q90 via cwebp | Public domain (picsum.photos CC0) |
| sample.avif | picsum.photos master → AVIF q90 via ImageMagick | Public domain (picsum.photos CC0) |
| sample.svg | Hand-crafted to include whitespace, comments, unused defs for minification testing | Public domain (this project) |
| sample.gif | picsum.photos master → GIF via ImageMagick | Public domain (picsum.photos CC0) |
| sample_animated.gif | 3 frames of picsum.photos master with color variations | Public domain (picsum.photos CC0) |
| sample.heic | picsum.photos master → HEIC q90 via heif-enc | Public domain (picsum.photos CC0) |
| sample.tiff | picsum.photos master → TIFF via ImageMagick | Public domain (picsum.photos CC0) |

Replace when updating fixtures — do not generate programmatically (synthetic images often don't compress meaningfully).
