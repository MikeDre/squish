use crate::error::SquishError;
use crate::options::SquishOptions;
use image::metadata::Orientation;
use image::{DynamicImage, GenericImageView};
use mozjpeg::compress::CompressStarted;
use mozjpeg::decompress::MarkerIter;
use mozjpeg::Marker;
use std::path::Path;

/// JPEG APP1 markers carrying EXIF are prefixed with this literal before the
/// TIFF-structured payload that `image::metadata::Orientation` parses.
const EXIF_PREFIX: &[u8] = b"Exif\0\0";
/// JPEG APP2 markers carrying an ICC profile are prefixed with a 12-byte
/// signature, then a 1-byte chunk index and 1-byte chunk count (both
/// 0-indexed by `mozjpeg::CompressStarted::write_icc_profile`), before the
/// profile bytes for that chunk.
const ICC_SIGNATURE: &[u8] = b"ICC_PROFILE\0";

/// EXIF/ICC captured from a source JPEG's markers, plus the orientation
/// already parsed out of the EXIF (if any).
struct SourceMetadata {
    /// Raw TIFF-structured EXIF payload (prefix already stripped), if present.
    exif: Option<Vec<u8>>,
    /// Reassembled ICC profile bytes, if present.
    icc: Option<Vec<u8>>,
    orientation: Orientation,
}

fn read_markers(markers: MarkerIter<'_>) -> SourceMetadata {
    let mut exif = None;
    let mut icc_chunks: Vec<(u8, &[u8])> = Vec::new();

    for m in markers {
        match m.marker {
            Marker::APP(1) if m.data.starts_with(EXIF_PREFIX) && exif.is_none() => {
                exif = Some(m.data[EXIF_PREFIX.len()..].to_vec());
            }
            Marker::APP(2) if m.data.starts_with(ICC_SIGNATURE) => {
                let rest = &m.data[ICC_SIGNATURE.len()..];
                if let Some((&index, chunk)) = rest.split_first() {
                    icc_chunks.push((index, &chunk[1..])); // skip the count byte too
                }
            }
            _ => {}
        }
    }

    let icc = if icc_chunks.is_empty() {
        None
    } else {
        icc_chunks.sort_by_key(|(index, _)| *index);
        Some(
            icc_chunks
                .into_iter()
                .flat_map(|(_, c)| c.to_vec())
                .collect(),
        )
    };

    let orientation = exif
        .as_deref()
        .and_then(Orientation::from_exif_chunk)
        .unwrap_or(Orientation::NoTransforms);

    SourceMetadata {
        exif,
        icc,
        orientation,
    }
}

/// Compress a JPEG. Uses mozjpeg — its default settings are already a 15-25%
/// improvement over libjpeg-turbo at the same visual quality.
///
/// EXIF orientation is always applied to the pixels before re-encoding (a
/// rotated/flipped source must never come out visually wrong just because
/// nothing preserved its orientation tag). The ICC colour profile, if
/// present, is always carried over. EXIF itself is stripped by default —
/// `opts.keep_metadata` preserves it, with the orientation tag zeroed out
/// since the pixels are already corrected.
pub fn compress(input: &[u8], opts: &SquishOptions, path: &Path) -> Result<Vec<u8>, SquishError> {
    let quality = opts.effective_quality(crate::format::Format::Jpeg);

    let decomp = mozjpeg::Decompress::with_markers(&[Marker::APP(1), Marker::APP(2)])
        .from_mem(input)
        .map_err(|e| SquishError::DecodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
    let meta = read_markers(decomp.markers());

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

    let (width, height, pixels) = if meta.orientation == Orientation::NoTransforms {
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
        img.apply_orientation(meta.orientation);
        let (w, h) = img.dimensions();
        (w as usize, h as usize, img.into_rgb8().into_raw())
    };

    let exif_to_write = if opts.keep_metadata {
        meta.exif.map(|mut tiff| {
            // Pixels are already correctly oriented; leaving a stale
            // orientation tag would make a compliant viewer rotate again.
            let _ = Orientation::remove_from_exif_chunk(&mut tiff);
            tiff
        })
    } else {
        None
    };

    encode_rgb_pixels_with_metadata(
        &pixels,
        width,
        height,
        quality,
        meta.icc.as_deref(),
        exif_to_write.as_deref(),
        path,
    )
}

/// Encode an already-decoded raster as JPEG. Used for cross-format conversions,
/// which have no JPEG-shaped EXIF/ICC of their own to carry over.
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
    encode_rgb_pixels_with_metadata(pixels, width, height, quality, None, None, path)
}

/// Writes an ICC profile as one or more APP2 markers, chunked and prefixed
/// per the de facto standard ("ICC_PROFILE\0" + 1-indexed sequence number +
/// chunk count). Deliberately not `mozjpeg::CompressStarted::write_icc_profile`
/// — that method numbers chunks from 0, which `zune-jpeg` (and so the `image`
/// crate, and so anything reading our output through it) treats a `seq_no`
/// of 0 as invalid and silently drops the whole profile. Confirmed by
/// writing a real profile through both paths and reading it back.
fn write_icc_profile<W: std::io::Write>(started: &mut CompressStarted<W>, data: &[u8]) {
    const OVERHEAD_LEN: usize = ICC_SIGNATURE.len() + 2;
    const MAX_BYTES_IN_MARKER: usize = 65533;
    const MAX_DATA_BYTES_IN_MARKER: usize = MAX_BYTES_IN_MARKER - OVERHEAD_LEN;

    let chunks: Vec<&[u8]> = data.chunks(MAX_DATA_BYTES_IN_MARKER).collect();
    let num_chunks = chunks.len() as u8;
    for (i, chunk) in chunks.into_iter().enumerate() {
        let mut buf = Vec::with_capacity(OVERHEAD_LEN + chunk.len());
        buf.extend_from_slice(ICC_SIGNATURE);
        buf.push(i as u8 + 1); // 1-indexed
        buf.push(num_chunks);
        buf.extend_from_slice(chunk);
        started.write_marker(Marker::APP(2), &buf);
    }
}

#[allow(clippy::too_many_arguments)]
fn encode_rgb_pixels_with_metadata(
    pixels: &[u8],
    width: usize,
    height: usize,
    quality: u8,
    icc: Option<&[u8]>,
    exif: Option<&[u8]>,
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

    if let Some(icc) = icc {
        write_icc_profile(&mut started, icc);
    }
    if let Some(exif) = exif {
        let mut marker = Vec::with_capacity(EXIF_PREFIX.len() + exif.len());
        marker.extend_from_slice(EXIF_PREFIX);
        marker.extend_from_slice(exif);
        started.write_marker(Marker::APP(1), &marker);
    }

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
