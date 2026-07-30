//! Interactive crop selection (`--select`).
//!
//! The picker's only job is to produce a rectangle and hand it to the Phase 1
//! crop engine as `CropSpec::Exact`. Everything downstream — cropping,
//! resizing, encoding, naming, the never-grow guarantee — is unchanged.

use crate::cli::Args;
use anyhow::{bail, Result};
use squish_core::{CropRect, CropSpec, Format, Gravity, SquishOptions};
use std::io::IsTerminal;
use std::path::PathBuf;

mod estimate;
mod server;

/// Longest edge of the preview handed to the browser.
pub(crate) const PREVIEW_MAX_EDGE: u32 = 4000;

/// Whether the TTY requirement is waived (tests, and automation that drives
/// the selector over HTTP itself).
fn no_open() -> bool {
    std::env::var_os("SQUISH_SELECT_NO_OPEN").is_some()
}

/// Validate that this run can host an interactive selection and return the one
/// image to crop.
///
/// Every check here runs *before* a server starts or a browser opens: throwing
/// away a selection the user already made is the worst possible outcome, so a
/// run that cannot succeed must fail while nothing has been invested.
pub(crate) fn preflight(worklist: &[PathBuf]) -> Result<PathBuf> {
    if !std::io::stderr().is_terminal() && !no_open() {
        bail!(
            "--select needs an interactive terminal; \
             pass an explicit --crop WxH+X+Y instead"
        );
    }
    match worklist.len() {
        0 => bail!("--select needs one image input, but no files matched"),
        1 => {}
        n => bail!(
            "--select needs exactly one image, but the given paths matched {n} files\n\
             help: crop one file at a time, or use a fixed crop like --crop 16:9"
        ),
    }

    let path = worklist[0].clone();
    let bytes = std::fs::read(&path)?;
    let Some(format) = squish_core::detect_format(&path, &bytes) else {
        bail!("--select needs an image; {} is not one", path.display());
    };
    if format == Format::Svg {
        bail!(
            "--select cannot crop {}: SVG is a vector format",
            path.display()
        );
    }
    if format == Format::Webp && squish_core::formats::webp::is_animated_webp(&bytes) {
        bail!(
            "--select cannot crop {}: animated WebP is not croppable",
            path.display()
        );
    }
    Ok(path)
}

/// The rectangle the selector opens with: the resolved `--crop` (anchored by
/// `--gravity`, exactly as a non-interactive run would place it) when one was
/// given, otherwise a centred box at 80% of each dimension.
pub(crate) fn seed_rect(
    spec: Option<CropSpec>,
    gravity: Gravity,
    w: u32,
    h: u32,
) -> Result<CropRect> {
    match spec {
        None => {
            let cw = ((w as u64 * 4 / 5) as u32).max(1);
            let ch = ((h as u64 * 4 / 5) as u32).max(1);
            Ok(CropRect {
                x: (w - cw) / 2,
                y: (h - ch) / 2,
                w: cw,
                h: ch,
            })
        }
        Some(spec) => match spec.resolve(gravity, w, h) {
            Ok(Some(r)) => Ok(r),
            // A full-image spec resolves to "no crop"; the selector still needs
            // a concrete box, so open on the whole image.
            Ok(None) => Ok(CropRect { x: 0, y: 0, w, h }),
            Err(reason) => bail!("--crop {reason}"),
        },
    }
}

/// The aspect ratio the selector should lock to, if the user asked for one.
/// Only an aspect `--crop` locks: an exact rect is a starting point, not a
/// constraint.
pub(crate) fn ratio_lock(spec: Option<CropSpec>) -> Option<(u32, u32)> {
    match spec {
        Some(CropSpec::Aspect { w, h }) => Some((w, h)),
        _ => None,
    }
}

/// The outcome of an interactive selection: the chosen rect (None = cancelled)
/// plus the source dimensions it was chosen against, so the caller can tell a
/// whole-image selection from a real crop.
pub(crate) struct Selection {
    pub rect: Option<CropRect>,
    pub source: (u32, u32),
}

