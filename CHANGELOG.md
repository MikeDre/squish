# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Two-pass video encoding for `--target-size`.** H.264/H.265/VP9 now use
  ffmpeg's native two-pass ABR (an analysis pass to the null muxer, then the
  real encode) as the primary size-targeting strategy, so the output lands on
  the budget in one encode instead of relying on repeated single-pass retries.
  Pass-log files are written to a tempdir, never the source directory. The
  single-pass retry loop is kept as a fallback for SVT-AV1 (whose two-pass is
  awkward) and as an overshoot backstop after the second pass (rarely
  triggered). AV1 rate targeting is best-effort — it can't use VBV — while the
  VBV-capable codecs hit the budget exactly.
- **Shell completions.** `squish completions <bash|zsh|fish>` prints a
  completion script to stdout, generated from the CLI's own flag definitions.
  The Homebrew tap installs completions automatically.
- **Man page.** `squish man` prints the `squish.1` roff source to stdout.
  Included in release tarballs and installed automatically by the Homebrew
  tap.
- **`cargo binstall` support.** `cargo binstall squish-media-cli` fetches the
  prebuilt release binary for macOS/Linux instead of compiling from source.
- **`--json` output mode.** Prints a single machine-readable report (per-file
  bytes in/out, format, saving %, status; totals by kind; errors) to stdout
  instead of the human summary — nothing else touches stdout, so it's safe to
  pipe into `jq` or parse in CI. Works with `--dry-run`. Conflicts with
  `--verbose`/`--quiet`/`--watch`/`--stats`. Exit codes unchanged.
- **`--exclude` globs and ignore-aware walking.** `--exclude <GLOB>`
  (repeatable) skips matching files/dirs during a directory walk, rooted at
  each input path; explicit file arguments are never excluded. `.git`,
  `node_modules`, and `target` are pruned by default (`--no-default-excludes`
  to disable). `--gitignore` opts into also respecting `.gitignore` (and
  `.git/info/exclude`, and the global gitignore) — off by default so existing
  behavior doesn't change. New `exclude = [...]` `squish.toml` key. Applies to
  `--watch` too. Switched the directory walker from `walkdir` to the `ignore`
  crate.
- **`--keep-metadata`.** Preserve EXIF and the ICC colour profile in image
  output where the format supports it (currently JPEG and PNG). Default:
  EXIF stripped, ICC always preserved (colour correctness). Mirrors
  `--strip-tags`' naming for audio, inverted — images strip by default.
  Preserved EXIF has its orientation tag reset to 1, since pixels are always
  corrected before encoding either way (see the orientation fix below).
- **`--preset web` now bounds image height too.** Previously only
  `max-width 1920` was set, so a portrait photo (taller than it is wide)
  passed through unresized. The preset now also sets `max-height 1920`,
  bounding images to a 1920×1920 box on either axis regardless of
  orientation.

### Fixed
- **`--target-size` larger than the input still grew the file.** The
  never-grow guard treated `--target-size` itself as a "legitimate
  conversion" allowed to grow the output, the same as an explicit
  `--format`/`--codec`/resize — but a size *budget* never needs to exceed the
  original, unlike those. A budget bigger than the input (e.g. `--target-size
  1M` on a 40k file) now leaves the file untouched and reports "skipped
  (already optimal)", instead of needlessly re-encoding it larger. Affected
  images, video, and audio identically.
- **`--codec av1 --target-size` failed to encode.** SVT-AV1 rejects the
  `-maxrate`/`-bufsize` VBV constraint outside CRF mode (`Max Bitrate only
  supported with CRF mode`), which errored the whole encode. AV1 now uses plain
  VBR (`-b:v` only) and relies on the retry loop to converge on the budget.
- **JPEG EXIF orientation was silently discarded.** A rotated/flipped JPEG
  (the overwhelming majority of real-world EXIF-orientation use — camera
  photos) decoded as raw upright pixels and re-encoded with no orientation
  tag, baking in the wrong visual framing. Orientation is now always read
  and applied to pixels before encoding.
- **Never-grow guarantee.** squish could silently write an output larger
  than its input (a size guard already existed for SVG only, see v0.3.3).
  Now applies uniformly to images, video, audio, and code: if the encode
  doesn't help and no `--format`/resize/codec conversion was requested, it's
  discarded and the output is left byte-identical to the input, reported as
  "skipped (already optimal)" in the summary and `--json` (`status:
  "skipped"`) instead of a (non-)saving. Safe under `--overwrite` too — the
  original is never lost even though encoders write in place. When a
  conversion *is* requested, growth is allowed (e.g. a tiny PNG icon
  converted to AVIF can legitimately grow), with a `--verbose` note.

