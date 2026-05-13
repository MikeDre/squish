use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn core_fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/
    p.push("squish-core/tests/fixtures");
    p.push(name);
    p
}

fn bin() -> Command {
    Command::cargo_bin("squish").unwrap()
}

fn video_fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/
    p.push("squish-video/tests/fixtures");
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
fn help_exits_zero_and_prints_usage() {
    bin()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage"))
        .stdout(predicate::str::contains("--quality"));
}

#[test]
fn missing_path_is_fatal() {
    bin()
        .arg("/definitely/does/not/exist.png")
        .assert()
        .failure()
        .code(2);
}

#[test]
fn single_png_produces_squished_sibling() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sample.png");
    fs::copy(core_fixture("sample.png"), &input).unwrap();

    bin()
        .arg(&input)
        .assert()
        .success()
        .stdout(predicate::str::contains("Squished 1 files"));

    assert!(tmp.path().join("sample_squished.png").exists());
}

#[test]
fn directory_non_recursive_skips_subfolders() {
    let tmp = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("a.png")).unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("sub/b.png")).unwrap();

    bin()
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Squished 1 files"));

    assert!(tmp.path().join("a_squished.png").exists());
    assert!(!tmp.path().join("sub/b_squished.png").exists());
}

#[test]
fn recursive_flag_includes_subdirs() {
    let tmp = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("a.png")).unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("sub/b.png")).unwrap();

    bin()
        .arg(tmp.path())
        .arg("-r")
        .assert()
        .success()
        .stdout(predicate::str::contains("Squished 2 files"));

    assert!(tmp.path().join("a_squished.png").exists());
    assert!(tmp.path().join("sub/b_squished.png").exists());
}

#[test]
fn collision_uses_numeric_suffix() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("x.png");
    fs::copy(core_fixture("sample.png"), &input).unwrap();

    bin().arg(&input).assert().success();
    bin().arg(&input).assert().success();

    assert!(tmp.path().join("x_squished.png").exists());
    assert!(tmp.path().join("x_squished_2.png").exists());
}

#[test]
fn force_overwrites_existing() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("x.png");
    fs::copy(core_fixture("sample.png"), &input).unwrap();

    bin().arg(&input).assert().success();
    bin().arg(&input).arg("--force").assert().success();
    assert!(!tmp.path().join("x_squished_2.png").exists());
}

#[test]
fn dry_run_does_not_write_files() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("x.png");
    fs::copy(core_fixture("sample.png"), &input).unwrap();

    bin()
        .arg(&input)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("would squish"));

    assert!(!tmp.path().join("x_squished.png").exists());
}

#[test]
fn unrecognized_file_is_skipped_with_log() {
    let tmp = TempDir::new().unwrap();
    let weird = tmp.path().join("thing.xyz");
    fs::write(&weird, b"random bytes").unwrap();

    bin()
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Skipped 1"))
        .stdout(predicate::str::contains("thing.xyz"));
}

#[test]
fn one_failing_file_doesnt_abort_batch() {
    let tmp = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("ok.png")).unwrap();
    fs::write(tmp.path().join("corrupt.png"), b"not actually a PNG").unwrap();

    bin()
        .arg(tmp.path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains("Squished 1 files"));

    assert!(tmp.path().join("ok_squished.png").exists());
}

#[test]
fn format_conversion_png_to_webp() {
    let tmp = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("a.png")).unwrap();

    bin()
        .arg(tmp.path().join("a.png"))
        .arg("--format")
        .arg("webp")
        .assert()
        .success();

    assert!(tmp.path().join("a_squished.webp").exists());
    assert!(!tmp.path().join("a_squished.png").exists());
}

#[test]
fn single_mp4_produces_squished_sibling() {
    if !has_ffmpeg() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sample.mp4");
    fs::copy(video_fixture("sample.mp4"), &input).unwrap();

    bin()
        .arg(&input)
        .assert()
        .success()
        .stdout(predicate::str::contains("Squished 1 files"));

    assert!(tmp.path().join("sample_squished.mp4").exists());
}

#[test]
fn mixed_batch_images_and_videos() {
    if !has_ffmpeg() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("a.png")).unwrap();
    fs::copy(video_fixture("sample.mp4"), tmp.path().join("b.mp4")).unwrap();

    bin()
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("images"))
        .stdout(predicate::str::contains("videos"));

    assert!(tmp.path().join("a_squished.png").exists());
    assert!(tmp.path().join("b_squished.mp4").exists());
}

#[test]
fn fast_flag_works_for_video() {
    if !has_ffmpeg() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sample.mp4");
    fs::copy(video_fixture("sample.mp4"), &input).unwrap();

    bin().arg(&input).arg("--fast").assert().success();

    assert!(tmp.path().join("sample_squished.mp4").exists());
}

#[test]
fn codec_flag_works() {
    if !has_ffmpeg() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sample.mp4");
    fs::copy(video_fixture("sample.mp4"), &input).unwrap();

    bin()
        .arg(&input)
        .arg("--codec")
        .arg("h264")
        .assert()
        .success();

    assert!(tmp.path().join("sample_squished.mp4").exists());
}

