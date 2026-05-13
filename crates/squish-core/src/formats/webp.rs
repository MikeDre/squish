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

/// Detect whether `bytes` is an animated WebP via the VP8X chunk's animation flag.
///
/// WebP structure: RIFF(4) + size(4) + WEBP(4) + chunk_fourcc(4) + chunk_size(4) + chunk_data.
/// For animated files the first chunk fourcc is `VP8X` and the flags byte at offset 20
/// (bit 1, mask 0x02) is set. Static files use `VP8 ` (lossy) or `VP8L` (lossless).
pub fn is_animated_webp(bytes: &[u8]) -> bool {
    if bytes.len() < 30 {
        return false;
    }
    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return false;
    }
    if &bytes[12..16] != b"VP8X" {
        return false;
    }
    (bytes[20] & 0x02) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_webp_header(first_chunk: &[u8; 4], vp8x_flags: u8) -> Vec<u8> {
        // RIFF + size(4) + WEBP + chunk(4) + chunk_size(4) + 10-byte body
        let mut buf = Vec::with_capacity(30);
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&[0, 0, 0, 0]); // size placeholder
        buf.extend_from_slice(b"WEBP");
        buf.extend_from_slice(first_chunk);
        buf.extend_from_slice(&[10, 0, 0, 0]); // chunk size
        // VP8X body: flags(1) + reserved(3) + width(3) + height(3) = 10 bytes
        buf.push(vp8x_flags);
        buf.extend_from_slice(&[0; 9]);
        buf
    }

    #[test]
    fn is_animated_webp_true_for_animated_flag() {
        let buf = make_webp_header(b"VP8X", 0x02);
        assert!(is_animated_webp(&buf));
    }

    #[test]
    fn is_animated_webp_false_for_static_vp8() {
        let buf = make_webp_header(b"VP8 ", 0x00);
        assert!(!is_animated_webp(&buf));
    }

    #[test]
    fn is_animated_webp_false_for_static_vp8l() {
        let buf = make_webp_header(b"VP8L", 0x00);
        assert!(!is_animated_webp(&buf));
    }

    #[test]
    fn is_animated_webp_false_for_vp8x_without_anim_bit() {
        // VP8X header but animation bit is clear (e.g., extended with only alpha/EXIF)
        let buf = make_webp_header(b"VP8X", 0x10); // alpha bit only
        assert!(!is_animated_webp(&buf));
    }

    #[test]
    fn is_animated_webp_false_for_truncated() {
        assert!(!is_animated_webp(&[0xff; 10]));
        assert!(!is_animated_webp(&[]));
    }

    #[test]
    fn is_animated_webp_false_for_non_webp() {
        // PNG magic (89 50 4E 47 0D 0A 1A 0A)
        let png = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0,
                   0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(!is_animated_webp(&png));
    }
}
