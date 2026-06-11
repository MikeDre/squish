# squish-audio

Audio compression library for [squish](https://github.com/MikeDre/squish) — the local CLI that optimises images, video, audio, and code.

Re-encodes MP3, AAC/M4A, WAV, FLAC, OGG, Opus, AIFF, and WebM-audio through system `ffmpeg`: lossy inputs re-encode at the same codec with sensible quality, lossless inputs convert to Opus by default, tags and album art are preserved (strippable), and `target_size` budgets compute the bitrate from the probed duration.

```rust
use squish_audio::{squish_audio, AudioCodec, AudioOptions};

let opts = AudioOptions {
    codec: Some(AudioCodec::Opus),
    ..Default::default()
};
let result = squish_audio("song.flac".as_ref(), &opts)?;
println!("{:?} → {}", result.codec_used, result.output_path.display());
```

## System dependencies

Requires `ffmpeg` (and `ffprobe`, which ships with it) on PATH.

Most users want the CLI: `brew install mikedre/tap/squish` or `cargo install squish-media-cli`. See the [squish README](https://github.com/MikeDre/squish) for full documentation.

## Licence

MIT
