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

/// Always use this helper to spawn the squish binary from tests. It sets
/// `SQUISH_NO_STATS=1` so test runs never pollute the developer's real usage
/// ledger at `~/Library/Application Support/squish/usage.jsonl`. The
/// `no_direct_cargo_bin_calls_in_this_file` test enforces the discipline.
fn bin() -> Command {
    let mut cmd = Command::cargo_bin("squish").unwrap();
    cmd.env("SQUISH_NO_STATS", "1");
    // Hermetic: never read the developer's real global config in tests.
    cmd.env("SQUISH_GLOBAL_CONFIG", "/nonexistent/squish-config.toml");
    cmd
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

    let mut cmd = bin();
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

    let mut cmd = bin();
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

    let mut cmd = bin();
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

    let mut cmd = bin();
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

    let mut cmd = bin();
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

    let mut cmd = bin();
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

    let mut cmd = bin();
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

    let mut cmd = bin();
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

    let mut cmd = bin();
    cmd.arg(&input).assert().success();

    assert!(tmp.path().join("app.min.js").exists());
    assert!(!tmp.path().join("app_squished.js").exists());
}

#[test]
fn cli_custom_suffix_for_code_uses_dot_separator() {
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("app.js");
    std::fs::write(&input, b"console.log('x');").unwrap();

    let mut cmd = bin();
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

    let mut cmd = bin();
    cmd.arg("--source-map").arg(&input).assert().success();

    assert!(tmp.path().join("app.min.js").exists());
    assert!(tmp.path().join("app.min.js.map").exists());
}

#[test]
fn cli_source_map_errors_on_json_only_batch() {
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("data.json");
    std::fs::write(&input, br#"{"a":1}"#).unwrap();

    let mut cmd = bin();
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

    let mut cmd = bin();
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

    let mut cmd = bin();
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

    let mut cmd = bin();
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

    let mut cmd = bin();
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

    let mut cmd = bin();
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

    let mut cmd = bin();
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

    let mut cmd = bin();
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

#[test]
fn format_unknown_value_errors() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("dot.png");
    // Tiny 1x1 PNG so the path-existence check passes.
    std::fs::write(
        &input,
        [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08,
            0x99, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x5B, 0xD0, 0x3F,
            0x80, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ],
    )
    .unwrap();

    bin()
        .arg(&input)
        .arg("--format")
        .arg("zzz")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown --format value: zzz"));
}

#[test]
fn format_cross_kind_mismatch_errors() {
    // --format mp4 with only an image input → cross-kind validation fires.
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("dot.png");
    std::fs::write(
        &input,
        [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08,
            0x99, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x5B, 0xD0, 0x3F,
            0x80, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ],
    )
    .unwrap();

    bin()
        .arg(&input)
        .arg("--format")
        .arg("mp4")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--format specifies a video/audio format, but no video/audio files were provided",
        ));
}

#[test]
fn format_mov_to_mp4_round_trip() {
    if !has_ffmpeg() {
        eprintln!("skipping: ffmpeg not present");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("clip.mov");
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

    bin()
        .arg(&input)
        .arg("--format")
        .arg("mp4")
        .assert()
        .success();

    let output = tmp.path().join("clip_squished.mp4");
    assert!(output.exists(), "expected output at {}", output.display());
    assert!(std::fs::metadata(&output).unwrap().len() > 0);
    // The .mov input must NOT have a sibling .mov output (proves --format
    // changed the container).
    assert!(!tmp.path().join("clip_squished.mov").exists());
}

/// Regression: every squish-binary invocation in this file must go through
/// the `bin()` helper, which sets the no-stats env var. If anyone reintroduces
/// a direct binary-spawn call, this test fails — preventing future test runs
/// from polluting the developer's real usage ledger.
#[test]
fn no_direct_cargo_bin_calls_in_this_file() {
    let src = include_str!("./cli_tests.rs");
    // The only allowed occurrence of this literal is the one inside `fn bin()`.
    let needle = concat!("cargo_", "bin(\"squish\")");
    let count = src.matches(needle).count();
    assert_eq!(
        count, 1,
        "all squish-binary invocations in cli_tests.rs must go through bin() \
         (got {count} direct binary-spawn occurrences; only the one inside fn bin() is allowed)"
    );
}

#[test]
fn target_size_flag_fits_image_under_budget() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sample.jpg");
    fs::copy(core_fixture("sample.jpg"), &input).unwrap();

    bin()
        .arg(&input)
        .args(["--target-size", "12k"])
        .assert()
        .success();

    let out = tmp.path().join("sample_squished.jpg");
    assert!(out.exists());
    assert!(
        fs::metadata(&out).unwrap().len() <= 12_000,
        "output exceeds 12k budget"
    );
}

