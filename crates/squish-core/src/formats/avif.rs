use crate::error::SquishError;
use crate::options::SquishOptions;
use image::GenericImageView;
use ravif::{Encoder, Img, RGBA8};
use std::path::Path;

pub fn compress(
    input: &[u8],
    opts: &SquishOptions,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    let img = image::load_from_memory(input).map_err(|e| SquishError::DecodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;

    let (w, h) = img.dimensions();
    let rgba = img.to_rgba8();

    let pixels: Vec<RGBA8> = rgba
        .pixels()
        .map(|p| RGBA8 { r: p[0], g: p[1], b: p[2], a: p[3] })
        .collect();

    let quality = opts.effective_quality(crate::format::Format::Avif) as f32;
    let encoder = Encoder::new()
        .with_quality(quality)
        .with_speed(6);

    let result = encoder
        .encode_rgba(Img::new(&pixels, w as usize, h as usize))
        .map_err(|e| SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;

    Ok(result.avif_file)
}
