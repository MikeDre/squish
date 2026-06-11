# squish-video

Video compression library for [squish](https://github.com/MikeDre/squish) — the local CLI that optimises images, video, audio, and code.

Re-encodes MP4, WebM, MOV, AVI, MKV, FLV, and DV through system `ffmpeg`: H.265 by default (with the `hvc1` tag so QuickTime/Safari play it), H.264/AV1/VP9 selectable, metadata stripping, a `--fast` stream-copy mode, and `target_size` budgets (ABR bitrate computed from the probed duration, copied audio subtracted, VBV-constrained with shrink-and-retry on overshoot).

```rust
use squish_video::{squish_video, VideoOptions};

let opts = VideoOptions {
    target_size: Some(8_000_000), // fit under 8 MB
    ..Default::default()
};
let result = squish_video("clip.mp4".as_ref(), &opts)?;
println!("saved {:.1}%", result.reduction_percent());
```

## System dependencies

Requires `ffmpeg` (and `ffprobe`, which ships with it) on PATH.

Most users want the CLI: `brew install mikedre/tap/squish` or `cargo install squish-media-cli`. See the [squish README](https://github.com/MikeDre/squish) for full documentation.

## Licence

MIT
