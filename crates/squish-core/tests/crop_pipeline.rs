//! Pipeline integration tests for --crop. Fixtures sample.{png,jpg} are
//! 640x480; anim.webp is animated.

use squish_core::{squish_file, CropSpec, Gravity, SquishOptions};
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

fn crop_opts(spec: CropSpec) -> SquishOptions {
    SquishOptions {
        crop: Some(spec),
        ..Default::default()
    }
}

#[test]
fn aspect_crop_square_png() {
    let (_tmp, input) = copy_fixture("sample.png");
    let r = squish_file(&input, &crop_opts(CropSpec::Aspect { w: 1, h: 1 })).unwrap();
    assert_eq!(image::image_dimensions(&r.output_path).unwrap(), (480, 480));
    assert!(r.warnings.is_empty());
}

#[test]
fn exact_crop_jpeg() {
    let (_tmp, input) = copy_fixture("sample.jpg");
    let r = squish_file(
        &input,
        &crop_opts(CropSpec::Exact {
            w: 300,
            h: 200,
            x: 10,
            y: 20,
        }),
    )
    .unwrap();
    assert_eq!(image::image_dimensions(&r.output_path).unwrap(), (300, 200));
}

#[test]
fn crop_composes_with_resize() {
    // 640x480 → crop 1:1 → 480x480 → max_width 240 → 240x240
    let (_tmp, input) = copy_fixture("sample.png");
    let opts = SquishOptions {
        crop: Some(CropSpec::Aspect { w: 1, h: 1 }),
        max_width: Some(240),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert_eq!(image::image_dimensions(&r.output_path).unwrap(), (240, 240));
}

#[test]
fn crop_composes_with_format_conversion() {
    let (_tmp, input) = copy_fixture("sample.png");
    let opts = SquishOptions {
        crop: Some(CropSpec::Aspect { w: 16, h: 9 }),
        output_format: Some(squish_core::Format::Webp),
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert_eq!(r.format_out, squish_core::Format::Webp);
    assert_eq!(image::image_dimensions(&r.output_path).unwrap(), (640, 360));
}

#[test]
fn gravity_west_keeps_left_content() {
    // Left half red, right half blue; 1:1 west crop must be all red.
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("wide.png");
    let mut img = image::RgbImage::from_pixel(4, 2, image::Rgb([0, 0, 255]));
    for y in 0..2 {
        for x in 0..2 {
            img.put_pixel(x, y, image::Rgb([255, 0, 0]));
        }
    }
    img.save(&input).unwrap();
    let opts = SquishOptions {
        crop: Some(CropSpec::Aspect { w: 1, h: 1 }),
        gravity: Gravity::West,
        lossless: true, // keep exact pixel values through PNG quantization
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    let out = image::open(&r.output_path).unwrap().to_rgb8();
    assert_eq!(out.dimensions(), (2, 2));
    assert_eq!(out.get_pixel(0, 0), &image::Rgb([255, 0, 0]));
    assert_eq!(out.get_pixel(1, 1), &image::Rgb([255, 0, 0]));
}

#[test]
fn out_of_bounds_exact_crop_is_invalid_crop_error() {
    let (_tmp, input) = copy_fixture("sample.png");
    let err = squish_file(
        &input,
        &crop_opts(CropSpec::Exact {
            w: 100,
            h: 100,
            x: 9999,
            y: 0,
        }),
    )
    .unwrap_err();
    assert!(matches!(err, squish_core::SquishError::InvalidCrop { .. }));
}

#[test]
fn svg_crop_warns_and_passes_through() {
    let (_tmp, input) = copy_fixture("sample.svg");
    let r = squish_file(&input, &crop_opts(CropSpec::Aspect { w: 1, h: 1 })).unwrap();
    assert_eq!(r.warnings.len(), 1);
    assert!(r.warnings[0].contains("--crop is not supported for SVG"));
    assert_eq!(r.format_out, squish_core::Format::Svg);
}

#[test]
fn animated_webp_crop_warns_and_passes_through() {
    let (_tmp, input) = copy_fixture("anim.webp");
    let r = squish_file(&input, &crop_opts(CropSpec::Aspect { w: 1, h: 1 })).unwrap();
    assert!(r
        .warnings
        .iter()
        .any(|w| w.contains("--crop is not supported for animated WebP")));
}

#[test]
fn crop_composes_with_quality_auto() {
    let (_tmp, input) = copy_fixture("sample.jpg");
    let opts = SquishOptions {
        crop: Some(CropSpec::Aspect { w: 1, h: 1 }),
        auto: true,
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();
    assert_eq!(image::image_dimensions(&r.output_path).unwrap(), (480, 480));
}

#[test]
fn gif_exact_crop_via_gifsicle() {
    let (_tmp, input) = copy_fixture("sample.gif");
    let r = squish_file(
        &input,
        &crop_opts(CropSpec::Exact {
            w: 100,
            h: 80,
            x: 10,
            y: 10,
        }),
    )
    .unwrap();
    assert_eq!(image::image_dimensions(&r.output_path).unwrap(), (100, 80));
}

#[test]
fn animated_gif_crop_preserves_frames() {
    use image::AnimationDecoder;
    let (_tmp, input) = copy_fixture("sample_animated.gif");
    let r = squish_file(&input, &crop_opts(CropSpec::Aspect { w: 1, h: 1 })).unwrap();
    // 320x240 → 1:1 → 240x240
    assert_eq!(image::image_dimensions(&r.output_path).unwrap(), (240, 240));
    let file = std::fs::File::open(&r.output_path).unwrap();
    let decoder = image::codecs::gif::GifDecoder::new(std::io::BufReader::new(file)).unwrap();
    let frames = decoder
        .into_frames()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(
        frames.len() > 1,
        "animation flattened: {} frame(s)",
        frames.len()
    );
}
