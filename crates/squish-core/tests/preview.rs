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

    let p = preview_bytes(&path, 4000, &squish_core::SquishOptions::default()).unwrap();

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

    let p = preview_bytes(&path, 4000, &squish_core::SquishOptions::default()).unwrap();

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

    let p = preview_bytes(&path, 4000, &squish_core::SquishOptions::default()).unwrap();

    assert_eq!(p.mime, "image/png");
}

#[test]
fn decodes_formats_the_image_crate_cannot() {
    // HEIC goes through libheif, not `image` — the preview must still work.
    let p = preview_bytes(
        &fixture("sample.heic"),
        4000,
        &squish_core::SquishOptions::default(),
    )
    .unwrap();
    assert!(p.source_w > 0 && p.source_h > 0);
    assert!(!p.bytes.is_empty());
}

#[test]
fn renders_an_svg_preview_at_the_requested_size() {
    let path = fixture("sample.svg"); // 200×200 with a viewBox
    let opts = squish_core::SquishOptions {
        width: Some(600),
        ..Default::default()
    };

    let p = preview_bytes(&path, 4000, &opts).unwrap();

    assert_eq!(
        (p.source_w, p.source_h),
        (600, 600),
        "selection maths runs in render pixels"
    );
    assert_eq!((p.w, p.h), (600, 600), "under max_edge, so no downscale");
    assert_eq!(p.mime, "image/png", "a rendered canvas has alpha");
    assert!(!p.bytes.is_empty());
}

#[test]
fn caps_a_huge_svg_render_to_max_edge() {
    let path = fixture("sample.svg");
    let opts = squish_core::SquishOptions {
        width: Some(9000),
        ..Default::default()
    };

    let p = preview_bytes(&path, 4000, &opts).unwrap();

    assert_eq!((p.source_w, p.source_h), (9000, 9000));
    assert_eq!((p.w, p.h), (4000, 4000), "preview is capped, source is not");
}

#[test]
fn an_svg_without_a_size_cannot_be_previewed() {
    let path = fixture("sample.svg");
    let err = preview_bytes(&path, 4000, &squish_core::SquishOptions::default()).unwrap_err();
    assert!(matches!(
        err,
        squish_core::SquishError::MissingRenderSize { .. }
    ));
}
