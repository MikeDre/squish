# squish

Super fast local image & media compression on your machine. Takes files or directories, writes `*_squished.*` siblings alongside the originals. Non-destructive — originals are never touched.

## Install

### Pre-built binary (macOS)

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/MikeDre/squish/releases/latest/download/squish-cli-installer.sh | sh
```

This downloads the latest release for your architecture (Apple Silicon or Intel) and installs it to `~/.cargo/bin`.

After installing, you still need the system dependencies for full format support (see below).

### Build from source

**1. Install Rust** (skip if `rustc --version` already works):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://rustup.rs | sh
```

Once the installer finishes, open a **new terminal** (or run `source ~/.cargo/env`) so that `cargo` is available on your PATH.

**2. Install system deps and build:**

```bash
./scripts/setup.sh    # installs system deps via Homebrew (macOS) or apt (Linux)
cargo install --path crates/squish-cli
```

**3. Make sure `squish` is on your PATH:**

`cargo install` places the binary in `~/.cargo/bin`. If `squish` isn't found after installation, add that directory to your shell profile and reload it:

```bash
# bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc && source ~/.bashrc

# zsh
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc && source ~/.zshrc
```

Then verify with `squish --version`.

### System dependencies

GIF and HEIC support require external libraries. Install them for full format coverage:

- **`gifsicle`** (required for GIF compression)
  - macOS: `brew install gifsicle`
  - Linux: `apt install gifsicle`
- **`libheif` + `x265`** (required for HEIC/HEIF)
  - macOS: `brew install libheif x265`
  - Linux: `apt install libheif-dev libx265-dev`
- **`dav1d`** (required for AVIF decoding)
  - macOS: `brew install dav1d`
  - Linux: `apt install libdav1d-dev`
- **`ffmpeg`** (required for video compression)
  - macOS: `brew install ffmpeg`
  - Linux: `apt install ffmpeg`

If a dependency is missing when you need it, squish tells you exactly what to install.

## Use

### Images

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

### Video

```bash
# Compress a video (defaults to H.265)
squish video.mp4
# → video_squished.mp4

# Use H.264 instead
squish video.mp4 --codec h264

# Fast mode — optimize without re-encoding
squish video.mp4 --fast

# Mixed batch — images and videos together
squish ./media/ -r
# → Squished 8 files (5 images, 3 videos) · 120.3 MB → 34.1 MB (-71.7%)
```

## Formats

### Images

Supported as **input** and **output**: PNG, JPEG, WebP, AVIF, SVG, GIF, HEIC, TIFF.

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

### Video

Supported containers: MP4, WebM, MOV, AVI, MKV, FLV. Requires system `ffmpeg`.

| Codec | Flag | Notes |
|---|---|---|
| H.265 (HEVC) | `--codec h265` (default) | ~50% smaller than H.264 |
| H.264 (AVC) | `--codec h264` | Maximum compatibility |
| AV1 | `--codec av1` | Best compression, slower encode |
| VP9 | auto for `.webm` | Selected automatically for WebM containers |
| Copy | `--fast` | No re-encode, strips metadata only |

Audio streams are copied as-is (no audio re-encoding).

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
      --codec <CODEC>    Video codec: h264, h265, av1 (default: h265)
      --fast             Video: optimize without re-encoding
```

## Collision behavior

If `dog_squished.png` already exists, squish writes `dog_squished_2.png`, then `_3`, etc. Pass `--force` to overwrite instead.

## Development

```bash
cargo test              # run all tests
cargo build --release   # optimized binary
```

Test fixtures are in `crates/squish-core/tests/fixtures/` (images) and `crates/squish-video/tests/fixtures/` (videos). See the README in each for sources.

## Roadmap

- Audio compression (`squish-audio` crate)
- Image resizing (`--max-width`)
- Tauri desktop app sharing the same `squish-core` library

## License

MIT.