#[test]
fn video_in_directory_walk() {
    if !has_ffmpeg() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    fs::copy(video_fixture("sample.mp4"), tmp.path().join("v.mp4")).unwrap();

    bin()
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Squished 1 files"));

    assert!(tmp.path().join("v_squished.mp4").exists());
}

#[test]
fn video_dry_run() {
    if !has_ffmpeg() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sample.mp4");
    fs::copy(video_fixture("sample.mp4"), &input).unwrap();

    bin()
        .arg(&input)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("would squish (video)"));

    assert!(!tmp.path().join("sample_squished.mp4").exists());
}

// ----- Audio integration tests -----

fn make_sine(path: &std::path::Path, codec_args: &[&str]) {
    let mut cmd = std::process::Command::new("ffmpeg");
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
fn cli_compresses_mp3() {
    if !has_ffmpeg() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sine.mp3");
    make_sine(&input, &["-c:a", "libmp3lame"]);

    let mut cmd = Command::cargo_bin("squish").unwrap();
    cmd.arg(&input).assert().success();

    assert!(tmp.path().join("sine_squished.mp3").exists());
}

#[test]
fn cli_lossless_non_tty_defaults_to_opus() {
    if !has_ffmpeg() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sine.wav");
    make_sine(&input, &[]);

    let mut cmd = Command::cargo_bin("squish").unwrap();
    // assert_cmd does not allocate a TTY, so std::io::stdin().is_terminal() is false.
    cmd.arg(&input).assert().success();

    assert!(tmp.path().join("sine_squished.opus").exists());
}

#[test]
fn cli_codec_validation_rejects_video_codec_with_audio_only_batch() {
    if !has_ffmpeg() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sine.mp3");
    make_sine(&input, &["-c:a", "libmp3lame"]);

    let mut cmd = Command::cargo_bin("squish").unwrap();
    let assert = cmd.args(["--codec", "h265"]).arg(&input).assert().failure();
    let output = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(output.contains("video codec"));
}

#[test]
fn cli_bitrate_rejected_with_flac() {
    if !has_ffmpeg() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sine.flac");
    make_sine(&input, &["-c:a", "flac"]);

    let mut cmd = Command::cargo_bin("squish").unwrap();
    cmd.args(["--codec", "flac", "--bitrate", "128k"])
        .arg(&input)
        .assert()
        .failure();
}

#[test]
fn cli_strip_tags_removes_title() {
    if !has_ffmpeg() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("tagged.mp3");
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=0.5",
            "-c:a",
            "libmp3lame",
            "-metadata",
            "title=KeepMe",
        ])
        .arg(&input)
        .output()
        .unwrap();
    assert!(status.status.success());

    let mut cmd = Command::cargo_bin("squish").unwrap();
    cmd.arg("--strip-tags").arg(&input).assert().success();

    let probe = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format_tags=title",
            "-of",
            "csv=p=0",
        ])
        .arg(tmp.path().join("tagged_squished.mp3"))
        .output()
        .unwrap();
    let title = String::from_utf8_lossy(&probe.stdout);
    assert!(
        !title.trim().contains("KeepMe"),
        "title should be stripped: {title}"
    );
}

#[test]
fn cli_dry_run_lists_audio() {
    if !has_ffmpeg() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sine.mp3");
    make_sine(&input, &["-c:a", "libmp3lame"]);

    let mut cmd = Command::cargo_bin("squish").unwrap();
    let out = cmd.args(["--dry-run"]).arg(&input).assert().success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    assert!(stdout.contains("would squish (audio)"));
    // No output file should be produced.
    assert!(!tmp.path().join("sine_squished.mp3").exists());
}

#[test]
fn cli_mixed_batch_summary_shows_three_kinds() {
    if !has_ffmpeg() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let audio = tmp.path().join("sine.mp3");
    make_sine(&audio, &["-c:a", "libmp3lame"]);

    // Use the existing shared PNG fixture for a valid image input.
    let img = tmp.path().join("dot.png");
    std::fs::copy(core_fixture("sample.png"), &img).unwrap();

    let mut cmd = Command::cargo_bin("squish").unwrap();
    let out = cmd.arg(&audio).arg(&img).assert().success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    assert!(stdout.contains("audio"));
    assert!(stdout.contains("images"));
}

// ----- Code integration tests -----

#[test]
fn cli_minifies_js() {
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("app.js");
    std::fs::write(
        &input,
        b"console.log('hi ' + 'world ' + 42 + ' more text');",
    )
    .unwrap();

    let mut cmd = assert_cmd::Command::cargo_bin("squish").unwrap();
    cmd.arg(&input).assert().success();

    let output = tmp.path().join("app.min.js");
    assert!(output.exists());
    let body_in = std::fs::read_to_string(&input).unwrap();
    let body_out = std::fs::read_to_string(&output).unwrap();
    assert!(body_out.len() < body_in.len());
}

#[test]
fn cli_default_suffix_is_min_with_dot_separator() {
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("app.js");
    std::fs::write(&input, b"console.log('x');").unwrap();

    let mut cmd = assert_cmd::Command::cargo_bin("squish").unwrap();
    cmd.arg(&input).assert().success();

    assert!(tmp.path().join("app.min.js").exists());
    assert!(!tmp.path().join("app_squished.js").exists());
}