## [0.7.0] - 2026-06-15

### Added
- **`--quality auto`.** Perceptual auto-quality: binary-searches for the lowest
  encoder quality whose output is still visually lossless (SSIMULACRA2 ≥ 90).
  Image formats with a quality dial (JPEG/WebP/AVIF) only; conflicts with
  `--target-size`. Settable device-wide as `quality = "auto"` in config and via
  the `squish config` wizard.
- **`squish doctor`.** Prints a capability report — built-in formats plus the
  external tools (ffmpeg, ffprobe, gifsicle) with versions and install hints for
  anything missing. Always exits 0.
- **`--preset web`.** A destination preset bundling web-delivery defaults —
  images resize to 1920px wide, convert to WebP, and use `--quality auto`;
  video re-encodes to H.264. Overridable: any explicit flag wins over the
  preset. Applies only to the kinds present (never errors on a batch missing
  images or video).

[0.7.0]: https://github.com/MikeDre/squish/compare/v0.6.0...v0.7.0

## [0.6.0] - 2026-06-14

### Added
- **Finder Quick Action (macOS).** `squish finder-action install` adds a
  "Squish" entry to Finder's right-click → Quick Actions menu. Zero UI:
  squishes the selected files/folders (media only — images, video, audio)
  with the usual non-destructive defaults, posting start and finish
  notifications with the savings. `squish finder-action uninstall` removes
  it. The Homebrew formula now points at the command post-install.
- **`--kinds` flag.** Restrict a run to specific file kinds
  (`--kinds image,video,audio`). Unknown kind names are an error. CLI-only
  by design — not a `squish.toml` key.
- **`squish config` wizard.** An interactive command that steps through the
  common defaults (quality, format, suffix, recursive, strip-tags, overwrite)
  and writes the config file — global by default, `--local` for
  `./squish.toml`. Pre-fills existing values; Enter keeps, `-` clears.
- **`overwrite` config key.** Default `-o` (replace originals in place) from
  the config file. CLI flags still win, and it stays mutually exclusive with
  `suffix`.

[0.6.0]: https://github.com/MikeDre/squish/compare/v0.5.0...v0.6.0

## [0.5.0] - 2026-06-11

### Added
- **`squish.toml` config files.** Defaults are read from the nearest
  `squish.toml` (walking up from the current directory) merged over a global
  config at the platform config dir. Precedence: CLI flags > project config >
  global config; `--no-config` skips both. Keys mirror CLI flag names, with
  `[video]`/`[audio]`/`[code]` tables for kind-specific options (including
  separate per-kind codecs). Unknown keys are an error. Rate control is
  all-or-nothing: any explicit rate flag on the CLI disables all config
  rate-control keys for that run.
- **`--watch` mode.** `squish ./assets/ -r --watch` runs an initial pass,
  then keeps watching and squishes files as they appear or change (debounced).
  Squish never re-squishes its own outputs: `_suffix`/`_suffix_N`, `.min.*`,
  and `.map` names are filtered, and self-written paths (e.g. with
  `--overwrite`) are skipped once.
- **GitHub Action.** `uses: MikeDre/squish@v0.5.0` downloads the platform
  binary, installs runtime dependencies (skippable), and squishes the given
  paths on ubuntu/macos runners — handy for optimising assets in CI.

[0.5.0]: https://github.com/MikeDre/squish/compare/v0.4.0...v0.5.0

## [0.4.0] - 2026-06-10

### Added
- **`--target-size` — compress to a size budget.** `squish clip.mp4
  --target-size 8M` fits each file under a per-file byte budget (decimal
  units: `500k`, `1.5M`, `2g`). Images binary-search the quality dial for the
  highest quality that fits (SVG/TIFF outputs have no dial and warn when over
  budget; unreachable budgets write the smallest attempt plus a warning).
  Video computes an ABR bitrate from the probed duration (copied audio
  subtracted, VBV-constrained, with up to three shrink-and-retry passes on
  overshoot). Audio computes a bitrate from duration with 5% container
  headroom. Conflicts with `--quality`/`--lossless`/`--bitrate`/`--fast`;
  rejected for code-only batches and lossless audio codecs.
- **Universal `--format`.** The `--format` flag now accepts image, video, and
  audio formats and applies each to the matching input kind (e.g.
  `squish media/ -r --format webp` converts images while videos and audio use
  their defaults; `squish trailer.mov --format mp4` re-encodes the container).
  Cross-kind validation rejects a `--format` whose kind has no matching files
  in the batch.

