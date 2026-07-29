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

    // Whether this tiny fixture actually shrinks under default re-encode
    // settings varies by ffmpeg build/version (the never-grow guarantee,
    // Brief 12, reports it as "already optimal" instead of "Squished" when
    // it doesn't) — the thing this test actually cares about is that the
    // run succeeds and produces a sibling output either way.
    bin().arg(&input).assert().success();

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

    // No stdout wording assertion: whether the video shows up under
    // "Squished" or "Skipped (already optimal)" depends on whether this tiny
    // fixture happens to shrink under this ffmpeg build (see the never-grow
    // guarantee, Brief 12) — the sibling files existing either way is what
    // this test actually cares about.
    bin().arg(tmp.path()).assert().success();

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
fn video_quality_auto_conflicts_with_fast() {
    if !has_ffmpeg() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sample.mp4");
    fs::copy(video_fixture("sample.mp4"), &input).unwrap();

    bin()
        .arg(&input)
        .args(["--quality", "auto", "--fast"])
        .assert()
        .failure();
}

#[test]
fn video_quality_auto_produces_valid_output() {
    if !has_ffmpeg() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("clip.mp4");
    let gen = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x240:rate=30:duration=3",
            "-c:v",
            "libx264",
            "-crf",
            "10",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&input)
        .output()
        .unwrap();
    assert!(gen.status.success(), "fixture generation failed");

    let assert = bin()
        .arg(&input)
        .args(["--codec", "h264", "--quality", "auto"])
        .assert();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    if stderr.contains("libvmaf") {
        eprintln!("skipping: this ffmpeg build lacks libvmaf");
        return;
    }
    assert.success();
    assert!(tmp.path().join("clip_squished.mp4").exists());
}

#[test]
fn video_in_directory_walk() {
    if !has_ffmpeg() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    fs::copy(video_fixture("sample.mp4"), tmp.path().join("v.mp4")).unwrap();

    // Whether this tiny fixture actually shrinks under default re-encode
    // settings varies by ffmpeg build/version (the never-grow guarantee,
    // Brief 12, reports it as "already optimal" instead of "Squished" when
    // it doesn't) — the thing this test actually cares about is that the
    // directory walk finds the video and produces a sibling output either way.
    bin().arg(tmp.path()).assert().success();

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

// ----- Shell completions -----

#[test]
fn completions_zsh_prints_compdef() {
    bin()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("#compdef squish"));
}

#[test]
fn completions_bash_prints_complete_dash_f() {
    bin()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete -F"));
}

#[test]
fn completions_fish_prints_fish_function() {
    bin()
        .args(["completions", "fish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("function __fish_squish"));
}

// ----- Man page -----

#[test]
fn man_prints_roff_header() {
    // clap_mangen/roff emit a two-line quote-escaping preamble before the
    // `.TH` title macro, so assert `.TH` is present rather than on line 1.
    bin()
        .arg("man")
        .assert()
        .success()
        .stdout(predicate::str::contains(".TH squish 1"));
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

    // The bundle icon is installed and referenced by Info.plist.
    assert!(tmp
        .path()
        .join("Squish.workflow/Contents/Resources/squish.icns")
        .exists());
    let info = fs::read_to_string(tmp.path().join("Squish.workflow/Contents/Info.plist")).unwrap();
    assert!(info.contains("CFBundleIconFile"));

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

#[test]
fn config_overwrite_true_replaces_in_place() {
    let tmp = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("a.png")).unwrap();
    fs::write(tmp.path().join("squish.toml"), "overwrite = true\n").unwrap();

    bin()
        .current_dir(tmp.path())
        .arg("a.png")
        .assert()
        .success();

    assert!(tmp.path().join("a.png").exists());
    assert!(!tmp.path().join("a_squished.png").exists());
}

#[test]
fn config_help_mentions_local_flag() {
    bin()
        .args(["config", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--local"));
}

#[test]
fn config_without_tty_errors() {
    bin()
        .arg("config")
        .write_stdin("")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("interactive terminal"));
}

#[test]
fn cli_suffix_suppresses_config_overwrite() {
    let tmp = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("a.png")).unwrap();
    fs::write(tmp.path().join("squish.toml"), "overwrite = true\n").unwrap();

    bin()
        .current_dir(tmp.path())
        .args(["a.png", "--suffix", "tiny"])
        .assert()
        .success();

    assert!(tmp.path().join("a_tiny.png").exists());
    assert!(tmp.path().join("a.png").exists());
}

#[test]
fn quality_auto_produces_smaller_visually_lossless_jpeg() {
    let tmp = TempDir::new().unwrap();
    let src = tmp.path().join("a.jpg");
    fs::copy(core_fixture("sample.jpg"), &src).unwrap();
    bin()
        .args(["a.jpg", "--quality", "auto"])
        .current_dir(tmp.path())
        .assert()
        .success();
    let out = tmp.path().join("a_squished.jpg");
    assert!(out.exists());
    let in_len = fs::metadata(&src).unwrap().len();
    let out_len = fs::metadata(&out).unwrap().len();
    assert!(
        out_len < in_len,
        "auto output {out_len} should be < source {in_len}"
    );
}

#[test]
fn quality_numeric_still_parses() {
    let tmp = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("a.png")).unwrap();
    bin()
        .args(["a.png", "--quality", "50"])
        .current_dir(tmp.path())
        .assert()
        .success();
    assert!(tmp.path().join("a_squished.png").exists());
}

#[test]
fn quality_invalid_value_errors() {
    bin()
        .args([
            core_fixture("sample.png").to_str().unwrap(),
            "--quality",
            "autoo",
        ])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("auto"));
}

