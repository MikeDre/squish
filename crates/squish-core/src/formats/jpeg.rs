use crate::error::SquishError;
use crate::options::SquishOptions;
use std::path::Path;

/// Compress a JPEG. Uses mozjpeg — its default settings are already a 15-25%
/// improvement over libjpeg-turbo at the same visual quality.
pub fn compress(
    input: &[u8],
    opts: &SquishOptions,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    let quality = opts.effective_quality(crate::format::Format::Jpeg);

    // Decode the input to RGB with mozjpeg's decoder.
    let decomp = mozjpeg::Decompress::new_mem(input).map_err(|e| SquishError::DecodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;

    let mut started = decomp.rgb().map_err(|e| SquishError::DecodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;

    let width = started.width();
    let height = started.height();
    let pixels: Vec<u8> = started
        .read_scanlines()
        .map_err(|e| SquishError::DecodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
    started.finish().map_err(|e| SquishError::DecodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;

    encode_rgb_pixels(&pixels, width, height, quality, path)
}

/// Encode raw interleaved RGB8 pixels as JPEG. Used by other format modules
/// (e.g. TIFF) that need to convert to JPEG without round-tripping through a
/// JPEG decoder.
pub fn encode_rgb_pixels(
    pixels: &[u8],
    width: usize,
    height: usize,
    quality: u8,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
    comp.set_size(width, height);
    comp.set_quality(quality as f32);
    // Progressive + optimized huffman = smaller JPEG at the cost of a tad more CPU.
    comp.set_progressive_mode();
    comp.set_optimize_coding(true);

    let mut started = comp.start_compress(Vec::new()).map_err(|e| SquishError::EncodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;
    started.write_scanlines(pixels).map_err(|e| SquishError::EncodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;
    let out = started.finish().map_err(|e| SquishError::EncodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;
    Ok(out)
}
