# squish-media

Shared ffmpeg plumbing for [squish](https://github.com/MikeDre/squish)'s video and audio crates: binary detection (`check_ffmpeg`), command execution with partial-output cleanup (`run_ffmpeg`), and the common `MediaError` type.

This is an internal support crate for [`squish-video`](https://crates.io/crates/squish-video) and [`squish-audio`](https://crates.io/crates/squish-audio) — it has no user-facing API of its own.

Most users want the CLI: `brew install mikedre/tap/squish` or `cargo install squish-media-cli`. See the [squish README](https://github.com/MikeDre/squish) for full documentation.

## Licence

MIT
