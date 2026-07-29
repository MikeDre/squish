use image::{Rgb, RgbImage, Rgba, RgbaImage};
use squish_core::preview_bytes;
use std::path::PathBuf;
use tempfile::TempDir;

fn fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures");
    p.push(name);
    p
}

#[test]
fn downscales_large_source_and_reports_source_dims() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("big.png");
    RgbImage::from_pixel(5000, 2500, Rgb([10, 120, 200]))
        .save(&path)
        .unwrap();

    let p = preview_bytes(&path, 4000).unwrap();

    assert_eq!((p.source_w, p.source_h), (5000, 2500));
    assert_eq!((p.w, p.h), (4000, 2000), "fits max_edge, aspect preserved");
    assert_eq!(p.mime, "image/jpeg", "no alpha -> jpeg");
    assert!(!p.bytes.is_empty());
}

#[test]
fn never_upscales_a_small_source() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("small.png");
    RgbImage::from_pixel(100, 80, Rgb([0, 0, 0]))
        .save(&path)
        .unwrap();

    let p = preview_bytes(&path, 4000).unwrap();

    assert_eq!((p.w, p.h), (100, 80));
    assert_eq!((p.source_w, p.source_h), (100, 80));
}

#[test]
fn uses_png_when_the_source_has_alpha() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("alpha.png");
    RgbaImage::from_pixel(64, 64, Rgba([1, 2, 3, 128]))
        .save(&path)
        .unwrap();

    let p = preview_bytes(&path, 4000).unwrap();

    assert_eq!(p.mime, "image/png");
}

#[test]
fn decodes_formats_the_image_crate_cannot() {
    // HEIC goes through libheif, not `image` — the preview must still work.
    let p = preview_bytes(&fixture("sample.heic"), 4000).unwrap();
    assert!(p.source_w > 0 && p.source_h > 0);
    assert!(!p.bytes.is_empty());
}

#[test]
fn rejects_svg() {
    assert!(preview_bytes(&fixture("sample.svg"), 4000).is_err());
}
