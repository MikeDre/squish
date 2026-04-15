use crate::error::SquishError;
use crate::options::SquishOptions;
use std::path::Path;

/// TIFF output. Rarely useful but supported for `--format tiff` override.
pub fn compress(
    input: &[u8],
    _opts: &SquishOptions,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    let img = image::load_from_memory_with_format(input, image::ImageFormat::Tiff)
        .map_err(|e| SquishError::DecodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Tiff)
        .map_err(|e| SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
    Ok(out)
}

/// Decode TIFF to raw RGB and hand directly to the JPEG encoder.
/// Used when TIFF is the input format and output_format is unspecified (default).
pub fn compress_as_jpeg(
    input: &[u8],
    opts: &SquishOptions,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    let img = image::load_from_memory_with_format(input, image::ImageFormat::Tiff)
        .map_err(|e| SquishError::DecodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;

    let rgb = img.to_rgb8();
    let width = rgb.width() as usize;
    let height = rgb.height() as usize;
    let pixels = rgb.into_raw();

    let quality = opts.effective_quality(crate::format::Format::Jpeg);
    crate::formats::jpeg::encode_rgb_pixels(&pixels, width, height, quality, path)
}
