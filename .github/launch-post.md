# Launch post drafts

Working drafts for the public launch. Adjust voice as needed — these aim for
the "built a thing, here's what it does and how" register that lands well on
both forums. Post once CI is green and the brew install has been verified on a
clean machine.

---

## Show HN

**Title:** Show HN: Squish – one local CLI to compress images, video, audio and minify code

**Body:**

I kept bouncing between TinyPNG tabs, ImageOptim, ad-hoc ffmpeg incantations,
and Node minifier configs — so I built one CLI that handles all of it locally.
No uploads, no servers, no Node runtime.

    brew install mikedre/tap/squish
    squish ./assets/ -r
    # → Squished 7 files · 5.5 MB → 1.5 MB (-72.4%) · 795ms

What it does:

- **Images** — PNG/JPEG/WebP/AVIF/SVG/GIF/HEIC/TIFF via mozjpeg, oxipng +
  imagequant, ravif, and an SVGO-equivalent — mostly pure Rust, statically
  compiled in.
- **Video/audio** — H.265/H.264/AV1/VP9 and MP3/AAC/Opus/FLAC via your
  system ffmpeg, tags and album art preserved.
- **Code** — JS/TS/CSS/HTML/JSON minification via oxc and lightningcss; no
  node_modules anywhere.
- **`--target-size 8M`** — fits a file under an upload limit. Images
  binary-search the quality dial; video/audio compute bitrate from the
  probed duration.
- Non-destructive by default (`*_squished.*` siblings), `-o` to replace in
  place, `--dry-run` to preview, `--stats` for a local-only savings ledger.

Rust workspace, MIT licensed. Prebuilt binaries for macOS/Linux or
`cargo install squish-media-cli` (stable toolchain).

Repo: https://github.com/MikeDre/squish

Happy to answer questions — particularly interested in what formats/flags
people are missing.

---

## r/rust

**Title:** squish 0.4.0 — local file optimisation CLI (images, video, audio, code) in Rust

**Body:**

squish is a workspace of six crates that compresses images, re-encodes
video/audio through ffmpeg, and minifies JS/TS/CSS/HTML/JSON — one CLI,
everything local.

New in 0.4.0:

- `--target-size 500k` — fit any file under a byte budget. Images
  binary-search the encoder's quality dial (~7 passes); video computes an
  ABR bitrate from ffprobe duration minus copied audio streams, with
  shrink-and-retry on overshoot.
- `--format` now works across kinds — `squish media/ -r --format webp`
  converts the images in a mixed batch while video/audio keep their
  defaults.
- Builds on stable Rust (1.95) — the `if_let_guard` stabilisation finally
  let us drop the nightly pin that oxc_transformer forced.
- Prebuilt binaries + a Homebrew tap.

Crate highlights for the Rust-curious: mozjpeg/oxipng/imagequant/ravif for
images (no C system deps beyond libheif/dav1d), oxc_minifier +
lightningcss for code (TS minified without Node), thin ffmpeg subprocess
plumbing for media. 425 tests, TDD throughout.

Repo: https://github.com/MikeDre/squish · crates.io: squish-media-cli

Feedback and PRs welcome.
