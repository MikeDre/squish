//! Browser-renderable previews for the interactive crop selector.
//!
//! The selector needs to show any image squish can crop — including HEIC,
//! AVIF and TIFF, which browsers cannot display — so the decode has to happen
//! here, where the codecs live. The preview is deliberately capped: a 100 MP
//! scan would otherwise ship 400 MB of decoded pixels into a browser tab.

use crate::error::SquishError;
use crate::format::detect_format;
use std::io::Cursor;
use std::path::Path;

/// A downscaled, browser-renderable copy of an image, plus the dimensions of
/// the source it was made from. Selection maths runs in *source* pixels, so
/// callers need both.
#[derive(Debug, Clone)]
pub struct Preview {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub w: u32,
    pub h: u32,
    pub source_w: u32,
    pub source_h: u32,
}

/// Decode `path`, downscale to fit `max_edge` (never upscaling), and encode a
/// browser-renderable image: JPEG q85, or PNG when the source has alpha.
///
/// `opts` is needed because vector input has no pixels of its own — an SVG is
/// rendered at the size `opts.width`/`opts.height` implies, and that render
/// size is what `source_w`/`source_h` report, because the selector's maths
/// runs in source pixels.
pub fn preview_bytes(
    path: &Path,
    max_edge: u32,
    opts: &crate::options::SquishOptions,
) -> Result<Preview, SquishError> {
    let input = std::fs::read(path)?;
    let format_in = detect_format(path, &input).ok_or_else(|| SquishError::UnsupportedFormat {
        path: path.to_path_buf(),
        reason: "could not identify format from extension or magic bytes".into(),
    })?;

    // Font warnings are the run's business, not the preview's — the conversion
    // itself reports them.
    let (img, _warnings) = crate::decode_to_dynamic_image(format_in, &input, opts, path)?;
    let (source_w, source_h) = (img.width(), img.height());

    let scaled = if source_w > max_edge || source_h > max_edge {
        img.resize(max_edge, max_edge, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let (w, h) = (scaled.width(), scaled.height());

    let mut bytes = Vec::new();
    let mime = if scaled.color().has_alpha() {
        scaled
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .map_err(|e| SquishError::EncodeFailed {
                path: path.to_path_buf(),
                source: Box::new(e),
            })?;
        "image/png"
    } else {
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 85);
        enc.encode_image(&scaled.to_rgb8())
            .map_err(|e| SquishError::EncodeFailed {
                path: path.to_path_buf(),
                source: Box::new(e),
            })?;
        "image/jpeg"
    };

    Ok(Preview {
        bytes,
        mime,
        w,
        h,
        source_w,
        source_h,
    })
}
