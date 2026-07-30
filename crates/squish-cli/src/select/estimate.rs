//! Exact output size for a candidate crop.
//!
//! The number shown in the selector is produced by the real pipeline, not a
//! model: the crop is encoded exactly as the run would encode it. That is also
//! the simplest implementation, and the only one that covers GIF (whose crop
//! happens inside gifsicle, not in memory).

use anyhow::Result;
use squish_core::{CropRect, CropSpec, SquishOptions};
use std::path::{Path, PathBuf};

/// Selections bigger than this skip the live estimate: the encode would take
/// long enough to feel broken.
const MAX_ESTIMATE_PIXELS: u64 = 40_000_000;

#[derive(Debug)]
pub(crate) enum EstimateOutcome {
    Bytes(u64),
    Skipped(&'static str),
}

/// Scratch directory for one estimate. Named per session+request so concurrent
/// squish processes never collide.
fn work_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("squish-estimate-{tag}"))
}

/// Encode `rect` at the run's effective options and report the output size.
///
/// The input is copied first and the options are neutered (`overwrite: false`)
/// because `squish_file` honours `overwrite` — estimating with the run's own
/// options would otherwise replace the user's original on every adjustment.
pub(crate) fn estimate(
    source: &Path,
    opts: &SquishOptions,
    rect: CropRect,
    tag: &str,
) -> Result<EstimateOutcome> {
    if opts.auto {
        return Ok(EstimateOutcome::Skipped("--quality auto"));
    }
    if rect.w as u64 * rect.h as u64 > MAX_ESTIMATE_PIXELS {
        return Ok(EstimateOutcome::Skipped("selection too large"));
    }

    let dir = work_dir(tag);
    std::fs::create_dir_all(&dir)?;
    let name = source
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("input"));
    let copy = dir.join(name);
    std::fs::copy(source, &copy)?;

    let mut o = opts.clone();
    o.overwrite = false;
    o.force_overwrite = true;
    o.suffix = None;
    o.crop = Some(CropSpec::Exact {
        w: rect.w,
        h: rect.h,
        x: rect.x,
        y: rect.y,
    });

    let result = squish_core::squish_file(&copy, &o);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(EstimateOutcome::Bytes(result?.output_bytes))
}

/// Human label for the settings an estimate was produced with, e.g. "q75 webp".
pub(crate) fn settings_label(opts: &SquishOptions, source: &Path) -> String {
    let ext = opts
        .output_format
        .map(|f| f.extension().to_string())
        .unwrap_or_else(|| {
            source
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
        });
    if opts.lossless {
        return format!("lossless {ext}");
    }
    match opts.quality {
        Some(q) => format!("q{q} {ext}"),
        None => format!("default quality {ext}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};
    use squish_core::SquishOptions;

    fn png(dir: &std::path::Path, w: u32, h: u32) -> std::path::PathBuf {
        let p = dir.join("in.png");
        RgbImage::from_pixel(w, h, Rgb([90, 140, 210]))
            .save(&p)
            .unwrap();
        p
    }

    #[test]
    fn matches_a_real_squish_of_the_same_rect() {
        let dir = tempfile::TempDir::new().unwrap();
        let src = png(dir.path(), 400, 300);
        let rect = CropRect {
            x: 10,
            y: 10,
            w: 100,
            h: 80,
        };
        let opts = SquishOptions::default();

        let estimated = match estimate(&src, &opts, rect, "test-a").unwrap() {
            EstimateOutcome::Bytes(n) => n,
            other => panic!("expected Bytes, got {other:?}"),
        };

        // The real thing, for comparison.
        let mut real_opts = opts.clone();
        real_opts.crop = Some(CropSpec::Exact {
            w: 100,
            h: 80,
            x: 10,
            y: 10,
        });
        let real = squish_core::squish_file(&src, &real_opts).unwrap();

        assert_eq!(estimated, real.output_bytes);
    }

    #[test]
    fn leaves_the_input_untouched_even_with_overwrite() {
        let dir = tempfile::TempDir::new().unwrap();
        let src = png(dir.path(), 400, 300);
        let before = std::fs::read(&src).unwrap();

        // The footgun: with these options squish_file would replace `src`.
        let opts = SquishOptions {
            overwrite: true,
            ..Default::default()
        };

        for i in 0..3 {
            estimate(
                &src,
                &opts,
                CropRect {
                    x: 0,
                    y: 0,
                    w: 200,
                    h: 150,
                },
                &format!("test-b{i}"),
            )
            .unwrap();
        }

        assert_eq!(
            std::fs::read(&src).unwrap(),
            before,
            "input must be byte-identical"
        );
    }

    #[test]
    fn skips_when_quality_auto_is_in_effect() {
        let dir = tempfile::TempDir::new().unwrap();
        let src = png(dir.path(), 64, 64);
        let opts = SquishOptions {
            auto: true,
            ..Default::default()
        };

        assert!(matches!(
            estimate(
                &src,
                &opts,
                CropRect {
                    x: 0,
                    y: 0,
                    w: 32,
                    h: 32
                },
                "test-c"
            )
            .unwrap(),
            EstimateOutcome::Skipped(_)
        ));
    }

    #[test]
    fn skips_an_oversized_selection() {
        let dir = tempfile::TempDir::new().unwrap();
        let src = png(dir.path(), 64, 64);
        let opts = SquishOptions::default();

        assert!(matches!(
            estimate(
                &src,
                &opts,
                CropRect {
                    x: 0,
                    y: 0,
                    w: 9000,
                    h: 9000
                },
                "test-d"
            )
            .unwrap(),
            EstimateOutcome::Skipped(_)
        ));
    }

    #[test]
    fn removes_its_temp_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        let src = png(dir.path(), 100, 100);
        estimate(
            &src,
            &SquishOptions::default(),
            CropRect {
                x: 0,
                y: 0,
                w: 50,
                h: 50,
            },
            "test-e",
        )
        .unwrap();
        assert!(!work_dir("test-e").exists());
    }
}
