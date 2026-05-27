# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