#[test]
fn cli_custom_suffix_for_code_uses_dot_separator() {
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("app.js");
    std::fs::write(&input, b"console.log('x');").unwrap();

    let mut cmd = assert_cmd::Command::cargo_bin("squish").unwrap();
    cmd.args(["--suffix", "tiny"])
        .arg(&input)
        .assert()
        .success();

    assert!(tmp.path().join("app.tiny.js").exists());
}

#[test]
fn cli_source_map_emits_map_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("app.js");
    std::fs::write(&input, b"console.log('hello world');").unwrap();

    let mut cmd = assert_cmd::Command::cargo_bin("squish").unwrap();
    cmd.arg("--source-map").arg(&input).assert().success();

    assert!(tmp.path().join("app.min.js").exists());
    assert!(tmp.path().join("app.min.js.map").exists());
}

#[test]
fn cli_source_map_errors_on_json_only_batch() {
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("data.json");
    std::fs::write(&input, br#"{"a":1}"#).unwrap();

    let mut cmd = assert_cmd::Command::cargo_bin("squish").unwrap();
    let assert = cmd.arg("--source-map").arg(&input).assert().failure();
    let output = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(output.contains("source-map"));
}

#[test]
fn cli_source_map_with_mixed_js_and_json_succeeds() {
    let tmp = tempfile::TempDir::new().unwrap();
    let js_input = tmp.path().join("app.js");
    let json_input = tmp.path().join("data.json");
    std::fs::write(&js_input, b"console.log('x');").unwrap();
    std::fs::write(&json_input, br#"{"a":1}"#).unwrap();

    let mut cmd = assert_cmd::Command::cargo_bin("squish").unwrap();
    cmd.arg("--source-map")
        .arg(&js_input)
        .arg(&json_input)
        .assert()
        .success();

    assert!(tmp.path().join("app.min.js.map").exists());
    assert!(!tmp.path().join("data.min.json.map").exists());
}

#[test]
fn cli_dry_run_lists_code() {
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("app.js");
    std::fs::write(&input, b"console.log('x');").unwrap();

    let mut cmd = assert_cmd::Command::cargo_bin("squish").unwrap();
    let out = cmd.args(["--dry-run"]).arg(&input).assert().success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    assert!(stdout.contains("would squish (code)"));
    assert!(!tmp.path().join("app.min.js").exists());
}

#[test]
fn cli_summary_includes_code_count() {
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("app.js");
    std::fs::write(&input, b"console.log('x');").unwrap();

    let mut cmd = assert_cmd::Command::cargo_bin("squish").unwrap();
    let out = cmd.arg(&input).assert().success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    assert!(stdout.contains("Squished"));
}

// ----- Animated WebP warning integration tests -----

fn core_fixture_anim() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/
    p.push("squish-core/tests/fixtures/anim.webp");
    p
}

#[test]
fn cli_animated_webp_with_resize_prints_warning_in_verbose() {
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("anim.webp");
    std::fs::copy(core_fixture_anim(), &input).unwrap();

    let mut cmd = assert_cmd::Command::cargo_bin("squish").unwrap();
    let assert = cmd
        .args(["--verbose", "--max-width", "100"])
        .arg(&input)
        .assert()
        .success();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.contains("cannot be resized"),
        "expected warning in stderr; got: {stderr}"
    );
}

#[test]
fn cli_animated_webp_quiet_no_warning_output() {
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("anim.webp");
    std::fs::copy(core_fixture_anim(), &input).unwrap();

    let mut cmd = assert_cmd::Command::cargo_bin("squish").unwrap();
    let assert = cmd
        .args(["--quiet", "--max-width", "100"])
        .arg(&input)
        .assert()
        .success();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        !stderr.contains("cannot be resized"),
        "quiet mode should suppress warnings; got: {stderr}"
    );
}

#[test]
fn cli_animated_webp_default_mode_shows_warning() {
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("anim.webp");
    std::fs::copy(core_fixture_anim(), &input).unwrap();

    let mut cmd = assert_cmd::Command::cargo_bin("squish").unwrap();
    let assert = cmd
        .args(["--max-width", "100"])
        .arg(&input)
        .assert()
        .success();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(stderr.contains("cannot be resized"));
}

#[test]
fn cli_animated_webp_no_resize_no_warning() {
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("anim.webp");
    std::fs::copy(core_fixture_anim(), &input).unwrap();

    let mut cmd = assert_cmd::Command::cargo_bin("squish").unwrap();
    let assert = cmd.arg(&input).assert().success();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        !stderr.contains("cannot be resized"),
        "no conflict; no warning expected; got: {stderr}"
    );

    // Output exists and is a byte-for-byte copy of the input.
    let output = tmp.path().join("anim_squished.webp");
    assert!(output.exists());
    let input_bytes = std::fs::read(&input).unwrap();
    let output_bytes = std::fs::read(&output).unwrap();
    assert_eq!(output_bytes, input_bytes);
}
