//! Compression-ratio regression guard.
//!
//! Runs the real `squish` binary against each image fixture with default
//! settings and asserts the output shrinks by at least a known floor. Catches
//! a dependency bump that quietly *worsens* output — the failure mode behind
//! the v0.3.3 usvg->oxvg SVG regression.
//!
//! Thresholds are set ~10 percentage points looser than the measured ratio on
//! the current toolchain so ordinary encoder-version noise doesn't flake CI.

use assert_cmd::Command;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/
    p.push("squish-core/tests/fixtures");
    p.push(name);
    p
}

/// Spawns the squish binary hermetically: no usage-ledger writes, no reading
/// the developer's real global config. Mirrors `bin()` in cli_tests.rs; kept
/// local to this file because ratio_guard.rs is its own test binary.
fn bin() -> Command {
    let mut cmd = Command::cargo_bin("squish").unwrap();
    cmd.env("SQUISH_NO_STATS", "1");
    cmd.env("SQUISH_GLOBAL_CONFIG", "/nonexistent/squish-config.toml");
    cmd
}

fn has_gifsicle() -> bool {
    std::process::Command::new("gifsicle")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Runs `squish <input>` with no flags and returns (input_bytes, output_bytes).
/// The output filename is whatever the CLI picks (format conversions change
/// the extension, e.g. sample.tiff -> sample_squished.jpg), so we locate it
/// by listing the temp dir rather than assuming a name.
fn run_default(fixture_name: &str) -> (u64, u64) {
    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join(fixture_name);
    fs::copy(fixture(fixture_name), &input).unwrap();

    bin().arg(&input).assert().success();

    let input_bytes = fs::metadata(&input).unwrap().len();
    let output_path = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p != &input)
        .unwrap_or_else(|| panic!("no squished output produced for {fixture_name}"));
    let output_bytes = fs::metadata(&output_path).unwrap().len();
    (input_bytes, output_bytes)
}

/// (fixture, minimum required reduction fraction, needs gifsicle on PATH)
///
/// Measured reductions on the reference toolchain (2026-07):
/// png 77.4%, jpg 33.9%, webp 43.7%, avif 65.0%, heic 48.1%, svg 54.2%,
/// tiff 96.9% (converts to JPEG by default), gif 0.04%, anim.webp 0%.
const CASES: &[(&str, f64, bool)] = &[
    ("sample.png", 0.65, false),
    ("sample.jpg", 0.20, false),
    ("sample.webp", 0.30, false),
    ("sample.avif", 0.50, false),
    ("sample.heic", 0.35, false),
    ("sample.svg", 0.40, false),
    ("sample.tiff", 0.85, false),
    // Already near-optimal fixtures: no real headroom to compress further,
    // but must never grow.
    ("sample.gif", 0.0, true),
    ("anim.webp", 0.0, false),
];

#[test]
fn ratio_guard_meets_thresholds() {
    for &(name, min_reduction, needs_gifsicle) in CASES {
        if needs_gifsicle && !has_gifsicle() {
            eprintln!("skipping {name}: gifsicle not found");
            continue;
        }
        let (input_bytes, output_bytes) = run_default(name);

        // Universal floor: never grow, for every fixture (including SVG,
        // which also has its own internal guard in squish-core).
        assert!(
            output_bytes <= input_bytes,
            "{name}: output grew ({input_bytes} -> {output_bytes})"
        );

        let reduction = 1.0 - (output_bytes as f64 / input_bytes as f64);
        assert!(
            reduction >= min_reduction,
            "{name}: reduction {:.1}% below required {:.1}% ({input_bytes} -> {output_bytes})",
            reduction * 100.0,
            min_reduction * 100.0
        );
    }
}

/// Animated GIF is excluded from `ratio_guard_meets_thresholds`'s strict
/// floor: gifsicle's re-encode of this fixture currently grows it by ~0.03%
/// (113426 -> 113457 bytes on gifsicle 1.96), which the existing
/// `animated_gif_preserves_frames` round-trip test already tolerates by not
/// asserting a size decrease. Bound the growth instead of ignoring it
/// entirely, so a much larger regression still fails CI. See Brief 12
/// (never-grow guarantee) in IMPLEMENTATION-BRIEFS.md for the real fix.
#[test]
fn animated_gif_growth_is_bounded() {
    if !has_gifsicle() {
        eprintln!("skipping: gifsicle not found");
        return;
    }
    let (input_bytes, output_bytes) = run_default("sample_animated.gif");
    let max_allowed = input_bytes + (input_bytes / 50); // +2% ceiling
    assert!(
        output_bytes <= max_allowed,
        "animated GIF grew more than the 2% tolerance: {input_bytes} -> {output_bytes}"
    );
}
