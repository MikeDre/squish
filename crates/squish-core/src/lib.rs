//! Core image compression library for squish.

pub mod error;
pub mod format;
pub mod formats;
pub mod naming;
pub mod options;
pub mod result;

pub use error::SquishError;
pub use format::{detect_format, Format};
pub use naming::derive_output_path;
pub use options::SquishOptions;
pub use result::SquishResult;

use image::DynamicImage;
use std::fs;
use std::path::Path;
use std::time::Instant;

/// Compress a single file. Reads `input`, dispatches by format, writes output
/// path (derived from `naming::derive_output_path`), returns a `SquishResult`.
///
/// On error, no output file is written.
pub fn squish_file(
    input: &Path,
    opts: &SquishOptions,
) -> Result<SquishResult, SquishError> {
    let start = Instant::now();
    let input_bytes_vec = fs::read(input)?;

    let format_in = detect_format(input, &input_bytes_vec).ok_or_else(|| {
        SquishError::UnsupportedFormat {
            path: input.to_path_buf(),
            reason: "could not identify format from extension or magic bytes".into(),
        }
    })?;

    // TIFF default-output rule: when input is TIFF and user didn't specify a
    // target format, convert to JPEG.
    let format_out = match (format_in, opts.output_format) {
        (Format::Tiff, None) => Format::Jpeg,
        (_, Some(f)) => f,
        (f, None) => f,
    };

    // If resize is requested and format supports it, decode → resize → encode.
    // SVG is skipped (vector). For same-format paths that normally skip decode,
    // resize forces the decode → resize → encode path.
    let output_bytes = if opts.needs_resize() && format_in != Format::Svg {
        let mut img = decode_to_dynamic_image(format_in, &input_bytes_vec, input)?;
        if let Some((new_w, new_h)) = opts.resize_dimensions(img.width(), img.height()) {
            img = img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);
        }
        dispatch_encode_raster(format_out, &img, opts, input)?
    } else {
        dispatch_compress_with_conversion(
            format_in, format_out, &input_bytes_vec, opts, input,
        )?
    };

    let target_ext = if format_in == format_out {
        input
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_else(|| format_out.extension().to_string())
    } else {
        format_out.extension().to_string()
    };

    let output_path = derive_output_path(input, &target_ext, opts.force_overwrite);
    fs::write(&output_path, &output_bytes)?;

    Ok(SquishResult {
        input_path: input.to_path_buf(),
        output_path,
        input_bytes: input_bytes_vec.len() as u64,
        output_bytes: output_bytes.len() as u64,
        format_in,
        format_out,
        duration: start.elapsed(),
    })
}

fn dispatch_compress_with_conversion(
    format_in: Format,
    format_out: Format,
    input: &[u8],
    opts: &SquishOptions,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    // Same-format fast path: route to the native single-format compressor, which
    // can use format-specific decoders (e.g. mozjpeg) and preserve features like
    // animated-GIF frames.
    if format_in == format_out {
        return dispatch_same_format(format_out, input, opts, path);
    }

    // TIFF → JPEG is the documented default when TIFF is input without override.
    // Keep the existing direct path so we don't double-decode.
    if format_in == Format::Tiff && format_out == Format::Jpeg {
        return formats::tiff::compress_as_jpeg(input, opts, path);
    }

    // SVG cannot be rasterized here (no renderer linked), and no raster source
    // can be vectorized. Reject cross-format conversions involving SVG early
    // with a clear message instead of letting the underlying decoder crash.
    if format_in == Format::Svg || format_out == Format::Svg {
        return Err(SquishError::UnsupportedFormat {
            path: path.to_path_buf(),
            reason: format!(
                "cannot convert {} to {}: SVG cross-format conversion is not supported",
                format_in.extension(),
                format_out.extension()
            ),
        });
    }

    // Generic cross-format path: decode source to a DynamicImage, then hand
    // off to the target encoder's raster entry point.
    let img = decode_to_dynamic_image(format_in, input, path)?;
    dispatch_encode_raster(format_out, &img, opts, path)
}