#[test]
fn quality_auto_conflicts_with_target_size() {
    bin()
        .args([
            core_fixture("sample.jpg").to_str().unwrap(),
            "--quality",
            "auto",
            "--target-size",
            "1M",
        ])
        .assert()
        .failure()
        .code(2);
}

#[test]
fn doctor_reports_capabilities_and_exits_zero() {
    bin()
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("Images"))
        .stdout(predicate::str::contains("Code"))
        .stdout(predicate::str::contains("ffmpeg"));
}

#[test]
fn preset_web_converts_image_to_webp() {
    let tmp = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("a.png")).unwrap();
    bin()
        .args(["a.png", "--preset", "web"])
        .current_dir(tmp.path())
        .assert()
        .success();
    assert!(tmp.path().join("a_squished.webp").exists());
}

#[test]
fn preset_web_explicit_quality_is_honored() {
    let tmp = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("a.png")).unwrap();
    bin()
        .args(["a.png", "--preset", "web", "--quality", "90"])
        .current_dir(tmp.path())
        .assert()
        .success();
    assert!(tmp.path().join("a_squished.webp").exists());
}

#[test]
fn preset_web_on_code_only_does_not_error_on_missing_images() {
    // web requests an image output format (webp); a batch with NO images
    // (here only a JS file) must not error with "no image files". This
    // exercises RunConfig::skip_format_kind_check — without it the format
    // validator would bail before the JS is minified.
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("app.js"), "const x = 1; console.log(x);\n").unwrap();
    bin()
        .args(["app.js", "--preset", "web"])
        .current_dir(tmp.path())
        .assert()
        .success();
    assert!(tmp.path().join("app.min.js").exists());
}

#[test]
fn preset_bogus_value_errors() {
    bin()
        .args([
            core_fixture("sample.png").to_str().unwrap(),
            "--preset",
            "bogus",
        ])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("web"));
}

// ----- --json output mode -----

/// Parses stdout as JSON, asserting it is *only* JSON (no leading/trailing
/// human text mixed in) from the first byte to the last.
fn parse_json_stdout(assert: &assert_cmd::assert::Assert) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    serde_json::from_str(stdout.trim_end()).unwrap_or_else(|e| {
        panic!("stdout is not pure JSON ({e}); got:\n{stdout}");
    })
}

