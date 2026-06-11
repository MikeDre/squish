# squish

Super fast local file optimisation. Compresses images, video, and audio; minifies JS, TS, CSS, HTML, and JSON — all from one CLI, no servers, no uploads. Non-destructive by default: writes `*_squished.*` siblings, or replaces in place with `-o`.

```bash
# Images — single file, folders, format conversion, size budgets
squish dog.png
squish ./assets/ -r --format webp --quality 75
squish hero.jpg --target-size 500k

# Video — H.265 by default, or fit under an upload limit
squish video.mp4
squish clip.mp4 --target-size 8M

# Audio — re-encode, convert lossless to Opus, budget by size
squish track.mp3
squish song.flac --format opus

# Code — minify a dist folder, no Node required
squish dist/ -r

# How much have I saved?
squish --stats
```

## Install

```bash
# Homebrew (macOS) — installs all system dependencies too
brew install mikedre/tap/squish

# Cargo (stable Rust 1.95+)
cargo install squish-media-cli
```

Prebuilt binaries for macOS (arm64/x64) and Linux (x64/arm64) are on the [releases page](https://github.com/MikeDre/squish/releases).

Full format support needs `ffmpeg` (video/audio), `gifsicle` (GIF), `libheif` + `x265` (HEIC), and `dav1d` (AVIF decode) — squish tells you exactly what to install if something is missing.

## Formats

- **Images:** PNG, JPEG, WebP, AVIF, SVG, GIF, HEIC, TIFF — pure-Rust encoders (`mozjpeg`, `oxipng` + `imagequant`, `ravif`, `oxvg_optimiser`)
- **Video:** MP4, WebM, MOV, AVI, MKV, FLV, DV — H.265/H.264/AV1/VP9 via ffmpeg
- **Audio:** MP3, AAC, WAV, FLAC, OGG, Opus, AIFF — tags and album art preserved
- **Code:** JS/TS/JSX, CSS, HTML, JSON — `oxc_minifier`, `lightningcss`, `minify-html`

See the [full documentation](https://github.com/MikeDre/squish) for every flag and format detail.

## Licence

MIT