### Changed
- **squish now builds on stable Rust (MSRV 1.95).** The `if_let_guard`
  feature that forced a nightly toolchain pin stabilised in Rust 1.95.0, so
  `rust-toolchain.toml` tracks `stable` and the workspace `rust-version` is
  1.95. `cargo install squish-media-cli` no longer needs nightly.
- **Prebuilt binaries.** Version tags now build release binaries for
  macOS (arm64/x64) and Linux (x64/arm64) via GitHub Actions and attach them
  to the GitHub release.

[0.4.0]: https://github.com/MikeDre/squish/compare/v0.3.5...v0.4.0

## [0.3.5] - 2026-05-28

### Fixed
- **Integration tests no longer pollute the local usage ledger.** Before this
  fix, `cargo test --workspace` (and `cargo test -p squish-media-cli`) ran the
  `squish` binary against audio/image/video/code fixtures via `assert_cmd`,
  and those invocations appended records to the developer's real
  `~/Library/Application Support/squish/usage.jsonl`. The integration tests
  now route all binary invocations through a `bin()` helper that sets
  `SQUISH_NO_STATS=1`, and a meta-test prevents any future direct
  `cargo_bin("squish")` call from being reintroduced. Only the squish
  maintainer/contributor workflow was affected; regular users installing via
  `cargo install` were not.

[0.3.5]: https://github.com/MikeDre/squish/compare/v0.3.4...v0.3.5

## [0.3.4] - 2026-05-28

### Added
- **`--stats` usage report.** A new local-only usage ledger records each
  `squish` batch (files squished + input/output bytes, broken down by image /
  video / audio / code). Running `squish --stats` prints a small report with
  two windows: **this month** and **all time**, each showing total files
  squished, bytes saved, and a per-kind breakdown. Nothing leaves the machine.
- **Opt-out.** Recording is on by default. Use `--no-stats` to skip a single
  run, or set `SQUISH_NO_STATS=1` to disable globally. Stats are also skipped
  for `--dry-run` and for batches that produced no successful squishes.
- The usage ledger lives at the platform data dir
  (`~/Library/Application Support/squish/usage.jsonl` on macOS, the equivalent
  XDG path on Linux, `%APPDATA%\squish\usage.jsonl` on Windows). One JSON
  Lines record per batch; ~200 bytes/record. Atomic `O_APPEND` writes; future
  schema changes detected via a `v` field on each record.

[0.3.4]: https://github.com/MikeDre/squish/compare/v0.3.3...v0.3.4

## [0.3.3] - 2026-05-28

### Fixed
- **SVG minification.** The SVG handler previously used `usvg` (a rendering
  library), which on already-clean SVGs produced output that was *larger* than
  the input (+17.2% on a representative file), inlined inherited attributes,
  added an empty `<defs/>`, and **dropped `viewBox`** — a correctness
  regression for downstream responsive scaling. Replaced with
  `oxvg_optimiser` (a Rust port of SVGO): now matches svgo within 0.5% on the
  same input (−42.3% size reduction) and preserves `viewBox`. A size guard
  also ensures squish never grows an already-minified SVG.

### Changed (internal)
- `lightningcss` pinned to `=1.0.0-alpha.65` and `minify-html` pinned to
  `=0.16.0` in `squish-code`. Forced by an upstream constraint:
  `oxvg_optimiser 0.0.5` unconditionally enables `lightningcss`'s `grid`
  feature, which was removed after `1.0.0-alpha.65`. The pin will be unwound
  once `oxvg` releases a version that drops the `grid` feature requirement.
  `squish-code`'s tests pass against the older versions; the API surface used
  is stable across the affected releases.

[0.3.3]: https://github.com/MikeDre/squish/compare/v0.3.2...v0.3.3

## [0.3.2] - 2026-05-27

### Added
- **`.dv` (Digital Video) support.** DV files are recognized as a transcode-only
  input and always re-encoded to MP4/H.265. `--fast` (stream-copy) is
  overridden for DV since DV cannot be muxed into MP4, with a note printed when
  it happens.
- **`-o, --overwrite` flag.** Replaces each input file in place with its
  squished version instead of writing a `_squished` copy. Specified once, it
  applies to every input. Files whose squish would change the extension (e.g.
  `.dv`→`.mp4`, `.ts`→`.js`, `.flac`→`.opus`, or `--format` conversions) are
  skipped with a clear error and their originals left untouched. Video/audio
  encode to a temp file and atomically replace the original only on success, so
  a failed run never destroys the input.

[0.3.2]: https://github.com/MikeDre/squish/compare/v0.3.1...v0.3.2