fn dispatch_same_format(
    format: Format,
    input: &[u8],
    opts: &SquishOptions,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    match format {
        Format::Png => formats::png::compress(input, opts, path),
        Format::Jpeg => formats::jpeg::compress(input, opts, path),
        Format::Webp => formats::webp::compress(input, opts, path),
        Format::Avif => formats::avif::compress(input, opts, path),
        Format::Svg => formats::svg::compress(input, opts, path),
        Format::Gif => formats::gif::compress(input, opts, path),
        Format::Heic => formats::heic::compress(input, opts, path),
        Format::Tiff => formats::tiff::compress(input, opts, path),
    }
}

fn dispatch_encode_raster(
    format_out: Format,
    img: &DynamicImage,
    opts: &SquishOptions,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    match format_out {
        Format::Png => formats::png::encode_raster(img, opts, path),
        Format::Jpeg => formats::jpeg::encode_raster(img, opts, path),
        Format::Webp => formats::webp::encode_raster(img, opts, path),
        Format::Avif => formats::avif::encode_raster(img, opts, path),
        Format::Tiff => formats::tiff::encode_raster(img, opts, path),
        Format::Gif => formats::gif::encode_raster(img, opts, path),
        Format::Heic => formats::heic::encode_raster(img, opts, path),
        Format::Svg => Err(SquishError::UnsupportedFormat {
            path: path.to_path_buf(),
            reason: "cannot convert raster input to SVG".into(),
        }),
    }
}

fn decode_to_dynamic_image(
    format_in: Format,
    input: &[u8],
    path: &Path,
) -> Result<DynamicImage, SquishError> {
    match format_in {
        // HEIC isn't handled by the `image` crate — use libheif and hand back
        // an RGBA8 DynamicImage.
        Format::Heic => decode_heic_to_dynamic_image(input, path),
        // SVG never reaches here (rejected earlier), but guard in case.
        Format::Svg => Err(SquishError::UnsupportedFormat {
            path: path.to_path_buf(),
            reason: "cannot rasterize SVG for cross-format conversion".into(),
        }),
        // Everything else is a raster format supported by `image`.
        _ => image::load_from_memory(input).map_err(|e| SquishError::DecodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        }),
    }
}

fn decode_heic_to_dynamic_image(
    input: &[u8],
    path: &Path,
) -> Result<DynamicImage, SquishError> {
    use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};

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

    let w = image.width();
    let h = image.height();
    let planes = image.planes();
    let plane = planes.interleaved.ok_or_else(|| SquishError::DecodeFailed {
        path: path.to_path_buf(),
        source: "HEIC decoder did not return an interleaved RGBA plane".into(),
    })?;

    let row_bytes = (w as usize) * 4;
    let mut rgba = Vec::with_capacity(row_bytes * h as usize);
    for y in 0..(h as usize) {
        let start = y * plane.stride;
        rgba.extend_from_slice(&plane.data[start..start + row_bytes]);
    }

    let buf = image::RgbaImage::from_raw(w, h, rgba).ok_or_else(|| SquishError::DecodeFailed {
        path: path.to_path_buf(),
        source: "failed to build RGBA buffer from HEIC planes".into(),
    })?;
    Ok(DynamicImage::ImageRgba8(buf))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn unknown_format_returns_unsupported() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("mystery.xyz");
        fs::write(&input, b"random bytes not matching any magic").unwrap();

        let err = squish_file(&input, &SquishOptions::default()).unwrap_err();
        match err {
            SquishError::UnsupportedFormat { reason, .. } => {
                assert!(reason.contains("could not identify format"));
            }
            other => panic!("wrong error variant: {other:?}"),
        }
    }

    #[test]
    fn missing_file_returns_io_error() {
        let err = squish_file(Path::new("/nonexistent/path/xyz.png"), &SquishOptions::default())
            .unwrap_err();
        assert!(matches!(err, SquishError::Io(_)));
    }
}
