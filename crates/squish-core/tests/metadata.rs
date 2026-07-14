//! JPEG EXIF-orientation correctness test.
//!
//! `exif_sample.jpg` (60x40) was generated with Pillow: EXIF Orientation=6
//! (rotate 90° CW needed to display upright), a `Make` tag, a GPS IFD, and a
//! fake ICC profile. Only orientation is exercised here — EXIF/ICC
//! preservation is a separate, following change (this fixture already has
//! what that change will need too, so it's created once, up front).

use squish_core::{squish_file, SquishOptions};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures");
    p.push(name);
    p
}

fn copy_fixture(name: &str) -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let dst = tmp.path().join(name);
    fs::copy(fixture(name), &dst).unwrap();
    (tmp, dst)
}

#[test]
fn exif_orientation_is_applied_to_pixels() {
    let (_tmp, input) = copy_fixture("exif_sample.jpg");
    let r = squish_file(&input, &SquishOptions::default()).unwrap();

    // Orientation 6 (rotate 90 CW) on a 60x40 source must land as 40x60 —
    // before this fix, orientation was silently dropped and the output kept
    // the un-rotated 60x40 dimensions (visually wrong once a viewer, which
    // has no orientation tag to correct it, displays it as-is).
    assert_eq!(
        image::image_dimensions(&r.output_path).unwrap(),
        (40, 60),
        "orientation was not applied to pixels"
    );
}