#[test]
fn target_size_conflicts_with_quality() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sample.jpg");
    fs::copy(core_fixture("sample.jpg"), &input).unwrap();

    bin()
        .arg(&input)
        .args(["--target-size", "12k", "--quality", "50"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn target_size_rejects_invalid_value() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sample.jpg");
    fs::copy(core_fixture("sample.jpg"), &input).unwrap();

    bin()
        .arg(&input)
        .args(["--target-size", "abc"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("target-size"));
}

#[test]
fn target_size_code_only_batch_errors() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("app.js");
    fs::write(&input, "const x = 1;\nconsole.log(x);\n").unwrap();

    bin()
        .arg(&input)
        .args(["--target-size", "1k"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("code"));
}

#[test]
fn config_file_supplies_defaults() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("squish.toml"), "suffix = \"tiny\"\n").unwrap();
    let input = tmp.path().join("sample.png");
    fs::copy(core_fixture("sample.png"), &input).unwrap();

    bin()
        .current_dir(tmp.path())
        .arg("sample.png")
        .assert()
        .success();

    assert!(tmp.path().join("sample_tiny.png").exists());
    assert!(!tmp.path().join("sample_squished.png").exists());
}

#[test]
fn cli_flag_overrides_config_file() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("squish.toml"), "suffix = \"tiny\"\n").unwrap();
    let input = tmp.path().join("sample.png");
    fs::copy(core_fixture("sample.png"), &input).unwrap();

    bin()
        .current_dir(tmp.path())
        .args(["sample.png", "--suffix", "mini"])
        .assert()
        .success();

    assert!(tmp.path().join("sample_mini.png").exists());
    assert!(!tmp.path().join("sample_tiny.png").exists());
}

#[test]
fn config_file_found_in_parent_directory() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("squish.toml"), "suffix = \"tiny\"\n").unwrap();
    let sub = tmp.path().join("assets");
    fs::create_dir(&sub).unwrap();
    fs::copy(core_fixture("sample.png"), sub.join("sample.png")).unwrap();

    bin().current_dir(&sub).arg("sample.png").assert().success();

    assert!(sub.join("sample_tiny.png").exists());
}

#[test]
fn invalid_config_key_is_fatal_and_names_the_file() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("squish.toml"), "qualty = 80\n").unwrap();
    let input = tmp.path().join("sample.png");
    fs::copy(core_fixture("sample.png"), &input).unwrap();

    bin()
        .current_dir(tmp.path())
        .arg("sample.png")
        .assert()
        .failure()
        .stderr(predicate::str::contains("squish.toml"));
}

#[test]
fn no_config_flag_ignores_config_file() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("squish.toml"), "suffix = \"tiny\"\n").unwrap();
    let input = tmp.path().join("sample.png");
    fs::copy(core_fixture("sample.png"), &input).unwrap();

    bin()
        .current_dir(tmp.path())
        .args(["sample.png", "--no-config"])
        .assert()
        .success();

    assert!(tmp.path().join("sample_squished.png").exists());
    assert!(!tmp.path().join("sample_tiny.png").exists());
}

#[test]
fn global_config_applies_under_project_config() {
    let tmp = TempDir::new().unwrap();
    let global = tmp.path().join("global.toml");
    // Global sets quality AND suffix; project overrides only the suffix.
    fs::write(&global, "suffix = \"glob\"\nquality = 10\n").unwrap();
    fs::write(tmp.path().join("squish.toml"), "suffix = \"proj\"\n").unwrap();
    let input = tmp.path().join("sample.png");
    fs::copy(core_fixture("sample.png"), &input).unwrap();

    bin()
        .current_dir(tmp.path())
        .env("SQUISH_GLOBAL_CONFIG", &global)
        .arg("sample.png")
        .assert()
        .success();

    assert!(tmp.path().join("sample_proj.png").exists());
}

/// Spawn the squish binary directly (long-running watch process — assert_cmd's
/// blocking model doesn't fit). Mirrors bin()'s env hygiene.
fn spawn_watch(dir: &std::path::Path, extra: &[&str]) -> std::process::Child {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_squish"));
    cmd.env("SQUISH_NO_STATS", "1")
        .env("SQUISH_GLOBAL_CONFIG", "/nonexistent/squish-config.toml")
        .current_dir(dir)
        .arg(dir)
        .arg("--watch")
        .args(extra)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    cmd.spawn().expect("failed to spawn squish --watch")
}

