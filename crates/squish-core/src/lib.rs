//! Core image compression library for squish.

mod auto_quality;
pub mod crop;
pub mod error;
pub mod format;
pub mod formats;
pub mod naming;
pub mod options;
pub mod result;

pub use crop::{CropRect, CropSpec, Gravity};
pub use error::SquishError;
pub use format::{detect_format, Format};
pub use naming::{
    derive_output_path, derive_output_path_with_suffix, derive_output_path_with_suffix_sep,
    in_place_target, in_place_temp_path,
};
pub use options::SquishOptions;
pub use result::SquishResult;

use auto_quality::{ssimulacra2_score, VISUALLY_LOSSLESS_THRESHOLD};
use image::DynamicImage;
use std::fs;
use std::path::Path;
use std::time::Instant;

/// Compress a single file. Reads `input`, dispatches by format, writes output
/// path (derived from `naming::derive_output_path`), returns a `SquishResult`.
///
/// On error, no output file is written.
pub fn squish_file(input: &Path, opts: &SquishOptions) -> Result<SquishResult, SquishError> {
    let start = Instant::now();
    let input_bytes_vec = fs::read(input)?;

    let format_in =
        detect_format(input, &input_bytes_vec).ok_or_else(|| SquishError::UnsupportedFormat {
            path: input.to_path_buf(),
            reason: "could not identify format from extension or magic bytes".into(),
        })?;

    // TIFF default-output rule: when input is TIFF and user didn't specify a
    // target format, convert to JPEG.
    let format_out = match (format_in, opts.output_format) {
        (Format::Tiff, None) => Format::Jpeg,
        (_, Some(f)) => f,
        (f, None) => f,
    };

    let is_animated_webp =
        format_in == Format::Webp && formats::webp::is_animated_webp(&input_bytes_vec);
    let (output_bytes, warnings) = if let Some(target) = opts.target_size {
        compress_to_target(
            format_in,
            format_out,
            &input_bytes_vec,
            opts,
            input,
            is_animated_webp,
            target,
        )?
    } else if opts.auto && auto_quality_applies(format_out) && !opts.lossless && !is_animated_webp {
        compress_to_visually_lossless(
            format_in,
            format_out,
            &input_bytes_vec,
            opts,
            input,
            is_animated_webp,
        )?
    } else {
        encode_once(
            format_in,
            format_out,
            &input_bytes_vec,
            opts,
            input,
            is_animated_webp,
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

    let output_path = if opts.overwrite {
        match in_place_target(input, &target_ext) {
            Some(target) => target,
            None => {
                return Err(SquishError::InPlaceFormatChange {
                    path: input.to_path_buf(),
                    from: input
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase(),
                    to: target_ext.clone(),
                });
            }
        }
    } else {
        let suffix = opts.suffix.as_deref().unwrap_or("squished");
        derive_output_path_with_suffix(input, &target_ext, opts.force_overwrite, suffix)
    };
    fs::write(&output_path, &output_bytes)?;

    Ok(SquishResult {
        input_path: input.to_path_buf(),
        output_path,
        input_bytes: input_bytes_vec.len() as u64,
        output_bytes: output_bytes.len() as u64,
        format_in,
        format_out,
        duration: start.elapsed(),
        warnings,
    })
}

/// One full encode pass at the quality carried in `opts`.
///
/// If resize is requested and format supports it, decode → resize → encode.
/// SVG is skipped (vector). Animated WebP is also skipped — the webp codec
/// cannot resize animations; webp::compress emits a warning and passes through.
/// For same-format paths that normally skip decode, resize forces the
/// decode → resize → encode path.
fn encode_once(
    format_in: Format,
    format_out: Format,
    input_bytes: &[u8],
    opts: &SquishOptions,
    path: &Path,
    is_animated_webp: bool,
) -> Result<(Vec<u8>, Vec<String>), SquishError> {
    if opts.needs_resize() && format_in != Format::Svg && !is_animated_webp {
        let mut img = decode_to_dynamic_image(format_in, input_bytes, path)?;
        if let Some((new_w, new_h)) = opts.resize_dimensions(img.width(), img.height()) {
            img = img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);
        }
        let bytes = dispatch_encode_raster(format_out, &img, opts, path)?;
        Ok((bytes, Vec::new()))
    } else {
        dispatch_compress_with_conversion(format_in, format_out, input_bytes, opts, path)
    }
}

/// Whether an output format has a quality dial the target-size search can turn.
/// SVG is vector-lossless; TIFF output re-encodes losslessly.
fn has_quality_dial(format_out: Format) -> bool {
    !matches!(format_out, Format::Svg | Format::Tiff)
}

/// Whether `--quality auto` (perceptual search) applies to this output format.
/// Lossy raster only — JPEG, WebP, AVIF. Everything else falls back to a
/// normal encode.
fn auto_quality_applies(format_out: Format) -> bool {
    matches!(format_out, Format::Jpeg | Format::Webp | Format::Avif)
}

/// Find the LOWEST quality whose decoded output is still visually lossless
/// (SSIMULACRA2 ≥ threshold) versus the source, via binary search over the
/// 1..=100 range (~7 encode+decode+score passes). The inverse of
/// `compress_to_target`, which keeps the highest quality under a byte budget.
///
/// Falls back to a single default-quality `encode_once` when the metric can't
/// be computed (e.g. an image under 8px). Callers must only invoke this for
/// lossy raster formats — JPEG/WebP/AVIF (guarded in `squish_file`).
fn compress_to_visually_lossless(
    format_in: Format,
    format_out: Format,
    input_bytes: &[u8],
    opts: &SquishOptions,
    path: &Path,
    is_animated_webp: bool,
) -> Result<(Vec<u8>, Vec<String>), SquishError> {
    let src_rgb = match decode_to_dynamic_image(format_in, input_bytes, path) {
        Ok(img) => img.to_rgb8(),
        Err(_) => {
            return encode_once(
                format_in,
                format_out,
                input_bytes,
                opts,
                path,
                is_animated_webp,
            )
        }
    };

    let mut lo: u8 = 1;
    let mut hi: u8 = 100;
    let mut best_passing: Option<Vec<u8>> = None;

    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let attempt_opts = SquishOptions {
            quality: Some(mid),
            auto: false,
            target_size: None,
            ..opts.clone()
        };
        let (bytes, _warnings) = encode_once(
            format_in,
            format_out,
            input_bytes,
            &attempt_opts,
            path,
            is_animated_webp,
        )?;

        let cand_rgb = match image::load_from_memory(&bytes) {
            Ok(img) => img.to_rgb8(),
            Err(_) => {
                return encode_once(
                    format_in,
                    format_out,
                    input_bytes,
                    opts,
                    path,
                    is_animated_webp,
                )
            }
        };
        let (cw, ch) = cand_rgb.dimensions();
        let resized;
        let src_slice: &[u8] = if src_rgb.dimensions() == (cw, ch) {
            src_rgb.as_raw()
        } else {
            resized =
                image::imageops::resize(&src_rgb, cw, ch, image::imageops::FilterType::Lanczos3);
            resized.as_raw()
        };

        match ssimulacra2_score(src_slice, cand_rgb.as_raw(), cw as usize, ch as usize) {
            None => {
                return encode_once(
                    format_in,
                    format_out,
                    input_bytes,
                    opts,
                    path,
                    is_animated_webp,
                )
            }
            Some(score) if score >= VISUALLY_LOSSLESS_THRESHOLD => {
                best_passing = Some(bytes);
                if mid == 1 {
                    break;
                }
                hi = mid - 1;
            }
            Some(_) => {
                lo = mid + 1;
            }
        }
    }

    match best_passing {
        Some(bytes) => Ok((bytes, Vec::new())),
        None => {
            let q100 = SquishOptions {
                quality: Some(100),
                auto: false,
                target_size: None,
                ..opts.clone()
            };
            let (bytes, mut warnings) = encode_once(
                format_in,
                format_out,
                input_bytes,
                &q100,
                path,
                is_animated_webp,
            )?;
            warnings.push(format!(
                "{}: --quality auto could not reach visually-lossless quality; wrote maximum quality",
                path.display()
            ));
            Ok((bytes, warnings))
        }
    }
}