#[test]
fn json_reports_schema_fields_and_totals() {
    let tmp = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("a.png")).unwrap();
    fs::copy(core_fixture("sample.jpg"), tmp.path().join("b.jpg")).unwrap();

    let assert = bin().arg(tmp.path()).arg("--json").assert().success();
    let v = parse_json_stdout(&assert);

    assert_eq!(v["version"], 1);
    let files = v["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    for f in files {
        assert_eq!(f["status"], "squished");
        assert_eq!(f["kind"], "image");
        assert!(f["bytes_in"].as_u64().unwrap() > 0);
        assert!(f["bytes_out"].as_u64().unwrap() > 0);
        assert!(f["output"].is_string());
        assert!(f["format"].is_string());
    }

    let bytes_in: u64 = files.iter().map(|f| f["bytes_in"].as_u64().unwrap()).sum();
    let bytes_out: u64 = files.iter().map(|f| f["bytes_out"].as_u64().unwrap()).sum();
    assert_eq!(v["totals"]["files"], 2);
    assert_eq!(v["totals"]["bytes_in"], bytes_in);
    assert_eq!(v["totals"]["bytes_out"], bytes_out);
    assert_eq!(v["totals"]["by_kind"]["image"]["files"], 2);
    assert!(v["errors"].as_array().unwrap().is_empty());
}

#[test]
fn json_dry_run_lists_planned_files_without_writing() {
    let tmp = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("a.png")).unwrap();

    let assert = bin()
        .arg(tmp.path())
        .args(["--dry-run", "--json"])
        .assert()
        .success();
    let v = parse_json_stdout(&assert);

    let files = v["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["status"], "skipped");
    assert_eq!(files[0]["kind"], "image");
    assert!(files[0]["output"].is_null());
    assert!(!tmp.path().join("a_squished.png").exists());
}

#[test]
fn json_batch_with_one_failing_file_reports_error_and_exit_code() {
    let tmp = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("ok.png")).unwrap();
    fs::write(tmp.path().join("corrupt.png"), b"not actually a PNG").unwrap();

    let assert = bin().arg(tmp.path()).arg("--json").assert().code(1);
    let v = parse_json_stdout(&assert);

    assert_eq!(v["files"].as_array().unwrap().len(), 1);
    let errors = v["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert!(errors[0]["input"].as_str().unwrap().contains("corrupt.png"));
    assert!(!errors[0]["message"].as_str().unwrap().is_empty());
}

#[test]
fn json_conflicts_with_verbose_quiet_watch_stats() {
    for flag in ["--verbose", "--quiet", "--watch", "--stats"] {
        bin()
            .arg(core_fixture("sample.png"))
            .args(["--json", flag])
            .assert()
            .failure()
            .stderr(predicate::str::contains("cannot be used with"));
    }
}

// ----- --exclude / --gitignore / --no-default-excludes -----

#[test]
fn exclude_glob_skips_matching_files_in_a_directory_walk() {
    let tmp = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("a.png")).unwrap();
    fs::create_dir(tmp.path().join("vendor")).unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("vendor/b.png")).unwrap();

    bin()
        .arg(tmp.path())
        .args(["-r", "--exclude", "vendor/**"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Squished 1 files"));

    assert!(tmp.path().join("a_squished.png").exists());
    assert!(!tmp.path().join("vendor/b_squished.png").exists());
}

#[test]
fn explicit_file_argument_is_never_excluded() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("a.png");
    fs::copy(core_fixture("sample.png"), &input).unwrap();

    bin()
        .arg(&input)
        .args(["--exclude", "*.png"])
        .assert()
        .success();

    assert!(tmp.path().join("a_squished.png").exists());
}

#[test]
fn default_excludes_prune_git_node_modules_target() {
    let tmp = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("a.png")).unwrap();
    for dir in [".git", "node_modules", "target"] {
        fs::create_dir(tmp.path().join(dir)).unwrap();
        fs::copy(
            core_fixture("sample.png"),
            tmp.path().join(dir).join("b.png"),
        )
        .unwrap();
    }

    bin()
        .arg(tmp.path())
        .arg("-r")
        .assert()
        .success()
        .stdout(predicate::str::contains("Squished 1 files"));
}

#[test]
fn no_default_excludes_flag_walks_into_node_modules() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join("node_modules")).unwrap();
    fs::copy(
        core_fixture("sample.png"),
        tmp.path().join("node_modules/a.png"),
    )
    .unwrap();

    bin()
        .arg(tmp.path())
        .args(["-r", "--no-default-excludes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Squished 1 files"));

    assert!(tmp.path().join("node_modules/a_squished.png").exists());
}

