use squish_video::{squish_video, VideoCodec, VideoOptions};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures");
    p.push(name);
    p
}

fn has_ffmpeg() -> bool {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn mp4_compresses() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sample.mp4");
    fs::copy(fixture("sample.mp4"), &input).unwrap();

    let result = squish_video(&input, &VideoOptions::default()).unwrap();
    assert!(result.output_path.exists());
    assert!(result.output_bytes > 0);
    assert_eq!(result.output_path, tmp.path().join("sample_squished.mp4"));
}

#[test]
fn webm_compresses() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sample.webm");
    fs::copy(fixture("sample.webm"), &input).unwrap();

    let result = squish_video(&input, &VideoOptions::default()).unwrap();
    assert!(result.output_path.exists());
    assert!(result.output_bytes > 0);
}

#[test]
fn mov_compresses() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sample.mov");
    fs::copy(fixture("sample.mov"), &input).unwrap();

    let result = squish_video(&input, &VideoOptions::default()).unwrap();
    assert!(result.output_path.exists());
    assert!(result.output_bytes > 0);
}

#[test]
fn fast_mode_produces_output() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sample.mp4");
    fs::copy(fixture("sample.mp4"), &input).unwrap();

    let opts = VideoOptions { fast: true, ..Default::default() };
    let result = squish_video(&input, &opts).unwrap();
    assert!(result.output_path.exists());
    assert!(result.output_bytes > 0);
}

#[test]
fn h264_codec_override() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sample.mp4");
    fs::copy(fixture("sample.mp4"), &input).unwrap();

    let opts = VideoOptions {
        codec: Some(VideoCodec::H264),
        ..Default::default()
    };
    let result = squish_video(&input, &opts).unwrap();
    assert!(result.output_path.exists());
    assert!(result.output_bytes > 0);
}

#[test]
fn force_overwrite_works() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sample.mp4");
    fs::copy(fixture("sample.mp4"), &input).unwrap();

    let opts = VideoOptions { force_overwrite: true, ..Default::default() };

    let r1 = squish_video(&input, &opts).unwrap();
    assert!(r1.output_path.exists());

    let r2 = squish_video(&input, &opts).unwrap();
    assert_eq!(r1.output_path, r2.output_path);
}
