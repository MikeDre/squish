# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
