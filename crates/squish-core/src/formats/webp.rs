use crate::error::SquishError;
use crate::options::SquishOptions;
use image::{DynamicImage, GenericImageView};
use std::path::Path;

/// Static WebP compression. NOTE: does not preserve animated WebP animation
/// (it would produce a single-frame output). Animated-WebP support is planned
/// as follow-up work.
pub fn compress(
    input: &[u8],
    opts: &SquishOptions,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    // Decode whatever raster the caller gave us (could be WebP, PNG, JPEG, etc.
    // since this may be reached via --format conversion).
    let img = image::load_from_memory(input).map_err(|e| SquishError::DecodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;

    encode_raster(&img, opts, path)
}

/// Encode an already-decoded raster as WebP.
pub fn encode_raster(
    img: &DynamicImage,
    opts: &SquishOptions,
    _path: &Path,
) -> Result<Vec<u8>, SquishError> {
    let (w, h) = img.dimensions();
    let rgba = img.to_rgba8().into_raw();

    let encoder = webp::Encoder::from_rgba(&rgba, w, h);

    let encoded = if opts.lossless {
        encoder.encode_lossless()
    } else {
        let q = opts.effective_quality(crate::format::Format::Webp) as f32;
        encoder.encode(q)
    };

    Ok(encoded.to_vec())
}
