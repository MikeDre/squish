use crate::error::SquishError;
use crate::options::SquishOptions;
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageEncoder};
use std::io::Cursor;
use std::path::Path;

/// Compress a PNG. Strategy:
/// - Lossless: oxipng at max optimization, operating directly on the source
///   bytes (chunks and all — no decode step happens on this path).
/// - Lossy: imagequant to quantize to <=256 colors at target quality,
///   then oxipng on the quantized output to strip chunks and finish zlib.
///
/// EXIF is stripped by default (`oxipng_pass`'s `keep_metadata` controls
/// this); the ICC colour profile is always preserved. PNG orientation via
/// EXIF is vanishingly rare in practice (unlike JPEG) and isn't handled here.
pub fn compress(input: &[u8], opts: &SquishOptions, path: &Path) -> Result<Vec<u8>, SquishError> {
    if opts.lossless {
        return oxipng_pass(input, opts.keep_metadata, path);
    }

    let quality = opts.effective_quality(crate::format::Format::Png);
    let (exif, icc) = read_metadata(input, path)?;
    let quantized = quantize_png(
        input,
        quality,
        opts.keep_metadata,
        exif.as_deref(),
        icc.as_deref(),
        path,
    )?;
    oxipng_pass(&quantized, opts.keep_metadata, path)
}

/// Encode an already-decoded raster as PNG. Used for cross-format conversions
/// where the source was decoded from another format, so there's no
/// PNG-shaped EXIF/ICC of its own to carry over.
pub fn encode_raster(
    img: &DynamicImage,
    opts: &SquishOptions,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    let (w, h) = img.dimensions();
    let rgba = img.to_rgba8().into_raw();

    // First serialize to a PNG so we have bytes to hand to imagequant / oxipng.
    let raw_png = encode_rgba_to_png(&rgba, w, h, None, None, path)?;

    if opts.lossless {
        return oxipng_pass(&raw_png, opts.keep_metadata, path);
    }

    let quality = opts.effective_quality(crate::format::Format::Png);
    let quantized = quantize_png(&raw_png, quality, opts.keep_metadata, None, None, path)?;
    oxipng_pass(&quantized, opts.keep_metadata, path)
}

/// (EXIF, ICC profile), both `None` if absent.
type Metadata = (Option<Vec<u8>>, Option<Vec<u8>>);

/// Read EXIF and ICC profile from a PNG via `image`'s decoder trait (the
/// `eXIf`/`iCCP` ancillary chunks) — independent of `quantize_png`'s
/// convenience `load_from_memory_with_format` decode, which discards both.
fn read_metadata(input: &[u8], path: &Path) -> Result<Metadata, SquishError> {
    let mut decoder = image::codecs::png::PngDecoder::new(Cursor::new(input)).map_err(|e| {
        SquishError::DecodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        }
    })?;
    let exif = decoder
        .exif_metadata()
        .map_err(|e| SquishError::DecodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
    let icc = decoder
        .icc_profile()
        .map_err(|e| SquishError::DecodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
    Ok((exif, icc))
}

fn encode_rgba_to_png(
    rgba: &[u8],
    width: u32,
    height: u32,
    exif: Option<&[u8]>,
    icc: Option<&[u8]>,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    let mut out = Vec::new();
    let mut encoder = image::codecs::png::PngEncoder::new(&mut out);
    if let Some(icc) = icc {
        encoder
            .set_icc_profile(icc.to_vec())
            .map_err(|e| SquishError::EncodeFailed {
                path: path.to_path_buf(),
                source: Box::new(e),
            })?;
    }
    if let Some(exif) = exif {
        encoder
            .set_exif_metadata(exif.to_vec())
            .map_err(|e| SquishError::EncodeFailed {
                path: path.to_path_buf(),
                source: Box::new(e),
            })?;
    }
    encoder
        .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
        .map_err(|e| SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
    Ok(out)
}

/// `keep_metadata=false` strips every non-critical chunk except `iCCP` (the
/// colour profile, always preserved); `true` preserves everything present
/// (matches oxipng's own permissive default).
fn oxipng_pass(input: &[u8], keep_metadata: bool, path: &Path) -> Result<Vec<u8>, SquishError> {
    use oxipng::{indexset, optimize_from_memory, Options, StripChunks};
    let mut opts = Options::max_compression();
    opts.strip = if keep_metadata {
        StripChunks::None
    } else {
        StripChunks::Keep(indexset! { *b"iCCP" })
    };
    optimize_from_memory(input, &opts).map_err(|e| SquishError::EncodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })
}

#[allow(clippy::too_many_arguments)]
fn quantize_png(
    input: &[u8],
    quality: u8,
    keep_metadata: bool,
    exif: Option<&[u8]>,
    icc: Option<&[u8]>,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
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

    let exif_to_write = if keep_metadata { exif } else { None };
    encode_rgba_to_png(&rgba, width as u32, height as u32, exif_to_write, icc, path)
}
