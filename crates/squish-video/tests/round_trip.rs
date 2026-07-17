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

    let opts = VideoOptions {
        fast: true,
        ..Default::default()
    };
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
fn overwrite_replaces_mp4_in_place() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not present");
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("clip.mp4");
    let gen = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=128x128:rate=15:duration=1",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&input)
        .output()
        .unwrap();
    assert!(gen.status.success(), "fixture generation failed");

    let opts = squish_video::VideoOptions {
        overwrite: true,
        ..Default::default()
    };
    let r = squish_video::squish_video(&input, &opts).unwrap();

    assert_eq!(r.output_path, input, "output must be the input path itself");
    assert!(input.exists());
    assert!(std::fs::metadata(&input).unwrap().len() > 0);
    assert!(!tmp.path().join("clip_squished.mp4").exists());
    let leftovers: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".sq-"))
        .collect();
    assert!(leftovers.is_empty(), "temp file must be renamed away");
}

#[test]
fn overwrite_refuses_dv_in_place() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not present");
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("clip.dv");
    let gen = std::process::Command::new("ffmpeg")
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
        .arg(&input)
        .output()
        .unwrap();
    if !gen.status.success() || std::fs::metadata(&input).map(|m| m.len()).unwrap_or(0) == 0 {
        eprintln!("skipping: ffmpeg build cannot produce DV");
        return;
    }

    let opts = squish_video::VideoOptions {
        overwrite: true,
        ..Default::default()
    };
    let err = squish_video::squish_video(&input, &opts).unwrap_err();
    assert!(
        matches!(err, squish_video::VideoError::InPlaceFormatChange { .. }),
        "expected InPlaceFormatChange, got {err:?}"
    );
    assert!(input.exists(), "original .dv must be untouched");
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

    let opts = VideoOptions {
        force_overwrite: true,
        ..Default::default()
    };

    let r1 = squish_video(&input, &opts).unwrap();
    assert!(r1.output_path.exists());

    let r2 = squish_video(&input, &opts).unwrap();
    assert_eq!(r1.output_path, r2.output_path);
}

#[test]
fn mp4_respects_target_size() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not present");
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("clip.mp4");
    // 3 s of random noise — incompressible, so the input is guaranteed to be
    // far larger than the budget and rate control has real work to do.
    let gen = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=320x240:d=3:r=30",
            "-vf",
            "geq=random(1)*255:128:128",
            "-c:v",
            "libx264",
            "-crf",
            "18",
        ])
        .arg(&input)
        .output()
        .unwrap();
    assert!(gen.status.success(), "fixture generation failed");
    let input_size = std::fs::metadata(&input).unwrap().len();
    assert!(input_size > 200_000, "fixture too small: {input_size}");

    let opts = squish_video::VideoOptions {
        codec: Some(squish_video::VideoCodec::H264),
        target_size: Some(50_000),
        ..Default::default()
    };
    let r = squish_video::squish_video(&input, &opts).unwrap();
    assert!(
        r.output_bytes <= 50_000,
        "output {} exceeds target 50000",
        r.output_bytes
    );
    assert!(r.output_bytes > 5_000, "suspiciously small output");
}

/// Generate an incompressible noise clip so rate control has real work to do
/// and the input is guaranteed far larger than any sane budget.
fn generate_noise_fixture(path: &std::path::Path, size: &str, secs: u32) {
    let gen = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("nullsrc=s={size}:d={secs}:r=30"),
            "-vf",
            "geq=random(1)*255:128:128",
            "-c:v",
            "libx264",
            "-crf",
            "18",
        ])
        .arg(path)
        .output()
        .unwrap();
    assert!(gen.status.success(), "fixture generation failed");
}

#[test]
fn h265_respects_target_size_via_two_pass() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not present");
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("clip.mp4");
    generate_noise_fixture(&input, "320x240", 3);
    let input_size = std::fs::metadata(&input).unwrap().len();
    assert!(input_size > 200_000, "fixture too small: {input_size}");

    let opts = squish_video::VideoOptions {
        codec: Some(squish_video::VideoCodec::H265),
        target_size: Some(60_000),
        ..Default::default()
    };
    let r = squish_video::squish_video(&input, &opts).unwrap();
    assert!(
        r.output_bytes <= 60_000,
        "H.265 two-pass output {} exceeds target 60000",
        r.output_bytes
    );
    assert!(r.output_bytes > 5_000, "suspiciously small output");
    // No pass-log files leaked into the working directory.
    let leaks: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.contains("2pass") || n.ends_with(".log") || n.ends_with(".log.mbtree")
        })
        .collect();
    assert!(leaks.is_empty(), "two-pass log files leaked: {leaks:?}");
}

#[test]
fn vp9_respects_target_size_via_two_pass() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not present");
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("clip.mp4");
    generate_noise_fixture(&input, "320x240", 3);

    let opts = squish_video::VideoOptions {
        codec: Some(squish_video::VideoCodec::Vp9),
        output_format: Some(squish_video::VideoFormat::Webm),
        target_size: Some(60_000),
        ..Default::default()
    };
    let r = squish_video::squish_video(&input, &opts).unwrap();
    assert!(
        r.output_bytes <= 60_000,
        "VP9 two-pass output {} exceeds target 60000",
        r.output_bytes
    );
    assert!(r.output_bytes > 3_000, "suspiciously small output");
}

#[test]
fn av1_respects_target_size_via_single_pass() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not present");
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("clip.mp4");
    generate_noise_fixture(&input, "192x144", 2);

    // AV1 has no VBV two-pass here: it must use plain VBR + the retry loop.
    // Regression guard for SVT-AV1 rejecting -maxrate ("Max Bitrate only
    // supported with CRF mode"), which previously errored the whole encode.
    let opts = squish_video::VideoOptions {
        codec: Some(squish_video::VideoCodec::AV1),
        target_size: Some(40_000),
        ..Default::default()
    };
    let r = squish_video::squish_video(&input, &opts).unwrap();
    assert!(r.output_path.exists(), "AV1 target-size produced no output");
    assert!(r.output_bytes > 0, "AV1 output is empty");
}

#[test]
fn target_size_with_fast_copy_errors() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not present");
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("clip.mp4");
    std::fs::copy(fixture("sample.mp4"), &input).unwrap();

    let opts = squish_video::VideoOptions {
        fast: true,
        target_size: Some(50_000),
        ..Default::default()
    };
    let err = squish_video::squish_video(&input, &opts).unwrap_err();
    assert!(
        format!("{err}").contains("target-size"),
        "expected target-size error, got: {err}"
    );
}