#[test]
fn gitignore_flag_respects_dotgitignore_file() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".gitignore"), "ignored.png\n").unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("a.png")).unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("ignored.png")).unwrap();

    bin()
        .arg(tmp.path())
        .args(["-r", "--gitignore"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Squished 1 files"));

    assert!(tmp.path().join("a_squished.png").exists());
    assert!(!tmp.path().join("ignored_squished.png").exists());
}

#[test]
fn exclude_config_key_supplies_default() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("squish.toml"),
        "exclude = [\"vendor/**\"]\n",
    )
    .unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("a.png")).unwrap();
    fs::create_dir(tmp.path().join("vendor")).unwrap();
    fs::copy(core_fixture("sample.png"), tmp.path().join("vendor/b.png")).unwrap();

    bin()
        .current_dir(tmp.path())
        .args([".", "-r"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Squished 1 files"));
}

// ----- --keep-metadata -----

/// Cheap byte-level check for the JPEG APP1 EXIF marker signature. The
/// thorough, structurally-verified checks (parsed tags, orientation) live in
/// squish-core's own `tests/metadata.rs`; this just proves the CLI flag
/// actually threads through to the core option end-to-end.
fn has_exif_marker(bytes: &[u8]) -> bool {
    bytes.windows(6).any(|w| w == b"Exif\0\0")
}

#[test]
fn keep_metadata_flag_preserves_exif_default_strips_it() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("a.jpg");
    fs::copy(core_fixture("exif_sample.jpg"), &input).unwrap();

    bin().arg(&input).assert().success();
    let stripped = fs::read(tmp.path().join("a_squished.jpg")).unwrap();
    assert!(
        !has_exif_marker(&stripped),
        "EXIF should be stripped by default"
    );

    let input2 = tmp.path().join("b.jpg");
    fs::copy(core_fixture("exif_sample.jpg"), &input2).unwrap();
    bin().arg(&input2).arg("--keep-metadata").assert().success();
    let kept = fs::read(tmp.path().join("b_squished.jpg")).unwrap();
    assert!(
        has_exif_marker(&kept),
        "--keep-metadata should preserve EXIF"
    );
}

// ----- Never-grow guarantee -----

#[test]
fn already_optimal_file_is_skipped_byte_identical_on_second_run() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("a.png");
    fs::copy(core_fixture("sample.png"), &input).unwrap();

    // First pass: oxipng (--lossless, no quantization search) genuinely
    // compresses it.
    bin()
        .arg(&input)
        .arg("--lossless")
        .assert()
        .success()
        .stdout(predicate::str::contains("Squished 1 files"));
    let once = tmp.path().join("a_squished.png");
    assert!(once.exists());
    let bytes_after_first_pass = fs::read(&once).unwrap();

    // Second pass, re-squishing the already-optimized output: oxipng at max
    // compression is deterministic, so a second lossless pass over its own
    // output finds nothing further to gain — must report "skipped", not a
    // (non-)saving, and the result must be byte-identical, not merely the
    // same size.
    bin()
        .arg(&once)
        .arg("--lossless")
        .assert()
        .success()
        .code(0)
        .stdout(predicate::str::contains("Skipped 1 (already optimal"));
    let twice = tmp.path().join("a_squished_squished.png");
    assert_eq!(fs::read(&twice).unwrap(), bytes_after_first_pass);
}