/// Find the highest quality whose output fits `target` bytes, via binary
/// search over the 1..=100 quality range (~7 encode passes). For formats
/// without a quality dial, encodes once and warns if the budget is missed.
/// An unreachable budget yields the smallest attempt plus a warning.
fn compress_to_target(
    format_in: Format,
    format_out: Format,
    input_bytes: &[u8],
    opts: &SquishOptions,
    path: &Path,
    is_animated_webp: bool,
    target: u64,
) -> Result<(Vec<u8>, Vec<String>), SquishError> {
    let over_budget_warning = |size: u64, detail: &str| {
        format!(
            "{}: could not reach target size {target} bytes ({detail}); output is {size} bytes",
            path.display()
        )
    };

    if !has_quality_dial(format_out) || opts.lossless || is_animated_webp {
        let (bytes, mut warnings) = encode_once(
            format_in,
            format_out,
            input_bytes,
            opts,
            path,
            is_animated_webp,
        )?;
        if bytes.len() as u64 > target {
            let detail = format!(
                "{} output has no quality dial to adjust",
                format_out.extension()
            );
            warnings.push(over_budget_warning(bytes.len() as u64, &detail));
        }
        return Ok((bytes, warnings));
    }

    let mut lo: u8 = 1;
    let mut hi: u8 = 100;
    let mut best: Option<(Vec<u8>, Vec<String>)> = None;
    let mut smallest: Option<(Vec<u8>, Vec<String>)> = None;

    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let attempt_opts = SquishOptions {
            quality: Some(mid),
            target_size: None,
            ..opts.clone()
        };
        let (bytes, warnings) = encode_once(
            format_in,
            format_out,
            input_bytes,
            &attempt_opts,
            path,
            is_animated_webp,
        )?;
        if smallest
            .as_ref()
            .map(|(b, _)| bytes.len() < b.len())
            .unwrap_or(true)
        {
            smallest = Some((bytes.clone(), warnings.clone()));
        }
        if bytes.len() as u64 <= target {
            best = Some((bytes, warnings));
            lo = mid + 1;
        } else {
            if mid == 1 {
                break;
            }
            hi = mid - 1;
        }
    }

    match best {
        Some(found) => Ok(found),
        None => {
            let (bytes, mut warnings) =
                smallest.expect("search always evaluates at least one quality");
            warnings.push(over_budget_warning(
                bytes.len() as u64,
                "even at minimum quality",
            ));
            Ok((bytes, warnings))
        }
    }
}

