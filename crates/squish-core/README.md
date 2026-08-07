# squish-core

Image compression library for [squish](https://github.com/MikeDre/squish) — the local CLI that optimises images, video, audio, and code.

Compresses PNG, JPEG, WebP, AVIF, SVG, GIF, HEIC, and TIFF with pure-Rust encoders where possible (`oxipng` + `imagequant`, `mozjpeg`, `libwebp`, `ravif`, `oxvg_optimiser`), including cross-format conversion, proportional resizing, cropping (an aspect ratio anchored by a gravity, or an exact pixel rect — applied before any resize, and preserving animation for GIF), and `target_size` budgets (binary-searches the quality dial for the highest quality that fits).

SVG input can also be rasterised to any of the other formats via `resvg`, linked in-process — no new system dependency. A vector has no pixel size of its own, so rasterising needs `SquishOptions::width` and/or `height`; give both and the render fits inside that box at the source's own aspect ratio. Unlike the resize path, these upscale, and the never-grow guarantee doesn't apply — a rendered raster is routinely larger than its vector source. Cropping, `target_size`, and quality search all run on the rendered pixels. Converting the other way, raster to SVG, is not supported.

Encoding a transparent image to JPEG (including any SVG render) composites onto a white background rather than discarding the alpha, since JPEG has no alpha channel of its own.

`preview_bytes` renders any supported image — including HEIC, AVIF, and TIFF, which browsers can't display, and SVG, which has no pixels until rendered — as a downscaled JPEG or PNG, reporting both the preview and source dimensions. It takes a `&SquishOptions` so it knows what size to render vector input at. It backs the CLI's interactive crop selector.

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
