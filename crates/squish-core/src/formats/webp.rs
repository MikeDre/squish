use crate::error::SquishError;
use crate::options::SquishOptions;
use image::{DynamicImage, GenericImageView};
use std::path::Path;

/// Compress a WebP file. Animated WebPs are passed through unchanged
/// (the current encoder doesn't support animation). Static WebPs are
/// decoded and re-encoded with `encode_raster`.
///
/// Returns `(bytes, warnings)`. Warnings surface when the caller passed
/// flags we can't honor on an animated input (currently `--max-width` /
/// `--max-height`).
pub fn compress(
    input: &[u8],
    opts: &SquishOptions,
    path: &Path,
) -> Result<(Vec<u8>, Vec<String>), SquishError> {
    if is_animated_webp(input) {
        let mut warnings = Vec::new();
        if opts.max_width.is_some() || opts.max_height.is_some() {
            warnings.push(format!(
                "{}: animated WebP cannot be resized; passing through unchanged",
                path.display()
            ));
        }
        return Ok((input.to_vec(), warnings));
    }

    // Decode whatever raster the caller gave us (could be WebP, PNG, JPEG, etc.
    // since this may be reached via --format conversion).
    let img = image::load_from_memory(input).map_err(|e| SquishError::DecodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;

    encode_raster(&img, opts, path).map(|b| (b, Vec::new()))
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

    use crate::options::SquishOptions;
    use std::path::PathBuf;

    fn anim_fixture() -> Vec<u8> {
        std::fs::read("tests/fixtures/anim.webp").expect("fixture missing")
    }

    #[test]
    fn compress_passes_animated_through_unchanged() {
        let input = anim_fixture();
        let (bytes, warnings) =
            compress(&input, &SquishOptions::default(), &PathBuf::from("anim.webp")).unwrap();
        assert_eq!(bytes, input, "animated WebP should pass through byte-for-byte");
        assert!(warnings.is_empty(), "no flags conflict; no warnings expected");
    }

    #[test]
    fn compress_emits_warning_with_max_width_on_animated() {
        let input = anim_fixture();
        let opts = SquishOptions {
            max_width: Some(100),
            ..Default::default()
        };
        let (bytes, warnings) = compress(&input, &opts, &PathBuf::from("anim.webp")).unwrap();
        assert_eq!(bytes, input);
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].contains("cannot be resized"),
            "warning text: {}",
            warnings[0]
        );
    }

    #[test]
    fn compress_emits_warning_with_max_height_on_animated() {
        let input = anim_fixture();
        let opts = SquishOptions {
            max_height: Some(100),
            ..Default::default()
        };
        let (bytes, warnings) = compress(&input, &opts, &PathBuf::from("anim.webp")).unwrap();
        assert_eq!(bytes, input);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn compress_emits_single_warning_with_both_max_dims() {
        let input = anim_fixture();
        let opts = SquishOptions {
            max_width: Some(100),
            max_height: Some(100),
            ..Default::default()
        };
        let (bytes, warnings) = compress(&input, &opts, &PathBuf::from("anim.webp")).unwrap();
        assert_eq!(bytes, input);
        assert_eq!(warnings.len(), 1, "both flags should produce one warning, not two");
    }
}
