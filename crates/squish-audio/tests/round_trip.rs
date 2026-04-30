use squish_audio::{squish_audio, AudioCodec, AudioOptions};
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn has_ffmpeg() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn generate_sine(path: &Path, codec_args: &[&str]) {
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-y",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=1.0",
        "-ac",
        "2",
    ]);
    for a in codec_args {
        cmd.arg(a);
    }
    cmd.arg(path);
    let out = cmd.output().expect("ffmpeg invocation failed");
    assert!(
        out.status.success(),
        "ffmpeg failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn mp3_round_trip_default() {
    if !has_ffmpeg() {
        eprintln!("skip: no ffmpeg");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sine.mp3");
    generate_sine(&input, &["-c:a", "libmp3lame"]);

    let result = squish_audio(&input, &AudioOptions::default()).unwrap();
    assert!(result.output_path.exists());
    assert_eq!(result.codec_used, AudioCodec::Mp3);
    assert_eq!(result.output_path, tmp.path().join("sine_squished.mp3"));
}

#[test]
fn flac_explicit_codec_keeps_lossless() {
    if !has_ffmpeg() {
        eprintln!("skip: no ffmpeg");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sine.flac");
    generate_sine(&input, &["-c:a", "flac"]);

    let opts = AudioOptions {
        codec: Some(AudioCodec::Flac),
        ..Default::default()
    };
    let result = squish_audio(&input, &opts).unwrap();
    assert_eq!(result.codec_used, AudioCodec::Flac);
    assert!(result.output_path.exists());
    assert!(result.output_path.to_string_lossy().ends_with(".flac"));
}

#[test]
fn wav_default_converts_to_opus() {
    if !has_ffmpeg() {
        eprintln!("skip: no ffmpeg");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sine.wav");
    generate_sine(&input, &[]);

    let result = squish_audio(&input, &AudioOptions::default()).unwrap();
    assert_eq!(result.codec_used, AudioCodec::Opus);
    assert!(result.output_path.to_string_lossy().ends_with(".opus"));
    assert!(result.output_bytes > 0);
}

#[test]
fn bitrate_with_flac_errors() {
    if !has_ffmpeg() {
        eprintln!("skip: no ffmpeg");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sine.flac");
    generate_sine(&input, &["-c:a", "flac"]);

    let opts = AudioOptions {
        codec: Some(AudioCodec::Flac),
        bitrate_kbps: Some(128),
        ..Default::default()
    };
    let err = squish_audio(&input, &opts).unwrap_err();
    assert!(matches!(
        err,
        squish_audio::AudioError::InvalidOption { .. }
    ));
}

#[test]
fn explicit_mp3_codec_on_flac_input_produces_mp3() {
    if !has_ffmpeg() {
        eprintln!("skip: no ffmpeg");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sine.flac");
    generate_sine(&input, &["-c:a", "flac"]);

    let opts = AudioOptions {
        codec: Some(AudioCodec::Mp3),
        ..Default::default()
    };
    let result = squish_audio(&input, &opts).unwrap();
    assert_eq!(result.codec_used, AudioCodec::Mp3);
    assert!(result.output_path.to_string_lossy().ends_with(".mp3"));
}

#[test]
fn force_overwrite_reuses_path() {
    if !has_ffmpeg() {
        eprintln!("skip: no ffmpeg");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sine.mp3");
    generate_sine(&input, &["-c:a", "libmp3lame"]);

    let opts = AudioOptions {
        force_overwrite: true,
        ..Default::default()
    };
    let r1 = squish_audio(&input, &opts).unwrap();
    let r2 = squish_audio(&input, &opts).unwrap();
    assert_eq!(r1.output_path, r2.output_path);
}

#[test]
fn ambiguous_container_with_video_rejected() {
    if !has_ffmpeg() {
        eprintln!("skip: no ffmpeg");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("movie.m4a");
    // Generate an M4A container that *contains* video — this is unusual but ffmpeg allows
    // muxing a video stream into an .m4a, and our detection should reject it.
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=64x64:d=0.5",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=0.5",
            "-c:v",
            "libx264",
            "-c:a",
            "aac",
            "-shortest",
        ])
        .arg(&input)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "fixture build failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );

    let err = squish_audio(&input, &AudioOptions::default()).unwrap_err();
    assert!(matches!(err, squish_audio::AudioError::NotAudio { .. }));
}

#[test]
fn tag_preservation_default() {
    if !has_ffmpeg() {
        eprintln!("skip: no ffmpeg");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("tagged.mp3");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=0.5",
            "-c:a",
            "libmp3lame",
            "-metadata",
            "title=TestSong",
        ])
        .arg(&input)
        .output()
        .unwrap();
    assert!(status.status.success());

    let r = squish_audio(&input, &AudioOptions::default()).unwrap();

    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format_tags=title",
            "-of",
            "csv=p=0",
        ])
        .arg(&r.output_path)
        .output()
        .unwrap();
    let title = String::from_utf8_lossy(&probe.stdout);
    assert!(
        title.trim().contains("TestSong"),
        "title not preserved: {title}"
    );
}

#[test]
fn strip_tags_removes_title() {
    if !has_ffmpeg() {
        eprintln!("skip: no ffmpeg");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("tagged.mp3");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=0.5",
            "-c:a",
            "libmp3lame",
            "-metadata",
            "title=TestSong",
        ])
        .arg(&input)
        .output()
        .unwrap();
    assert!(status.status.success());

    let opts = AudioOptions {
        strip_tags: true,
        ..Default::default()
    };
    let r = squish_audio(&input, &opts).unwrap();

    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format_tags=title",
            "-of",
            "csv=p=0",
        ])
        .arg(&r.output_path)
        .output()
        .unwrap();
    let title = String::from_utf8_lossy(&probe.stdout);
    assert!(
        !title.trim().contains("TestSong"),
        "title should be stripped: {title}"
    );
}
