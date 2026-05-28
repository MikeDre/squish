# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
