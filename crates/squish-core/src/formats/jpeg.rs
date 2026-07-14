use crate::error::SquishError;
use crate::options::SquishOptions;
use image::metadata::Orientation;
use image::{DynamicImage, GenericImageView};
use mozjpeg::Marker;
use std::path::Path;

/// JPEG APP1 markers carrying EXIF are prefixed with this literal before the
/// TIFF-structured payload that `image::metadata::Orientation` parses.
const EXIF_PREFIX: &[u8] = b"Exif\0\0";

/// Compress a JPEG. Uses mozjpeg — its default settings are already a 15-25%
/// improvement over libjpeg-turbo at the same visual quality.
///
/// EXIF orientation is always applied to the pixels before re-encoding: a
/// rotated/flipped source (the overwhelming majority of real-world EXIF
/// orientation use is camera photos) must never come out visually wrong just
/// because nothing reads or preserves its orientation tag. EXIF/ICC
/// themselves are not yet preserved in the output at all — that's a
/// separate, following change.
pub fn compress(input: &[u8], opts: &SquishOptions, path: &Path) -> Result<Vec<u8>, SquishError> {
    let quality = opts.effective_quality(crate::format::Format::Jpeg);

    let decomp = mozjpeg::Decompress::with_markers(&[Marker::APP(1)])
        .from_mem(input)
        .map_err(|e| SquishError::DecodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
    let orientation = read_orientation(&decomp);

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

    let (width, height, pixels) = if orientation == Orientation::NoTransforms {
        (width, height, pixels)
    } else {
        let mut img = DynamicImage::ImageRgb8(
            image::RgbImage::from_raw(width as u32, height as u32, pixels).ok_or_else(|| {
                SquishError::DecodeFailed {
                    path: path.to_path_buf(),
                    source: "decoded JPEG pixel buffer size mismatch".into(),
                }
            })?,
        );
        img.apply_orientation(orientation);
        let (w, h) = img.dimensions();
        (w as usize, h as usize, img.into_rgb8().into_raw())
    };

    encode_rgb_pixels(&pixels, width, height, quality, path)
}

/// Reads the EXIF orientation tag from a `Decompress` constructed with
/// `with_markers(&[Marker::APP(1)])`. Defaults to `NoTransforms` if there's
/// no EXIF, or it doesn't parse.
fn read_orientation(decomp: &mozjpeg::Decompress<&[u8]>) -> Orientation {
    decomp
        .markers()
        .find(|m| m.marker == Marker::APP(1) && m.data.starts_with(EXIF_PREFIX))
        .and_then(|m| Orientation::from_exif_chunk(&m.data[EXIF_PREFIX.len()..]))
        .unwrap_or(Orientation::NoTransforms)
}

/// Encode an already-decoded raster as JPEG. Used for cross-format conversions.
pub fn encode_raster(
    img: &DynamicImage,
    opts: &SquishOptions,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    let (w, h) = img.dimensions();
    let rgb = img.to_rgb8().into_raw();
    let quality = opts.effective_quality(crate::format::Format::Jpeg);
    encode_rgb_pixels(&rgb, w as usize, h as usize, quality, path)
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

    let mut started = comp
        .start_compress(Vec::new())
        .map_err(|e| SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
    started
        .write_scanlines(pixels)
        .map_err(|e| SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
    let out = started.finish().map_err(|e| SquishError::EncodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;
    Ok(out)
}
