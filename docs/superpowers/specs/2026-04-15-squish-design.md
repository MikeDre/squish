# squish — Design Spec

**Date:** 2026-04-15
**Status:** Approved, pending implementation plan

## Overview

`squish` is a CLI tool that compresses and optimizes image files — similar in spirit to [tinypng.com](https://tinypng.com), but running locally and supporting more formats. It accepts individual files or directories, writes compressed copies alongside the originals with a `_squished` suffix, and never modifies the original.

The project is built in Rust as a Cargo workspace with the compression logic isolated in a library crate (`squish-core`), so the same core can later be consumed by a Tauri desktop app without rework. Video and audio support are explicitly out of scope for v1 but are anticipated as sibling crates later (`squish-video`, `squish-audio`).

## Goals

- Local, offline image compression for all major formats.
- Non-destructive: originals are never overwritten.
- Sensible defaults; CLI runs with zero flags in the common case.
- Library-first architecture — CLI is a thin wrapper, Tauri will be a thin wrapper, future video/audio crates share the workspace.
- Fast: parallel processing of batches.

## Non-Goals (v1)

- Video compression (mp4, mov, webm).
- Audio compression (mp3, wav, flac, aac).
- Desktop GUI / Tauri UI.
- Pre-built binary releases or installer packaging.
- Auto-selecting the "best" output format per image.
- Graphical progress bars (plain text progress only).
- Configurable metadata handling — v1 strips EXIF/XMP by default, no flag.

## Supported Formats

| Format | Input | Output | Library / Tooling |
|---|---|---|---|
| PNG | ✅ | ✅ | `oxipng` + `imagequant` |
| JPEG (`.jpg` and `.jpeg`) | ✅ | ✅ | `mozjpeg` |
| WebP (static) | ✅ | ✅ | `libwebp-sys` |
| WebP (animated) | ✅ | ✅ | `libwebp-sys` (animation API) |
| AVIF | ✅ | ✅ | `ravif` / `libavif` |
| SVG | ✅ | ✅ | `usvg` + text minification |
| GIF (static + animated) | ✅ | ✅ | `gifsicle` sidecar binary |
| HEIC / HEIF | ✅ | ✅ | `libheif-rs` (needs `libheif` + `x265` system libs) |
| TIFF | ✅ | → JPEG by default | `image` crate (decode only) |
| BMP and other recognized types | ✅ (log + skip) | — | — |

Format detection uses extension first, with magic-byte sniffing as a secondary check so mislabeled files still route correctly. Both `.jpg` and `.jpeg` resolve to the JPEG pipeline.

### Build / distribution implications

- **HEIC** requires `libheif` + `x265` system libraries at build time. Straightforward on macOS (`brew install libheif x265`) and Linux (`apt install libheif-dev`), fiddly on Windows. Documented in README.
- **gifsicle** is a standalone binary, not a Rust crate. The CLI requires it on `PATH`; missing-dep errors are caught and reported with install hints. For the future Tauri app, gifsicle will be bundled as a sidecar binary.
- `libwebp` and `libavif` are vendored via their Rust wrappers — no separate install.

## Architecture

### Workspace layout

```
squish/
├── Cargo.toml                    # workspace manifest
├── README.md
├── docs/superpowers/specs/       # design docs
└── crates/
    ├── squish-core/              # pure-Rust library; no CLI deps
    │   ├── Cargo.toml
    │   ├── src/
    │   │   ├── lib.rs            # public API
    │   │   ├── options.rs        # SquishOptions struct
    │   │   ├── result.rs         # SquishResult struct
    │   │   ├── error.rs          # SquishError enum
    │   │   ├── format.rs         # format detection + dispatch
    │   │   ├── naming.rs         # output path derivation
    │   │   └── formats/
    │   │       ├── png.rs
    │   │       ├── jpeg.rs
    │   │       ├── webp.rs
    │   │       ├── avif.rs
    │   │       ├── svg.rs
    │   │       ├── gif.rs        # shells out to gifsicle
    │   │       ├── heic.rs
    │   │       └── tiff.rs
    │   └── tests/
    │       └── fixtures/         # one small sample per format
    └── squish-cli/               # thin CLI wrapper
        ├── Cargo.toml
        └── src/
            └── main.rs           # clap arg parsing, walk, call core
```

### Hard separation rule

`squish-cli` contains **no format logic, no naming logic, no compression decisions**. It parses args, walks paths, hands absolute paths + a `SquishOptions` struct to `squish-core`, and renders results. Everything else lives in the core library. This discipline keeps the Tauri migration a matter of writing a second thin wrapper; it also keeps the library honest about its public surface.

### Future crates (not v1, but layout supports)

- `squish-video` — ffmpeg-backed video compression.
- `squish-audio` — ffmpeg / opus / lame-backed audio compression.
- `squish-tauri` — Tauri command handlers wrapping `squish-core`.

## Library API (`squish-core`)

```rust
pub struct SquishOptions {
    pub quality: Option<u8>,          // 0-100; None = format default
    pub lossless: bool,                // overrides quality
    pub output_format: Option<Format>, // None = preserve input format
    pub force_overwrite: bool,         // if false, use numeric suffix on collision
}

pub enum Format {
    Png, Jpeg, Webp, Avif, Svg, Gif, Heic, Tiff,
}

pub struct SquishResult {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub format_in: Format,
    pub format_out: Format,
    pub duration: Duration,
}

#[derive(thiserror::Error, Debug)]
pub enum SquishError {
    UnsupportedFormat { path: PathBuf, reason: String },
    DecodeFailed { path: PathBuf, source: Box<dyn Error> },
    EncodeFailed { path: PathBuf, source: Box<dyn Error> },
    Io(#[from] std::io::Error),
    MissingDependency { name: String }, // e.g. gifsicle not on PATH
}

/// Compress a single file. Synchronous.
pub fn squish_file(
    input: &Path,
    opts: &SquishOptions,
) -> Result<SquishResult, SquishError>;

/// Detect format from extension + magic bytes. Pure, no I/O.
pub fn detect_format(path: &Path, bytes_head: &[u8]) -> Option<Format>;
```

That is the complete public surface for v1. No directory walking, no progress callbacks, no stdout — all of that is a CLI concern. Tauri command handlers will call `squish_file` directly.

## CLI (`squish-cli`)

### Command shape

```
squish [OPTIONS] <PATHS>...

ARGS:
  <PATHS>...                 One or more files or directories

OPTIONS:
  -q, --quality <0-100>      Quality override (default: format-specific)
      --lossless             Lossless compression (overrides --quality)
  -f, --format <FORMAT>      Output format (png|jpg|jpeg|webp|avif|svg|gif|heic|tiff)
                             Default: preserve input format
  -r, --recursive            Recurse into directories
      --force                Overwrite existing _squished files instead of numbering
      --dry-run              Show what would happen; don't write anything
  -j, --jobs <N>             Parallelism (default: num CPUs)
  -v, --verbose              Per-file output
      --quiet                Errors only
  -h, --help
  -V, --version

EXAMPLES:
  squish dog.png
  squish ./assets/ -r
  squish photos/ -r --format webp --quality 75
  squish logo.svg --lossless
```

`-q` is reserved for `--quality` (higher-frequency flag). `--quiet` has no short flag to avoid conflict.

### Default output (non-verbose)

A running summary as files complete, plus a final report:

```
Squished 47 files · 38.2 MB → 11.6 MB (-69.6%) · 3.2s
Skipped 2 (unrecognized: thumbs.db, .DS_Store)
```

**Verbose** (`-v`): one line per file with before/after/savings.
**Quiet** (`--quiet`): errors only.

## Behaviors

### Output path derivation

Pure function in `squish-core::naming::derive_output_path(input, output_format, force_overwrite) -> PathBuf`.

1. Strip input extension. Append `_squished`. Append `.{output_format}`.
2. If that path doesn't exist, use it.
3. If `force_overwrite` is true, use it anyway.
4. Otherwise try `_squished_2.{ext}`, `_squished_3.{ext}`, … and use the first free slot.

Examples:
- `dog.png` → `dog_squished.png`
- `dog.png --format webp` → `dog_squished.webp`
- Second run on same folder → `dog_squished_2.png`, third run → `_squished_3.png`.
- `photo.jpeg` → `photo_squished.jpeg` (preserves input's chosen extension spelling).

### Directory walking (CLI only)

- Default: top-level only.
- `-r` / `--recursive`: recurse.
- Symlinks: not followed.
- Hidden/dotfiles: processed if they have a recognized extension (matches `cp` behavior).
- `*_squished.*` files are treated as regular inputs — no filtering. Re-running on the same directory will produce `dog_squished_squished.png` etc. This is intentional, chosen for deterministic "do exactly what I said."
- Unrecognized extensions: logged at info level, skipped.

### Concurrency

- `rayon` thread pool; `--jobs N` controls size, default `num_cpus::get()`.
- Each file is an independent unit of work.
- Per-file errors do not abort the batch — collected and reported at the end.
- Directory walk produces the worklist eagerly, then `par_iter` processes it.

### Error handling (three tiers)

1. **Fatal (exit 2):** arg parse errors, missing input path, permission denied on root. Abort before any work.
2. **Per-file failures (exit 1 if any failed):** decode/encode failure, missing sidecar binary (`gifsicle`), etc. Log, continue the batch, summarize at end.
3. **All clean (exit 0).**

Library errors use `thiserror`; CLI formats them for users with `anyhow`. Missing-dep example:

```
ERROR: photo.gif — gifsicle not found on PATH.
       Install: brew install gifsicle (macOS) / apt install gifsicle (Linux)
```

### "Squishing made it bigger" guard

When the compressed output is larger than the input (rare; happens with already-optimal small images), v1 **keeps the output anyway** and warns in verbose mode. Deterministic behavior > helpful heuristics. A `--skip-if-larger` flag can be added later if this proves annoying in practice.

### `--dry-run`

Walks the worklist, runs format detection, prints would-be output paths + planned actions. Never opens an encoder, never writes.

### Metadata

EXIF / XMP / ICC color profiles: stripped by default in v1. No flag. (Can be made configurable later if a real use case appears.)

## Testing

### `squish-core`

- **Unit tests** for pure functions: format detection, path derivation, options validation. Fast, no I/O.
- **Per-format round-trip integration tests**: one small fixture per format in `tests/fixtures/`. For each, compress → read back → assert the output decodes successfully + output bytes are strictly smaller than input. Fixtures are chosen to be compressible (normal photos/graphics, not already-optimal tiny assets), so this assertion is safe even though the library itself does **not** enforce size reduction at runtime (see "Squishing made it bigger" guard). This avoids byte-exact golden tests that rot across encoder versions.
- **Error-path tests**: corrupted bytes, unsupported format, missing sidecar (mocked via `PATH` manipulation).

### `squish-cli`

- `assert_cmd` + `predicates` end-to-end tests.
- Fixture directory with mixed file types. Run the binary. Assert on stdout + exit code + presence of output files.
- Coverage matrix: single file, directory non-recursive, directory recursive, `--force`, collision numbering, `--dry-run`, unrecognized extensions, one failing file in an otherwise-successful batch.

### CI

None in v1. Contract is `cargo test` runs green. CI + release automation are a later concern, paired with pre-built binary distribution.

## Distribution (v1)

- `cargo install --path crates/squish-cli` for local use.
- README documents system dependencies:
  - `gifsicle` (required if compressing GIFs)
  - `libheif` + `x265` (required if compressing HEIC/HEIF)
  - Platform install commands (Homebrew, apt).
- No pre-built binaries in v1.

## Implementation Milestones

Ordered so each milestone leaves the repo in a committable, testable state. The implementation plan will break these into individual tasks.

1. **Workspace scaffold** — two-crate workspace, `main.rs` that parses args and prints the parsed intent. No format work yet.
2. **Core types & naming** — `SquishOptions`, `SquishResult`, `SquishError`, `derive_output_path`, `detect_format`. All pure, all unit-tested.
3. **PNG + JPEG** — first real formats end-to-end. Proves the architecture. Vertical slice before going wide.
4. **WebP + AVIF** — modern formats.
5. **SVG** — text-based pipeline, different from raster.
6. **GIF via gifsicle** — first external binary dependency; missing-dep error path exercised.
7. **HEIC + TIFF** — last to land because HEIC has the heaviest build deps.
8. **Concurrency + progress output** — rayon integration and the final summary line.
9. **`--dry-run` and polish** — help text, error messages, README with install instructions.

## Open Questions

None — design approved end-to-end in brainstorming.

## Future Work (deferred, not v1)

- Video compression crate (`squish-video`) using ffmpeg.
- Audio compression crate (`squish-audio`) using ffmpeg / opus / lame.
- Tauri desktop app wrapping `squish-core`.
- Pre-built binary releases via `cargo-dist` + GitHub Actions.
- Smart auto-format selection ("pick the smallest valid output").
- Configurable metadata retention flags.
- `--skip-if-larger` if the "output grew" case becomes a real issue.