fn wait_for(path: &std::path::Path, timeout: std::time::Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if path.exists() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}

#[test]
fn watch_squishes_newly_added_file() {
    let tmp = TempDir::new().unwrap();
    let mut child = spawn_watch(tmp.path(), &[]);

    // Give the watcher a moment to arm, then drop a file in.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    fs::copy(core_fixture("sample.png"), tmp.path().join("new.png")).unwrap();

    let appeared = wait_for(
        &tmp.path().join("new_squished.png"),
        std::time::Duration::from_secs(15),
    );

    // Loop-prevention: wait out another debounce window; the output must not
    // have been re-squished.
    std::thread::sleep(std::time::Duration::from_millis(2500));
    let resquished = tmp.path().join("new_squished_squished.png").exists()
        || tmp.path().join("new_squished_2.png").exists();

    child.kill().ok();
    child.wait().ok();

    assert!(appeared, "watch did not squish the new file within 15s");
    assert!(!resquished, "watch re-squished its own output");
}

#[test]
fn watch_runs_initial_pass_over_existing_files() {
    let tmp = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("pre.png")).unwrap();

    let mut child = spawn_watch(tmp.path(), &[]);
    let appeared = wait_for(
        &tmp.path().join("pre_squished.png"),
        std::time::Duration::from_secs(15),
    );

    child.kill().ok();
    child.wait().ok();
    assert!(
        appeared,
        "initial pass did not squish the pre-existing file"
    );
}

#[test]
fn watch_conflicts_with_dry_run() {
    bin()
        .args([".", "--watch", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn kinds_excludes_code_from_directory_run() {
    let tmp = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("a.png")).unwrap();
    fs::write(
        tmp.path().join("app.js"),
        "const number_one = 1;\nconsole.log(number_one);\n",
    )
    .unwrap();

    bin()
        .arg(tmp.path())
        .arg("-r")
        .args(["--kinds", "image,video,audio"])
        .assert()
        .success();

    assert!(tmp.path().join("a_squished.png").exists());
    assert!(
        !tmp.path().join("app.min.js").exists(),
        "code file must not be minified when --kinds excludes code"
    );
}

#[test]
fn kinds_unknown_name_is_fatal() {
    let tmp = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("a.png")).unwrap();

    bin()
        .arg(tmp.path().join("a.png"))
        .args(["--kinds", "imagery"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("unknown kind"));
}

#[test]
fn finder_action_help_lists_install_and_uninstall() {
    bin()
        .args(["finder-action", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("install"))
        .stdout(predicate::str::contains("uninstall"));
}

#[cfg(target_os = "macos")]
#[test]
fn finder_action_install_and_uninstall_roundtrip() {
    let tmp = TempDir::new().unwrap();

    bin()
        .env("SQUISH_SERVICES_DIR", tmp.path())
        .args(["finder-action", "install"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed"));

    let wflow = tmp.path().join("Squish.workflow/Contents/document.wflow");
    assert!(tmp
        .path()
        .join("Squish.workflow/Contents/Info.plist")
        .exists());
    let doc = fs::read_to_string(&wflow).unwrap();
    assert!(doc.contains("--kinds image,video,audio"));

    // Both plists must be valid property lists.
    for f in ["Info.plist", "document.wflow"] {
        let lint = std::process::Command::new("plutil")
            .arg("-lint")
            .arg(tmp.path().join("Squish.workflow/Contents").join(f))
            .output()
            .unwrap();
        assert!(
            lint.status.success(),
            "plutil -lint {f}: {}",
            String::from_utf8_lossy(&lint.stdout)
        );
    }

    bin()
        .env("SQUISH_SERVICES_DIR", tmp.path())
        .args(["finder-action", "uninstall"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed"));
    assert!(!tmp.path().join("Squish.workflow").exists());
}

#[cfg(not(target_os = "macos"))]
#[test]
fn finder_action_errors_off_macos() {
    bin()
        .args(["finder-action", "install"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("only available on macOS"));
}

#[test]
fn plain_runs_still_work_without_subcommand() {
    let tmp = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("a.png")).unwrap();
    bin().arg(tmp.path().join("a.png")).assert().success();
    assert!(tmp.path().join("a_squished.png").exists());
}

#[test]
fn finder_action_without_subcommand_is_an_error() {
    // `finder-action` requires `install` or `uninstall`; bare invocation
    // must fail rather than silently doing nothing.
    bin().arg("finder-action").assert().failure().code(2);
}