fn dispatch_compress_with_conversion(
    format_in: Format,
    format_out: Format,
    input: &[u8],
    opts: &SquishOptions,
    path: &Path,
) -> Result<(Vec<u8>, Vec<String>), SquishError> {
    // Same-format fast path: route to the native single-format compressor, which
    // can use format-specific decoders (e.g. mozjpeg) and preserve features like
    // animated-GIF frames.
    if format_in == format_out {
        return dispatch_same_format(format_out, input, opts, path);
    }

    // TIFF → JPEG is the documented default when TIFF is input without override.
    // Keep the existing direct path so we don't double-decode.
    if format_in == Format::Tiff && format_out == Format::Jpeg {
        return formats::tiff::compress_as_jpeg(input, opts, path).map(|b| (b, Vec::new()));
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
    dispatch_encode_raster(format_out, &img, opts, path).map(|b| (b, Vec::new()))
}

fn dispatch_same_format(
    format: Format,
    input: &[u8],
    opts: &SquishOptions,
    path: &Path,
) -> Result<(Vec<u8>, Vec<String>), SquishError> {
    match format {
        Format::Png => formats::png::compress(input, opts, path).map(|b| (b, Vec::new())),
        Format::Jpeg => formats::jpeg::compress(input, opts, path).map(|b| (b, Vec::new())),
        Format::Webp => formats::webp::compress(input, opts, path),
        Format::Avif => formats::avif::compress(input, opts, path).map(|b| (b, Vec::new())),
        Format::Svg => formats::svg::compress(input, opts, path).map(|b| (b, Vec::new())),
        Format::Gif => formats::gif::compress(input, opts, path).map(|b| (b, Vec::new())),
        Format::Heic => formats::heic::compress(input, opts, path).map(|b| (b, Vec::new())),
        Format::Tiff => formats::tiff::compress(input, opts, path).map(|b| (b, Vec::new())),
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

fn decode_heic_to_dynamic_image(input: &[u8], path: &Path) -> Result<DynamicImage, SquishError> {
    use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};

    let lib = LibHeif::new();
    let ctx = HeifContext::read_from_bytes(input).map_err(|e| SquishError::DecodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;
    let handle = ctx
        .primary_image_handle()
        .map_err(|e| SquishError::DecodeFailed {
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
    let plane = planes
        .interleaved
        .ok_or_else(|| SquishError::DecodeFailed {
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
        let err = squish_file(
            Path::new("/nonexistent/path/xyz.png"),
            &SquishOptions::default(),
        )
        .unwrap_err();
        assert!(matches!(err, SquishError::Io(_)));
    }

    #[test]
    fn overwrite_replaces_png_in_place() {
        let tmp = tempfile::TempDir::new().unwrap();
        let input = tmp.path().join("dot.png");
        let img = image::RgbImage::from_pixel(8, 8, image::Rgb([255, 255, 255]));
        img.save(&input).unwrap();

        let opts = SquishOptions {
            overwrite: true,
            ..Default::default()
        };
        let r = squish_file(&input, &opts).unwrap();

        assert_eq!(r.output_path, input, "output must be the input path itself");
        assert!(input.exists());
        assert!(!tmp.path().join("dot_squished.png").exists());
    }

    #[test]
    fn overwrite_refuses_on_format_change() {
        let tmp = tempfile::TempDir::new().unwrap();
        let input = tmp.path().join("dot.png");
        let img = image::RgbImage::from_pixel(8, 8, image::Rgb([255, 255, 255]));
        img.save(&input).unwrap();

        let opts = SquishOptions {
            overwrite: true,
            output_format: Some(Format::Webp),
            ..Default::default()
        };
        let err = squish_file(&input, &opts).unwrap_err();
        assert!(matches!(err, SquishError::InPlaceFormatChange { .. }));
        assert!(input.exists());
        assert!(!tmp.path().join("dot.webp").exists());
    }
}

#[cfg(test)]
mod auto_quality_search_tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("tests/fixtures");
        p.push(name);
        p
    }

    #[test]
    fn auto_picks_a_quality_that_is_visually_lossless_and_smaller_than_max() {
        let bytes = std::fs::read(fixture("sample.jpg")).unwrap();

        let auto_opts = SquishOptions {
            auto: true,
            ..Default::default()
        };
        let (auto_bytes, _) = compress_to_visually_lossless(
            Format::Jpeg,
            Format::Jpeg,
            &bytes,
            &auto_opts,
            &fixture("sample.jpg"),
            false,
        )
        .unwrap();

        let src = decode_to_dynamic_image(Format::Jpeg, &bytes, &fixture("sample.jpg"))
            .unwrap()
            .to_rgb8();
        let cand = image::load_from_memory(&auto_bytes).unwrap().to_rgb8();
        assert_eq!(src.dimensions(), cand.dimensions());
        let score = crate::auto_quality::ssimulacra2_score(
            src.as_raw(),
            cand.as_raw(),
            src.width() as usize,
            src.height() as usize,
        )
        .unwrap();
        assert!(
            score >= crate::auto_quality::VISUALLY_LOSSLESS_THRESHOLD,
            "auto output should be visually lossless, scored {score}"
        );

        let q100 = SquishOptions {
            quality: Some(100),
            ..Default::default()
        };
        let (q100_bytes, _) = encode_once(
            Format::Jpeg,
            Format::Jpeg,
            &bytes,
            &q100,
            &fixture("sample.jpg"),
            false,
        )
        .unwrap();
        assert!(
            auto_bytes.len() < q100_bytes.len(),
            "auto ({}) should be smaller than q100 ({})",
            auto_bytes.len(),
            q100_bytes.len()
        );
    }

    #[test]
    fn auto_on_png_falls_back_to_normal_encode() {
        // PNG is not a lossy-dial format, so `auto` must behave exactly like a
        // normal default encode (no perceptual search, no error).
        let bytes = std::fs::read(fixture("sample.png")).unwrap();

        let auto_opts = SquishOptions {
            auto: true,
            ..Default::default()
        };
        let default_opts = SquishOptions::default();

        let (auto_out, _) = encode_once(
            Format::Png,
            Format::Png,
            &bytes,
            &auto_opts,
            &fixture("sample.png"),
            false,
        )
        .unwrap();
        let (default_out, _) = encode_once(
            Format::Png,
            Format::Png,
            &bytes,
            &default_opts,
            &fixture("sample.png"),
            false,
        )
        .unwrap();

        // encode_once ignores `auto` entirely (the search lives in squish_file's
        // dispatch, which only routes JPEG/WebP/AVIF to it), so the two outputs
        // are byte-identical.
        assert_eq!(auto_out, default_out);
    }
}
