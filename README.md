# squish

Super fast local image & media compression on your machine. Takes files or directories, writes `*_squished.*` siblings alongside the originals. Non-destructive - originals are never touched.

## Install

### Quick setup (recommended)

```bash
./scripts/setup.sh    # installs system deps via Homebrew (macOS) or apt (Linux)
cargo install --path crates/squish-cli
```

### Manual setup

Install Rust: https://rustup.rs

Install system dependencies:

- **`gifsicle`** (required for GIF compression)
  - macOS: `brew install gifsicle`
  - Linux: `apt install gifsicle`
- **`libheif` + `x265`** (required for HEIC/HEIF)
  - macOS: `brew install libheif x265`
  - Linux: `apt install libheif-dev libx265-dev`
- **`dav1d`** (required for AVIF decoding)
  - macOS: `brew install dav1d`
  - Linux: `apt install libdav1d-dev`

Then:

```bash
cargo install --path crates/squish-cli
```

If `gifsicle` is missing when you compress a GIF, squish tells you exactly what to install.

## Use

```bash
# Single file
squish dog.png
# → dog_squished.png

# Whole folder, recursively
squish ./assets/ -r

# Convert format while compressing
squish photos/ -r --format webp --quality 75

# Preserve every bit (lossless)
squish logo.svg --lossless

# Preview without writing
squish ./big-folder/ -r --dry-run
```

## Formats

Supported as **input** and **output**: PNG, JPEG, WebP, AVIF, SVG, GIF, HEIC, TIFF (with notes below).

| Format | Library |
|---|---|
| PNG | `oxipng` + `imagequant` |
| JPEG | `mozjpeg` (progressive, optimized Huffman) |
| WebP | `libwebp` |
| AVIF | `ravif` (encode) + `dav1d` (decode) |
| SVG | `usvg` (compact serialization) |
| GIF (static + animated) | `gifsicle -O3` |
| HEIC | `libheif-rs` |
| TIFF | input only — defaults to re-encoding as JPEG; use `--format tiff` to keep TIFF output |

## Flags

```
  -q, --quality <0-100>  Quality override (default: format-specific)
      --lossless         Lossless compression (overrides --quality)
  -f, --format <FORMAT>  Output format; default preserves input format
  -r, --recursive        Recurse into directories
      --force            Overwrite existing _squished files
      --dry-run          Show what would happen; don't write
  -j, --jobs <N>         Parallelism (default: num CPUs)
  -v, --verbose          Per-file output
      --quiet            Errors only
```

## Collision behavior

If `dog_squished.png` already exists, squish writes `dog_squished_2.png`, then `_3`, etc. Pass `--force` to overwrite instead.

## Development

```bash
cargo test              # run all tests
cargo build --release   # optimized binary
```

Test fixtures (`crates/squish-core/tests/fixtures/`) are real-world images used for per-format round-trip tests. See the README there for sources.

## Roadmap

v1 is image-only. Planned follow-ons:

- Video compression (`squish-video` crate, ffmpeg-backed)
- Audio compression (`squish-audio` crate)
- Tauri desktop app sharing the same `squish-core` library

## License

MIT.
