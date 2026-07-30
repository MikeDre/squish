# squish-core

Image compression library for [squish](https://github.com/MikeDre/squish) — the local CLI that optimises images, video, audio, and code.

Compresses PNG, JPEG, WebP, AVIF, SVG, GIF, HEIC, and TIFF with pure-Rust encoders where possible (`oxipng` + `imagequant`, `mozjpeg`, `libwebp`, `ravif`, `oxvg_optimiser`), including cross-format conversion, proportional resizing, cropping (an aspect ratio anchored by a gravity, or an exact pixel rect — applied before any resize, and preserving animation for GIF), and `target_size` budgets (binary-searches the quality dial for the highest quality that fits).

`preview_bytes` renders any supported image — including HEIC, AVIF, and TIFF, which browsers can't display — as a downscaled JPEG or PNG, reporting both the preview and source dimensions. It backs the CLI's interactive crop selector.

```rust
use squish_core::{squish_file, SquishOptions};

let opts = SquishOptions {
    target_size: Some(500_000), // fit under 500 kB
    ..Default::default()
};
let result = squish_file("photo.jpg".as_ref(), &opts)?;
println!("{} → {} bytes", result.input_bytes, result.output_bytes);
```

## System dependencies

- `gifsicle` — GIF compression (subprocess)
- `libheif` + `x265` — HEIC/HEIF encode/decode (linked)
- `dav1d` — AVIF decoding (linked)

Everything else is compiled in.

Most users want the CLI: `brew install mikedre/tap/squish` or `cargo install squish-media-cli`. See the [squish README](https://github.com/MikeDre/squish) for full documentation.

## Licence

MIT