#[test]
fn format_conversion_is_allowed_to_grow() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("a.png");
    fs::copy(core_fixture("sample.png"), &input).unwrap();
    bin().arg(&input).arg("--lossless").assert().success();
    let optimized = tmp.path().join("a_squished.png");

    // Converting an already-optimal PNG to TIFF (no compression) reliably
    // grows it — an explicit --format conversion must be allowed to do
    // that, not silently discarded by the never-grow guard.
    bin()
        .arg(&optimized)
        .args(["--format", "tiff"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Squished 1 files"));

    let converted = tmp.path().join("a_squished_squished.tiff");
    let out_size = fs::metadata(&converted).unwrap().len();
    let in_size = fs::metadata(&optimized).unwrap().len();
    assert!(
        out_size > in_size,
        "expected TIFF conversion to grow the file: {in_size} -> {out_size}"
    );
}

#[test]
fn already_optimal_reports_skipped_status_in_json() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("a.png");
    fs::copy(core_fixture("sample.png"), &input).unwrap();
    bin().arg(&input).arg("--lossless").assert().success();
    let optimized = tmp.path().join("a_squished.png");

    let assert = bin()
        .arg(&optimized)
        .args(["--lossless", "--json"])
        .assert()
        .success();
    let v = parse_json_stdout(&assert);

    let files = v["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["status"], "skipped");
    assert_eq!(files[0]["kind"], "image");
    assert_eq!(files[0]["bytes_in"], files[0]["bytes_out"]);
    // Already-optimal skips don't count toward "squished" totals.
    assert_eq!(v["totals"]["files"], 0);
}

#[test]
fn overwrite_mode_already_optimal_restores_original_bytes_safely() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("a.png");
    fs::copy(core_fixture("sample.png"), &input).unwrap();

    bin()
        .arg(&input)
        .args(["--lossless", "--overwrite"])
        .assert()
        .success();
    let after_first = fs::read(&input).unwrap();

    // Second in-place pass: the encoder overwrites `input` directly (no
    // temp+rename for images), so if the never-grow guard didn't cache the
    // original bytes *before* encoding, there would be nothing left to
    // restore. Verify the file is still intact and byte-identical.
    bin()
        .arg(&input)
        .args(["--lossless", "--overwrite"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Skipped 1 (already optimal"));
    assert_eq!(fs::read(&input).unwrap(), after_first);
}

#[test]
fn target_size_larger_than_input_does_not_grow_image() {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sample.jpg");
    fs::copy(core_fixture("sample.jpg"), &input).unwrap();
    let input_bytes = fs::read(&input).unwrap();

    // The fixture is ~43k; a 200k budget is already satisfied, so re-encoding
    // toward that budget must never be allowed to grow the file — --target-size
    // is not a "legitimate conversion" the way --format/--codec/resize are.
    bin()
        .arg(&input)
        .args(["--target-size", "200k"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Skipped 1 (already optimal"));

    let out = tmp.path().join("sample_squished.jpg");
    assert_eq!(fs::read(&out).unwrap(), input_bytes);
}

#[test]
fn target_size_larger_than_input_does_not_grow_video() {
    if !has_ffmpeg() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sample.mp4");
    fs::copy(video_fixture("sample.mp4"), &input).unwrap();
    let input_bytes = fs::read(&input).unwrap();

    // Fixture is ~10k; a 5M budget is already satisfied.
    bin()
        .arg(&input)
        .args(["--target-size", "5M"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Skipped 1 (already optimal"));

    let out = tmp.path().join("sample_squished.mp4");
    assert_eq!(fs::read(&out).unwrap(), input_bytes);
}

#[test]
fn target_size_larger_than_input_does_not_grow_audio() {
    if !has_ffmpeg() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("sine.mp3");
    make_sine(&input, &["-c:a", "libmp3lame"]);
    let input_bytes = fs::read(&input).unwrap();

    // A 1-second sine encodes to well under 100k; the budget is already met.
    bin()
        .arg(&input)
        .args(["--target-size", "100k"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Skipped 1 (already optimal"));

    let out = tmp.path().join("sine_squished.mp3");
    assert_eq!(fs::read(&out).unwrap(), input_bytes);
}
// ----- Crop / gravity integration tests -----

#[test]
fn crop_aspect_square() {
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("a.png");
    fs::copy(core_fixture("sample.png"), &input).unwrap();
    bin()
        .arg(&input)
        .arg("--crop")
        .arg("1:1")
        .assert()
        .success();
    let out = tmp.path().join("a_squished.png");
    assert_eq!(image::image_dimensions(&out).unwrap(), (480, 480));
}

#[test]
fn crop_exact_rect_with_gravity_flag_combo() {
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("a.png");
    fs::copy(core_fixture("sample.png"), &input).unwrap();
    bin()
        .arg(&input)
        .arg("--crop")
        .arg("16:9")
        .arg("--gravity")
        .arg("north")
        .assert()
        .success();
    let out = tmp.path().join("a_squished.png");
    assert_eq!(image::image_dimensions(&out).unwrap(), (640, 360));
}

#[test]
fn gravity_without_crop_is_a_usage_error() {
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("a.png");
    fs::copy(core_fixture("sample.png"), &input).unwrap();
    bin()
        .arg(&input)
        .arg("--gravity")
        .arg("north")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--crop"));
}

#[test]
fn malformed_crop_spec_is_a_usage_error() {
    let tmp = tempfile::TempDir::new().unwrap();
    let input = tmp.path().join("a.png");
    fs::copy(core_fixture("sample.png"), &input).unwrap();
    bin()
        .arg(&input)
        .arg("--crop")
        .arg("nonsense")
        .assert()
        .failure()
        .stderr(predicate::str::contains("expected an aspect ratio"));
}

#[test]
fn out_of_bounds_crop_is_per_file_error_and_batch_continues() {
    let tmp = tempfile::TempDir::new().unwrap();
    // A second, valid file alongside the bad crop, so a batch run actually
    // proves the run continues past the per-file error rather than just
    // failing the only file present. `sample.png` can't play that role here:
    // the offset (+9999) is out of bounds for any image of realistic width,
    // so a second copy of `sample.png` would fail identically. `sample.svg`
    // is crop-immune (crop is raster-only; SVG compresses unchanged with a
    // warning), so it reliably succeeds regardless of the batch's --crop flag.
    fs::copy(core_fixture("sample.svg"), tmp.path().join("ok.svg")).unwrap();
    let input = tmp.path().join("a.png");
    fs::copy(core_fixture("sample.png"), &input).unwrap();
    bin()
        .arg(tmp.path())
        .arg("--crop")
        .arg("100x100+9999+0")
        .assert()
        .code(1);
    assert!(!tmp.path().join("a_squished.png").exists());
    assert!(tmp.path().join("ok_squished.svg").exists());
}

#[test]
fn select_with_two_images_errors_before_any_ui() {
    let dir = TempDir::new().unwrap();
    fs::copy(core_fixture("sample.png"), dir.path().join("a.png")).unwrap();
    fs::copy(core_fixture("sample.jpg"), dir.path().join("b.jpg")).unwrap();

    bin()
        .env("SQUISH_SELECT_NO_OPEN", "1")
        .arg(dir.path())
        .arg("--select")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("needs exactly one image"))
        .stderr(predicate::str::contains("matched 2 files"));
}

#[test]
fn select_rejects_svg() {
    bin()
        .env("SQUISH_SELECT_NO_OPEN", "1")
        .arg(core_fixture("sample.svg"))
        .arg("--select")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("vector format"));
}

#[test]
fn select_rejects_animated_webp() {
    bin()
        .env("SQUISH_SELECT_NO_OPEN", "1")
        .arg(core_fixture("anim.webp"))
        .arg("--select")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("animated WebP"));
}

#[test]
fn select_rejects_a_non_image() {
    let dir = TempDir::new().unwrap();
    let js = dir.path().join("app.js");
    fs::write(&js, "const a = 1;\n").unwrap();

    bin()
        .env("SQUISH_SELECT_NO_OPEN", "1")
        .arg(&js)
        .arg("--select")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("needs an image"));
}

#[test]
fn select_requires_a_tty_without_the_env_seam() {
    bin()
        .arg(core_fixture("sample.png"))
        .arg("--select")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("interactive terminal"));
}

#[test]
fn select_conflicts_with_json() {
    bin()
        .env("SQUISH_SELECT_NO_OPEN", "1")
        .arg(core_fixture("sample.png"))
        .args(["--select", "--json"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn config_rejects_a_select_key() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("squish.toml"), "select = true\n").unwrap();
    fs::copy(core_fixture("sample.png"), dir.path().join("a.png")).unwrap();

    bin()
        .current_dir(dir.path())
        .arg("a.png")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("select"));
}
