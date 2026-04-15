use crate::error::SquishError;
use crate::options::SquishOptions;
use image::ImageEncoder;
use std::path::Path;

/// Compress a PNG. Strategy:
/// - Lossless: oxipng at max optimization.
/// - Lossy: imagequant to quantize to <=256 colors at target quality,
///   then oxipng on the quantized output to strip chunks and finish zlib.
pub fn compress(
    input: &[u8],
    opts: &SquishOptions,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    if opts.lossless {
        return oxipng_pass(input, path);
    }

    let quality = opts.effective_quality(crate::format::Format::Png);
    let quantized = quantize_png(input, quality, path)?;
    oxipng_pass(&quantized, path)
}

fn oxipng_pass(input: &[u8], path: &Path) -> Result<Vec<u8>, SquishError> {
    use oxipng::{optimize_from_memory, Options};
    let opts = Options::max_compression();
    optimize_from_memory(input, &opts).map_err(|e| SquishError::EncodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })
}

fn quantize_png(input: &[u8], quality: u8, path: &Path) -> Result<Vec<u8>, SquishError> {
    use imagequant::Attributes;

    // Decode to RGBA8.
    let img = image::load_from_memory_with_format(input, image::ImageFormat::Png)
        .map_err(|e| SquishError::DecodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?
        .to_rgba8();

    let width = img.width() as usize;
    let height = img.height() as usize;

    // Quantize.
    let mut attrs = Attributes::new();
    attrs
        .set_quality(0, quality)
        .map_err(|e| SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;

    let pixels: Vec<imagequant::RGBA> = img
        .pixels()
        .map(|p| imagequant::RGBA::new(p[0], p[1], p[2], p[3]))
        .collect();

    let mut image = attrs
        .new_image(&pixels[..], width, height, 0.0)
        .map_err(|e| SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;

    let mut res = attrs
        .quantize(&mut image)
        .map_err(|e| SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;

    let (palette, indexed) = res
        .remapped(&mut image)
        .map_err(|e| SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;

    // Encode quantized result back to PNG using image crate's indexed-color encoder.
    // We encode as RGBA8 (expanding the palette) — oxipng will re-encode it as an
    // indexed PNG in the next pass since it's denser.
    let mut rgba = Vec::with_capacity(indexed.len() * 4);
    for idx in &indexed {
        let c = palette[*idx as usize];
        rgba.extend_from_slice(&[c.r, c.g, c.b, c.a]);
    }

    let mut out = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut out);
    encoder
        .write_image(
            &rgba,
            width as u32,
            height as u32,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
    Ok(out)
}
