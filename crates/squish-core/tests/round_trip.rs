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
    assert!(
        r.output_bytes < r.input_bytes,
        "PNG output not smaller: {r:?}"
    );
    assert!(r.output_path.exists());
    // Decodes as PNG
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(
        squish_core::detect_format(&r.output_path, &bytes),
        Some(squish_core::Format::Png)
    );
}

#[test]
fn jpeg_compresses() {
    let (_tmp, input) = copy_fixture("sample.jpg");
    let r = squish_file(&input, &SquishOptions::default()).unwrap();
    assert!(
        r.output_bytes < r.input_bytes,
        "JPEG output not smaller: {r:?}"
    );
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(
        squish_core::detect_format(&r.output_path, &bytes),
        Some(squish_core::Format::Jpeg)
    );
}

#[test]
fn webp_compresses() {
    let (_tmp, input) = copy_fixture("sample.webp");
    let r = squish_file(&input, &SquishOptions::default()).unwrap();
    assert!(
        r.output_bytes < r.input_bytes,
        "WebP output not smaller: {r:?}"
    );
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(
        squish_core::detect_format(&r.output_path, &bytes),
        Some(squish_core::Format::Webp)
    );
}

#[test]
fn avif_compresses() {
    let (_tmp, input) = copy_fixture("sample.avif");
    let r = squish_file(&input, &SquishOptions::default()).unwrap();
    // AVIF can grow slightly on already-optimal inputs; allow up to 10% growth.
    assert!(
        r.output_bytes <= r.input_bytes * 11 / 10,
        "AVIF output grew >10%: {r:?}"
    );
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(
        squish_core::detect_format(&r.output_path, &bytes),
        Some(squish_core::Format::Avif)
    );
}

#[test]
fn tiff_converts_to_jpeg_by_default() {
    let (_tmp, input) = copy_fixture("sample.tiff");
    let r = squish_file(&input, &SquishOptions::default()).unwrap();
    assert_eq!(r.format_in, squish_core::Format::Tiff);
    assert_eq!(r.format_out, squish_core::Format::Jpeg);
    assert_eq!(
        r.output_path.extension().and_then(|s| s.to_str()),
        Some("jpg")
    );
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(
        squish_core::detect_format(&r.output_path, &bytes),
        Some(squish_core::Format::Jpeg)
    );
}

