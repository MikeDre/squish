//! Perceptual quality measurement for `--quality auto`, isolating the
//! `ssimulacra2` dependency behind one function.

use ssimulacra2::{compute_frame_ssimulacra2, ColorPrimaries, Rgb, TransferCharacteristic};

/// SSIMULACRA2 score at or above which output is considered visually lossless.
/// The metric is calibrated so ~90 = visually lossless, ~70 = high quality.
pub(crate) const VISUALLY_LOSSLESS_THRESHOLD: f64 = 90.0;

/// Compute the SSIMULACRA2 score between two equal-dimension RGB8 buffers
/// (3 bytes/pixel, `width * height * 3` long). Higher is more similar
/// (~100 identical). Returns `None` when the metric cannot be computed —
/// dimensions under 8px, a mismatch, or a malformed buffer — so callers can
/// fall back to a normal encode instead of failing.
pub(crate) fn ssimulacra2_score(
    orig_rgb: &[u8],
    cand_rgb: &[u8],
    width: usize,
    height: usize,
) -> Option<f64> {
    let to_rgb = |px: &[u8]| -> Option<Rgb> {
        if px.len() != width * height * 3 {
            return None;
        }
        let data: Vec<[f32; 3]> = px
            .chunks_exact(3)
            .map(|c| {
                [
                    c[0] as f32 / 255.0,
                    c[1] as f32 / 255.0,
                    c[2] as f32 / 255.0,
                ]
            })
            .collect();
        Rgb::new(
            data,
            width,
            height,
            TransferCharacteristic::SRGB,
            ColorPrimaries::BT709,
        )
        .ok()
    };
    let src = to_rgb(orig_rgb)?;
    let dst = to_rgb(cand_rgb)?;
    compute_frame_ssimulacra2(src, dst).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: usize, h: usize) -> Vec<u8> {
        let mut v = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let r = (x * 255 / w.max(1)) as u8;
                let g = (y * 255 / h.max(1)) as u8;
                v.extend_from_slice(&[r, g, 128]);
            }
        }
        v
    }

    #[test]
    fn identical_images_score_near_100() {
        let img = gradient(16, 16);
        let s = ssimulacra2_score(&img, &img, 16, 16).unwrap();
        assert!(s > 95.0, "identical images should score near 100, got {s}");
    }

    #[test]
    fn degraded_image_scores_below_threshold() {
        let img = gradient(16, 16);
        let mut bad = img.clone();
        for (i, b) in bad.iter_mut().enumerate() {
            if i % 2 == 0 {
                *b = 0;
            }
        }
        let s = ssimulacra2_score(&img, &bad, 16, 16).unwrap();
        assert!(
            s < VISUALLY_LOSSLESS_THRESHOLD,
            "heavy degradation should score below {VISUALLY_LOSSLESS_THRESHOLD}, got {s}"
        );
    }

    #[test]
    fn under_8px_returns_none() {
        let img = gradient(4, 4);
        assert_eq!(ssimulacra2_score(&img, &img, 4, 4), None);
    }

    #[test]
    fn malformed_buffer_returns_none() {
        let img = gradient(16, 16);
        assert_eq!(ssimulacra2_score(&img, &img[..10], 16, 16), None);
    }
}
