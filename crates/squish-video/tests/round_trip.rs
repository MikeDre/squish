use squish_video::{squish_video, VideoCodec, VideoFormat, VideoOptions};
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

/// Generate a real NTSC DV fixture with ffmpeg. Returns false if generation
/// fails or produces an empty file (e.g. the build lacks the dvvideo encoder),
/// so the caller can skip gracefully rather than fail.
fn generate_dv_fixture(path: &std::path::Path) -> bool {
    // Preferred: -target ntsc-dv sets up the correct DV profile in one shot.
    let target = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=720x480:rate=30000/1001:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-target",
            "ntsc-dv",
        ])
        .arg(path)
        .output();

    if matches!(target, Ok(ref o) if o.status.success())
        && fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false)
    {
        return true;
    }

    // Fallback: explicit dvvideo parameters.
    let explicit = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=720x480:rate=30000/1001:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-c:v",
            "dvvideo",
            "-s",
            "720x480",
            "-r",
            "30000/1001",
            "-pix_fmt",
            "yuv411p",
            "-c:a",
            "pcm_s16le",
            "-ar",
            "48000",
            "-ac",
            "2",
        ])
        .arg(path)
        .output();

    matches!(explicit, Ok(ref o) if o.status.success())
        && fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false)
}

#[test]
fn dv_transcodes_to_mp4() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sample.dv");

    if !generate_dv_fixture(&input) {
        eprintln!("skipping: could not generate a valid .dv fixture with this ffmpeg build");
        return;
    }

    let result = squish_video(&input, &VideoOptions::default()).unwrap();

    assert_eq!(result.format_in, VideoFormat::Dv);
    assert_eq!(result.format_out, VideoFormat::Mp4);
    assert_eq!(
        result.output_path.extension().and_then(|e| e.to_str()),
        Some("mp4")
    );
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