#[test]
fn tiff_respects_explicit_format_override() {
    let (_tmp, input) = copy_fixture("sample.tiff");
    let opts = SquishOptions {
        output_format: Some(squish_core::Format::Webp),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert_eq!(r.format_out, squish_core::Format::Webp);
}

#[test]
fn heic_compresses() {
    let (_tmp, input) = copy_fixture("sample.heic");
    let r = squish_file(&input, &SquishOptions::default()).unwrap();
    // HEIC from phones is often well-compressed; allow up to 5% growth.
    assert!(
        r.output_bytes <= r.input_bytes * 21 / 20,
        "HEIC output grew >5%: {r:?}"
    );
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(
        squish_core::detect_format(&r.output_path, &bytes),
        Some(squish_core::Format::Heic)
    );
}

#[test]
fn gif_compresses() {
    let (_tmp, input) = copy_fixture("sample.gif");
    let r = squish_file(&input, &SquishOptions::default()).unwrap();
    assert!(
        r.output_bytes < r.input_bytes,
        "GIF output not smaller: {r:?}"
    );
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(
        squish_core::detect_format(&r.output_path, &bytes),
        Some(squish_core::Format::Gif)
    );
}

#[test]
fn animated_gif_preserves_frames() {
    let (_tmp, input) = copy_fixture("sample_animated.gif");
    let r = squish_file(&input, &SquishOptions::default()).unwrap();
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(
        squish_core::detect_format(&r.output_path, &bytes),
        Some(squish_core::Format::Gif)
    );
}

#[test]
fn svg_compresses() {
    let (_tmp, input) = copy_fixture("sample.svg");
    let r = squish_file(&input, &SquishOptions::default()).unwrap();
    assert!(
        r.output_bytes < r.input_bytes,
        "SVG output not smaller: {r:?}"
    );
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(
        squish_core::detect_format(&r.output_path, &bytes),
        Some(squish_core::Format::Svg)
    );
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
    assert_eq!(
        r.output_path.extension().and_then(|s| s.to_str()),
        Some("webp")
    );
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(
        squish_core::detect_format(&r.output_path, &bytes),
        Some(squish_core::Format::Webp)
    );
}

#[test]
fn png_to_jpeg_conversion() {
    // Regression: previously aborted (exit 101) because raw PNG bytes were
    // passed straight to mozjpeg's decoder.
    let (_tmp, input) = copy_fixture("sample.png");
    let opts = SquishOptions {
        output_format: Some(squish_core::Format::Jpeg),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert_eq!(r.format_out, squish_core::Format::Jpeg);
    assert_eq!(
        r.output_path.extension().and_then(|s| s.to_str()),
        Some("jpg")
    );
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(
        squish_core::detect_format(&r.output_path, &bytes),
        Some(squish_core::Format::Jpeg)
    );
}

#[test]
fn jpeg_to_png_conversion() {
    let (_tmp, input) = copy_fixture("sample.jpg");
    let opts = SquishOptions {
        output_format: Some(squish_core::Format::Png),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert_eq!(r.format_out, squish_core::Format::Png);
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(
        squish_core::detect_format(&r.output_path, &bytes),
        Some(squish_core::Format::Png)
    );
}

#[test]
fn webp_to_gif_conversion() {
    let (_tmp, input) = copy_fixture("sample.webp");
    let opts = SquishOptions {
        output_format: Some(squish_core::Format::Gif),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert_eq!(r.format_out, squish_core::Format::Gif);
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(
        squish_core::detect_format(&r.output_path, &bytes),
        Some(squish_core::Format::Gif)
    );
}

#[test]
fn heic_to_jpeg_conversion() {
    let (_tmp, input) = copy_fixture("sample.heic");
    let opts = SquishOptions {
        output_format: Some(squish_core::Format::Jpeg),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert_eq!(r.format_out, squish_core::Format::Jpeg);
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(
        squish_core::detect_format(&r.output_path, &bytes),
        Some(squish_core::Format::Jpeg)
    );
}

#[test]
fn png_to_heic_conversion() {
    let (_tmp, input) = copy_fixture("sample.png");
    let opts = SquishOptions {
        output_format: Some(squish_core::Format::Heic),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert_eq!(r.format_out, squish_core::Format::Heic);
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(
        squish_core::detect_format(&r.output_path, &bytes),
        Some(squish_core::Format::Heic)
    );
}

#[test]
fn animated_webp_roundtrips_unchanged() {
    let (_tmp, input) = copy_fixture("anim.webp");
    let original = fs::read(&input).unwrap();

    let result = squish_file(&input, &SquishOptions::default()).unwrap();

    let output = fs::read(&result.output_path).unwrap();
    assert_eq!(
        output, original,
        "animated WebP must pass through unchanged"
    );
    assert!(
        result
            .output_path
            .to_string_lossy()
            .ends_with("_squished.webp"),
        "output path: {}",
        result.output_path.display()
    );
    assert!(result.warnings.is_empty());
}

#[test]
fn animated_webp_with_resize_produces_warning() {
    let (_tmp, input) = copy_fixture("anim.webp");
    let original = fs::read(&input).unwrap();

    let opts = SquishOptions {
        max_width: Some(100),
        ..Default::default()
    };
    let result = squish_file(&input, &opts).unwrap();

    let output = fs::read(&result.output_path).unwrap();
    assert_eq!(output, original, "resize must not modify animated WebP");
    assert_eq!(result.warnings.len(), 1);
    assert!(result.warnings[0].contains("cannot be resized"));
}

#[test]
fn jpeg_respects_target_size() {
    let (_tmp, input) = copy_fixture("sample.jpg");
    // Default-quality squish of this fixture lands well above 12 KB, so the
    // search must lower quality to fit the budget.
    let opts = SquishOptions {
        target_size: Some(12_000),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert!(
        r.output_bytes <= 12_000,
        "output {} exceeds target 12000",
        r.output_bytes
    );
    assert!(
        r.warnings.is_empty(),
        "unexpected warnings: {:?}",
        r.warnings
    );
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(
        squish_core::detect_format(&r.output_path, &bytes),
        Some(squish_core::Format::Jpeg)
    );
}

#[test]
fn target_size_uses_budget_for_quality() {
    // A generous budget should produce a *larger* (higher-quality) output than
    // a tight one — the search picks the highest quality that fits.
    let (_tmp1, input1) = copy_fixture("sample.jpg");
    let generous = squish_file(
        &input1,
        &SquishOptions {
            target_size: Some(40_000),
            ..Default::default()
        },
    )
    .unwrap();
    let (_tmp2, input2) = copy_fixture("sample.jpg");
    let tight = squish_file(
        &input2,
        &SquishOptions {
            target_size: Some(8_000),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(generous.output_bytes <= 40_000);
    assert!(tight.output_bytes <= 8_000);
    assert!(
        generous.output_bytes > tight.output_bytes,
        "generous {} should beat tight {}",
        generous.output_bytes,
        tight.output_bytes
    );
}

#[test]
fn target_size_unreachable_warns_and_writes_best_effort() {
    let (_tmp, input) = copy_fixture("sample.jpg");
    // 200 bytes is impossible for this image even at quality 1.
    let opts = SquishOptions {
        target_size: Some(200),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert!(r.output_path.exists());
    assert!(r.output_bytes > 200, "200 bytes should be unreachable");
    assert!(
        r.warnings.iter().any(|w| w.contains("target")),
        "expected a target-size warning, got: {:?}",
        r.warnings
    );
}

#[test]
fn target_size_no_dial_format_warns_when_over() {
    // SVG has no quality dial; an impossible target must warn, not loop.
    let (_tmp, input) = copy_fixture("sample.svg");
    let opts = SquishOptions {
        target_size: Some(10),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert!(r.output_bytes > 10);
    assert!(
        r.warnings.iter().any(|w| w.contains("target")),
        "expected a target-size warning, got: {:?}",
        r.warnings
    );
}

#[test]
fn target_size_applies_to_cross_format_conversion() {
    let (_tmp, input) = copy_fixture("sample.png");
    let opts = SquishOptions {
        target_size: Some(15_000),
        output_format: Some(squish_core::Format::Webp),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert!(
        r.output_bytes <= 15_000,
        "output {} exceeds target 15000",
        r.output_bytes
    );
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(
        squish_core::detect_format(&r.output_path, &bytes),
        Some(squish_core::Format::Webp)
    );
}

#[test]
fn svg_to_png_at_a_requested_width() {
    // sample.svg is 200×200 with a viewBox.
    let (_tmp, input) = copy_fixture("sample.svg");
    let opts = SquishOptions {
        output_format: Some(squish_core::Format::Png),
        width: Some(512),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert_eq!(r.format_out, squish_core::Format::Png);
    assert_eq!(
        r.output_path.extension().and_then(|s| s.to_str()),
        Some("png")
    );
    let img = image::open(&r.output_path).unwrap();
    assert_eq!((img.width(), img.height()), (512, 512));
}

#[test]
fn svg_to_png_without_a_size_is_an_error() {
    let (_tmp, input) = copy_fixture("sample.svg");
    let opts = SquishOptions {
        output_format: Some(squish_core::Format::Png),
        ..Default::default()
    };
    let err = squish_file(&input, &opts).unwrap_err();
    assert!(matches!(
        err,
        squish_core::SquishError::MissingRenderSize { .. }
    ));
}

#[test]
fn svg_to_jpeg_lands_on_white() {
    let (_tmp, input) = copy_fixture("sample.svg");
    let opts = SquishOptions {
        output_format: Some(squish_core::Format::Jpeg),
        width: Some(256),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    let px = image::open(&r.output_path)
        .unwrap()
        .to_rgb8()
        .get_pixel(2, 2)
        .0;
    assert!(
        px.iter().all(|c| *c > 235),
        "the transparent corner must be white, got {px:?}"
    );
}

#[test]
fn svg_to_webp_and_avif_convert() {
    for (fmt, ext) in [
        (squish_core::Format::Webp, "webp"),
        (squish_core::Format::Avif, "avif"),
    ] {
        let (_tmp, input) = copy_fixture("sample.svg");
        let opts = SquishOptions {
            output_format: Some(fmt),
            width: Some(128),
            ..Default::default()
        };
        let r = squish_file(&input, &opts).unwrap();
        assert_eq!(r.format_out, fmt);
        assert_eq!(
            r.output_path.extension().and_then(|s| s.to_str()),
            Some(ext)
        );
    }
}

#[test]
fn svg_to_svg_ignores_width_and_stays_vector() {
    let (_tmp, input) = copy_fixture("sample.svg");
    let opts = SquishOptions {
        width: Some(512),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert_eq!(r.format_out, squish_core::Format::Svg);
    assert!(
        r.warnings.iter().any(|w| w.contains("--width")),
        "expected a warning that --width was ignored: {:?}",
        r.warnings
    );
}

#[test]
fn width_on_a_raster_warns_and_still_compresses() {
    let (_tmp, input) = copy_fixture("sample.png");
    let opts = SquishOptions {
        width: Some(512),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert_eq!(r.format_out, squish_core::Format::Png);
    assert!(
        r.warnings.iter().any(|w| w.contains("--max-width")),
        "the warning should point at --max-width: {:?}",
        r.warnings
    );
}

#[test]
fn svg_to_jpeg_respects_target_size() {
    let (_tmp, input) = copy_fixture("sample.svg");
    let opts = SquishOptions {
        output_format: Some(squish_core::Format::Jpeg),
        width: Some(512),
        target_size: Some(8_000),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert!(
        r.output_bytes <= 8_000,
        "target-size should now drive the quality search: {} bytes",
        r.output_bytes
    );
}

#[test]
fn max_width_still_clamps_a_render() {
    // --width sizes the render canvas; --max-* clamps afterwards, exactly as
    // it would for a raster of that size.
    let (_tmp, input) = copy_fixture("sample.svg");
    let opts = SquishOptions {
        output_format: Some(squish_core::Format::Png),
        width: Some(512),
        max_width: Some(256),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    let img = image::open(&r.output_path).unwrap();
    assert_eq!((img.width(), img.height()), (256, 256));
}

#[test]
fn svg_to_jpeg_respects_quality_auto() {
    // The perceptual search needs a decodable source, which a vector only has
    // once it can be rendered.
    let (_tmp, input) = copy_fixture("sample.svg");
    let opts = SquishOptions {
        output_format: Some(squish_core::Format::Jpeg),
        width: Some(256),
        auto: true,
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert_eq!(r.format_out, squish_core::Format::Jpeg);
    let img = image::open(&r.output_path).unwrap();
    assert_eq!((img.width(), img.height()), (256, 256));
}

#[test]
fn a_render_may_be_larger_than_its_source() {
    // Rasterising is a representation change, so growth is expected here.
    // (The never-grow guard itself lives in the CLI runner — Task 8 covers it.)
    let (_tmp, input) = copy_fixture("sample.svg");
    let opts = SquishOptions {
        output_format: Some(squish_core::Format::Png),
        width: Some(1024),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert!(
        r.output_bytes > r.input_bytes,
        "a 1024px render should exceed the 1 KB source: {} vs {}",
        r.output_bytes,
        r.input_bytes
    );
    let bytes = fs::read(&r.output_path).unwrap();
    assert_eq!(
        squish_core::detect_format(&r.output_path, &bytes),
        Some(squish_core::Format::Png),
        "the output must still be a PNG, not a copy of the SVG"
    );
}

#[test]
fn raster_to_svg_is_still_refused() {
    let (_tmp, input) = copy_fixture("sample.png");
    let opts = SquishOptions {
        output_format: Some(squish_core::Format::Svg),
        ..Default::default()
    };
    let err = squish_file(&input, &opts).unwrap_err();
    assert!(matches!(
        err,
        squish_core::SquishError::UnsupportedFormat { .. }
    ));
}
