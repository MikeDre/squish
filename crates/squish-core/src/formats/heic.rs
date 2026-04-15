use crate::error::SquishError;
use crate::options::SquishOptions;
use libheif_rs::{ColorSpace, CompressionFormat, EncoderQuality, HeifContext, LibHeif, RgbChroma};
use std::path::Path;

pub fn compress(
    input: &[u8],
    opts: &SquishOptions,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    let lib = LibHeif::new();

    let ctx = HeifContext::read_from_bytes(input).map_err(|e| SquishError::DecodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;
    let handle = ctx.primary_image_handle().map_err(|e| SquishError::DecodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;
    let image = lib
        .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)
        .map_err(|e| SquishError::DecodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;

    let mut out_ctx = HeifContext::new().map_err(|e| SquishError::EncodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;

    let mut encoder = lib
        .encoder_for_format(CompressionFormat::Hevc)
        .map_err(|e| SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;

    let q = opts.effective_quality(crate::format::Format::Heic);
    if opts.lossless {
        encoder
            .set_quality(EncoderQuality::LossLess)
            .map_err(|e| SquishError::EncodeFailed { path: path.to_path_buf(), source: Box::new(e) })?;
    } else {
        encoder
            .set_quality(EncoderQuality::Lossy(q))
            .map_err(|e| SquishError::EncodeFailed { path: path.to_path_buf(), source: Box::new(e) })?;
    }

    out_ctx
        .encode_image(&image, &mut encoder, None)
        .map_err(|e| SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;

    out_ctx.write_to_bytes().map_err(|e| SquishError::EncodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })
}
