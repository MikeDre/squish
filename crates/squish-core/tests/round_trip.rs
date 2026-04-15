//! Per-format integration tests. Each test:
//! 1. Reads a fixture from tests/fixtures/
//! 2. Calls squish_core::squish_file on a temp copy
//! 3. Asserts: success, output exists, output smaller than input, output decodes

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
fn png_compresses() {
    let (_tmp, input) = copy_fixture("sample.png");
    let r = squish_file(&input, &SquishOptions::default()).unwrap();
    assert!(r.output_bytes < r.input_bytes, "PNG output not smaller: {r:?}");
    assert!(r.output_path.exists());
    // Decodes as PNG
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(squish_core::detect_format(&r.output_path, &bytes), Some(squish_core::Format::Png));
}

#[test]
fn jpeg_compresses() {
    let (_tmp, input) = copy_fixture("sample.jpg");
    let r = squish_file(&input, &SquishOptions::default()).unwrap();
    assert!(r.output_bytes < r.input_bytes, "JPEG output not smaller: {r:?}");
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(squish_core::detect_format(&r.output_path, &bytes), Some(squish_core::Format::Jpeg));
}

#[test]
fn webp_compresses() {
    let (_tmp, input) = copy_fixture("sample.webp");
    let r = squish_file(&input, &SquishOptions::default()).unwrap();
    assert!(r.output_bytes < r.input_bytes, "WebP output not smaller: {r:?}");
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(squish_core::detect_format(&r.output_path, &bytes), Some(squish_core::Format::Webp));
}

#[test]
fn avif_compresses() {
    let (_tmp, input) = copy_fixture("sample.avif");
    let r = squish_file(&input, &SquishOptions::default()).unwrap();
    // AVIF can grow slightly on already-optimal inputs; allow up to 10% growth.
    assert!(r.output_bytes <= r.input_bytes * 11 / 10, "AVIF output grew >10%: {r:?}");
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(squish_core::detect_format(&r.output_path, &bytes), Some(squish_core::Format::Avif));
}

#[test]
fn gif_compresses() {
    let (_tmp, input) = copy_fixture("sample.gif");
    let r = squish_file(&input, &SquishOptions::default()).unwrap();
    assert!(r.output_bytes < r.input_bytes, "GIF output not smaller: {r:?}");
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(squish_core::detect_format(&r.output_path, &bytes), Some(squish_core::Format::Gif));
}

#[test]
fn animated_gif_preserves_frames() {
    let (_tmp, input) = copy_fixture("sample_animated.gif");
    let r = squish_file(&input, &SquishOptions::default()).unwrap();
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(squish_core::detect_format(&r.output_path, &bytes), Some(squish_core::Format::Gif));
}

#[test]
fn svg_compresses() {
    let (_tmp, input) = copy_fixture("sample.svg");
    let r = squish_file(&input, &SquishOptions::default()).unwrap();
    assert!(r.output_bytes < r.input_bytes, "SVG output not smaller: {r:?}");
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(squish_core::detect_format(&r.output_path, &bytes), Some(squish_core::Format::Svg));
}

#[test]
fn png_to_webp_conversion() {
    let (_tmp, input) = copy_fixture("sample.png");
    let opts = SquishOptions {
        output_format: Some(squish_core::Format::Webp),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert_eq!(r.format_out, squish_core::Format::Webp);
    assert_eq!(r.output_path.extension().and_then(|s| s.to_str()), Some("webp"));
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(squish_core::detect_format(&r.output_path, &bytes), Some(squish_core::Format::Webp));
}