/// Run an interactive selection. `rect: None` means the user cancelled.
pub(crate) fn resolve_crop(
    worklist: &[PathBuf],
    args: &Args,
    opts: &SquishOptions,
) -> Result<Selection> {
    let path = preflight(worklist)?;
    // Fails fast on an image that cannot be decoded, before any UI exists.
    let preview = squish_core::preview_bytes(&path, PREVIEW_MAX_EDGE)?;
    let seed = seed_rect(args.crop, args.gravity, preview.source_w, preview.source_h)?;
    let source = (preview.source_w, preview.source_h);
    let session = server::Session {
        file_name: path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        source_bytes: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
        lock: ratio_lock(args.crop),
        settings: estimate::settings_label(opts, &path),
        source_path: path.clone(),
        opts: opts.clone(),
        preview,
        seed,
    };

    let rect = match server::run(&session)? {
        server::Outcome::Cropped(r) => Some(r),
        server::Outcome::Cancelled => None,
        server::Outcome::TimedOut => {
            anyhow::bail!("crop selector timed out after 10 minutes with no selection")
        }
    };
    Ok(Selection { rect, source })
}

#[cfg(test)]
mod tests {
    use super::*;
    use squish_core::{CropSpec, Gravity};

    #[test]
    fn seed_without_crop_is_centred_80_percent() {
        let r = seed_rect(None, Gravity::Center, 1000, 500).unwrap();
        assert_eq!((r.w, r.h), (800, 400));
        assert_eq!((r.x, r.y), (100, 50));
    }

    #[test]
    fn seed_from_aspect_spec_matches_the_engine() {
        // Must agree exactly with what --crop 16:9 does today.
        let expected = CropSpec::Aspect { w: 16, h: 9 }
            .resolve(Gravity::North, 640, 480)
            .unwrap()
            .unwrap();
        let r = seed_rect(
            Some(CropSpec::Aspect { w: 16, h: 9 }),
            Gravity::North,
            640,
            480,
        )
        .unwrap();
        assert_eq!(
            (r.x, r.y, r.w, r.h),
            (expected.x, expected.y, expected.w, expected.h)
        );
    }

    #[test]
    fn seed_from_exact_spec_is_that_rect() {
        let r = seed_rect(
            Some(CropSpec::Exact {
                w: 300,
                h: 200,
                x: 10,
                y: 20,
            }),
            Gravity::Center,
            640,
            480,
        )
        .unwrap();
        assert_eq!((r.x, r.y, r.w, r.h), (10, 20, 300, 200));
    }

    #[test]
    fn seed_from_full_image_spec_is_the_whole_image() {
        // resolve() reports a no-op for a full-image spec; the selector still
        // needs a concrete box to open with.
        let r = seed_rect(
            Some(CropSpec::Exact {
                w: 640,
                h: 480,
                x: 0,
                y: 0,
            }),
            Gravity::Center,
            640,
            480,
        )
        .unwrap();
        assert_eq!((r.x, r.y, r.w, r.h), (0, 0, 640, 480));
    }

    #[test]
    fn seed_from_out_of_bounds_spec_errors() {
        assert!(seed_rect(
            Some(CropSpec::Exact {
                w: 10,
                h: 10,
                x: 9999,
                y: 0
            }),
            Gravity::Center,
            640,
            480,
        )
        .is_err());
    }

    #[test]
    fn ratio_lock_comes_from_an_aspect_spec_only() {
        assert_eq!(
            ratio_lock(Some(CropSpec::Aspect { w: 16, h: 9 })),
            Some((16, 9))
        );
        assert_eq!(
            ratio_lock(Some(CropSpec::Exact {
                w: 10,
                h: 10,
                x: 0,
                y: 0
            })),
            None,
            "an exact rect is a starting point, not a constraint"
        );
        assert_eq!(ratio_lock(None), None);
    }

    #[test]
    fn seed_never_produces_a_zero_sized_box() {
        let r = seed_rect(None, Gravity::Center, 1, 1).unwrap();
        assert!(r.w >= 1 && r.h >= 1);
    }
}
