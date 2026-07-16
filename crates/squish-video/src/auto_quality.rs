//! Perceptual quality measurement for video `--quality auto`, using
//! segment-sampled VMAF scoring via ffmpeg's libvmaf filter. See
//! docs/superpowers/specs/2026-07-16-quality-auto-video-design.md.

use crate::options::{VideoCodec, VideoOptions};
use crate::VideoError;
use std::path::Path;

/// VMAF score at/above which video output is considered visually lossless.
pub(crate) const VISUALLY_LOSSLESS_THRESHOLD: f64 = 95.0;

/// One sampled segment of the source video: start offset and length, both in
/// seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Segment {
    pub start_secs: f64,
    pub len_secs: f64,
}

/// Duration at/under which the whole clip is scored as a single segment
/// instead of sampling three.
const SINGLE_SEGMENT_THRESHOLD_SECS: f64 = 8.0;
/// Length of each sampled segment, in seconds, for clips longer than the
/// single-segment threshold.
const SEGMENT_LEN_SECS: f64 = 2.0;
/// Relative positions (fraction of duration) sampled for longer clips —
/// avoids exact start/end, which can have black frames or atypical pacing.
const SEGMENT_POSITIONS: [f64; 3] = [0.10, 0.50, 0.85];

/// Compute the segments to sample for VMAF scoring. Clips at or under 8s
/// are scored whole (avoids seek-boundary edge cases on inputs shorter than
/// the sampling window would need anyway); longer clips get three 2s
/// segments at 10%/50%/85% of duration, each clamped so it never runs past
/// `duration_secs - SEGMENT_LEN_SECS`.
pub(crate) fn sample_segments(duration_secs: f64) -> Vec<Segment> {
    if duration_secs <= SINGLE_SEGMENT_THRESHOLD_SECS {
        return vec![Segment {
            start_secs: 0.0,
            len_secs: duration_secs.max(0.0),
        }];
    }

    let max_start = (duration_secs - SEGMENT_LEN_SECS).max(0.0);
    SEGMENT_POSITIONS
        .iter()
        .map(|&pos| Segment {
            start_secs: (duration_secs * pos).min(max_start),
            len_secs: SEGMENT_LEN_SECS,
        })
        .collect()
}

/// Parse the pooled mean VMAF score out of a `libvmaf` JSON log (written via
/// `log_fmt=json:log_path=<path>`). Returns `None` for malformed JSON or a
/// log missing the expected `pooled_metrics.vmaf.mean` field, so callers can
/// treat scoring failure as "does not pass" rather than panicking.
pub(crate) fn parse_vmaf_json(json_text: &str) -> Option<f64> {
    let v: serde_json::Value = serde_json::from_str(json_text).ok()?;
    v.get("pooled_metrics")?.get("vmaf")?.get("mean")?.as_f64()
}

/// Check that the installed ffmpeg's `-filters` output lists `libvmaf` — a
/// compile-time filter, not a standalone executable, so this can't reuse the
/// simpler "run `<tool> -version`" shape `squish_media::check_ffmpeg` uses.
pub(crate) fn check_libvmaf() -> Result<(), VideoError> {
    let output = std::process::Command::new("ffmpeg")
        .arg("-filters")
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                VideoError::MissingDependency {
                    name: "ffmpeg".into(),
                    install_hint: "brew install ffmpeg (macOS) or apt install ffmpeg (Linux)"
                        .into(),
                }
            } else {
                VideoError::Io(e)
            }
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && stdout.contains("libvmaf") {
        Ok(())
    } else {
        Err(VideoError::MissingDependency {
            name: "libvmaf".into(),
            install_hint: "reinstall/upgrade ffmpeg with libvmaf support (Homebrew's ffmpeg \
                           formula includes it by default: brew reinstall ffmpeg)"
                .into(),
        })
    }
}

/// Extract a frame-accurate, near-lossless reference clip covering
/// `segment` of `input`, written to `out_path` (container inferred from its
/// extension — callers use `.mkv`, which accepts any of the four codecs
/// without the container-compatibility quirks `.mp4` has for some of them).
/// Re-decodes rather than stream-copies so arbitrary seek points work; audio
/// is dropped since only video quality is being judged.
pub(crate) fn extract_reference_segment(
    input: &Path,
    segment: Segment,
    out_path: &Path,
) -> Result<(), VideoError> {
    let args: Vec<std::ffi::OsString> = vec![
        "-ss".into(),
        format!("{:.3}", segment.start_secs).into(),
        "-t".into(),
        format!("{:.3}", segment.len_secs).into(),
        "-an".into(),
        "-c:v".into(),
        "libx264".into(),
        "-crf".into(),
        "0".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
    ];
    squish_media::run_ffmpeg(input, out_path, &args)
}

/// Encode `ref_segment_path` (a short reference clip) at `quality` (the same
/// 1..=100 dial `--quality` uses) with `codec`, via the existing single-pass
/// CRF encode path.
pub(crate) fn encode_segment_at_quality(
    ref_segment_path: &Path,
    out_path: &Path,
    quality: u8,
    codec: VideoCodec,
) -> Result<(), VideoError> {
    let opts = VideoOptions {
        quality: Some(quality),
        codec: Some(codec),
        ..Default::default()
    };
    crate::ffmpeg::run_ffmpeg(ref_segment_path, out_path, &opts, false, None)
}

/// Run ffmpeg's `libvmaf` filter comparing `candidate` against `reference`,
/// writing a JSON log under `tmp_dir` and returning the parsed pooled mean
/// score. `Ok(None)` means ffmpeg succeeded but the log couldn't be parsed
/// (treated as "does not pass" by callers, never a panic).
///
/// Not built on `squish_media::run_ffmpeg`: that helper assumes one `-i
/// <input>` and one trailing output path, but VMAF scoring needs two `-i`
/// inputs and writes no real output (`-f null -`).
pub(crate) fn vmaf_score(
    candidate: &Path,
    reference: &Path,
    tmp_dir: &Path,
) -> Result<Option<f64>, VideoError> {
    let log_path = tmp_dir.join("vmaf.json");
    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.arg("-y")
        .arg("-i")
        .arg(candidate)
        .arg("-i")
        .arg(reference)
        .arg("-lavfi")
        .arg(format!(
            "libvmaf=log_fmt=json:log_path={}",
            log_path.display()
        ))
        .arg("-f")
        .arg("null")
        .arg("-");

    let output = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            VideoError::MissingDependency {
                name: "ffmpeg".into(),
                install_hint: "brew install ffmpeg (macOS) or apt install ffmpeg (Linux)".into(),
            }
        } else {
            VideoError::Io(e)
        }
    })?;

    if !output.status.success() {
        return Err(VideoError::FfmpegFailed {
            path: candidate.to_path_buf(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    let text = std::fs::read_to_string(&log_path)?;
    Ok(parse_vmaf_json(&text))
}

/// Binary-search the 1..=100 quality dial for the lowest quality whose
/// `score_at(quality)` is `>= threshold`. Mirrors
/// `squish_core::compress_to_visually_lossless`'s search shape exactly, with
/// `score_at` standing in for that function's "encode + measure" step —
/// callers wire it to real segment encode+VMAF (Task 7); tests here use a
/// synthetic closure so search correctness doesn't require ffmpeg.
/// `score_at` returning `None` (scoring failed) is treated the same as a
/// too-low score: keep searching higher.
///
/// Returns `(quality, true)` for the lowest passing quality found, or
/// `(100, false)` if nothing in the range passed.
pub(crate) fn binary_search_quality<F>(threshold: f64, mut score_at: F) -> (u8, bool)
where
    F: FnMut(u8) -> Option<f64>,
{
    let mut lo: u8 = 1;
    let mut hi: u8 = 100;
    let mut best_passing: Option<u8> = None;

    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        match score_at(mid) {
            Some(score) if score >= threshold => {
                best_passing = Some(mid);
                if mid == 1 {
                    break;
                }
                hi = mid - 1;
            }
            _ => {
                lo = mid + 1;
            }
        }
    }

    match best_passing {
        Some(q) => (q, true),
        None => (100, false),
    }
}

/// Find the lowest quality (1..=100 dial) whose full-clip output stays
/// visually lossless (VMAF >= 95, minimum across sampled segments). Extracts
/// each sampled segment's reference clip once (cached for the whole search),
/// then binary-searches, encoding + scoring every reference segment per
/// candidate quality.
///
/// Returns `(quality, true)` if some quality passed the threshold, or
/// `(100, false)` if none did (caller should re-encode the full clip at
/// quality 100 and surface a warning — mirrors the image auto-quality
/// fallback).
pub(crate) fn find_visually_lossless_quality(
    input: &Path,
    codec: VideoCodec,
) -> Result<(u8, bool), VideoError> {
    let duration =
        crate::ffmpeg::ffprobe_duration_secs(input)?.ok_or_else(|| VideoError::InvalidOption {
            reason: format!(
                "--quality auto requires a known duration for {}",
                input.display()
            ),
        })?;

    let segments = sample_segments(duration);
    let tmp = tempfile::tempdir()?;

    let mut ref_paths = Vec::with_capacity(segments.len());
    for (i, seg) in segments.iter().enumerate() {
        let ref_path = tmp.path().join(format!("ref-{i}.mkv"));
        extract_reference_segment(input, *seg, &ref_path)?;
        ref_paths.push(ref_path);
    }

    let candidate_path = tmp.path().join("candidate.mkv");
    let result = binary_search_quality(VISUALLY_LOSSLESS_THRESHOLD, |quality| {
        let mut min_score: Option<f64> = None;
        for ref_path in &ref_paths {
            if encode_segment_at_quality(ref_path, &candidate_path, quality, codec).is_err() {
                return None;
            }
            let score = match vmaf_score(&candidate_path, ref_path, tmp.path()) {
                Ok(Some(s)) => s,
                _ => return None,
            };
            min_score = Some(min_score.map_or(score, |m: f64| m.min(score)));
        }
        min_score
    });

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_ffmpeg() -> bool {
        std::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn short_clip_yields_one_whole_segment() {
        let segs = sample_segments(2.0);
        assert_eq!(segs.len(), 1);
        assert_eq!(
            segs[0],
            Segment {
                start_secs: 0.0,
                len_secs: 2.0
            }
        );
    }

    #[test]
    fn exactly_at_threshold_yields_one_whole_segment() {
        let segs = sample_segments(8.0);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].len_secs, 8.0);
    }

    #[test]
    fn long_clip_yields_three_positioned_segments() {
        let segs = sample_segments(60.0);
        assert_eq!(segs.len(), 3);
        assert!((segs[0].start_secs - 6.0).abs() < 0.001);
        assert!((segs[1].start_secs - 30.0).abs() < 0.001);
        assert!((segs[2].start_secs - 51.0).abs() < 0.001);
        for s in &segs {
            assert_eq!(s.len_secs, 2.0);
            assert!(s.start_secs + s.len_secs <= 60.0);
        }
    }

    #[test]
    fn last_segment_clamps_when_near_the_end() {
        // duration=8.01: the 85% position (6.8085s) would run past
        // duration - 2.0 = 6.01s, so it must clamp to 6.01.
        let segs = sample_segments(8.01);
        assert_eq!(segs.len(), 3);
        assert!((segs[2].start_secs - 6.01).abs() < 0.001);
        assert!(segs[2].start_secs + segs[2].len_secs <= 8.01 + 0.001);
    }

    #[test]
    fn parses_pooled_mean_from_real_shaped_log() {
        let json = r#"{
            "version": "3.0.0",
            "pooled_metrics": {
                "vmaf": {
                    "min": 96.363678,
                    "max": 97.434367,
                    "mean": 97.104564,
                    "harmonic_mean": 97.103996
                }
            }
        }"#;
        let score = parse_vmaf_json(json).expect("expected a score");
        assert!((score - 97.104564).abs() < 0.0001);
    }

    #[test]
    fn malformed_json_returns_none() {
        assert_eq!(parse_vmaf_json("not json at all"), None);
    }

    #[test]
    fn missing_pooled_metrics_returns_none() {
        assert_eq!(parse_vmaf_json(r#"{"version": "3.0.0"}"#), None);
    }

    #[test]
    fn missing_vmaf_mean_field_returns_none() {
        let json = r#"{"pooled_metrics": {"vmaf": {"min": 90.0}}}"#;
        assert_eq!(parse_vmaf_json(json), None);
    }

    #[test]
    fn check_libvmaf_reports_missing_ffmpeg_when_binary_absent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let original_path = std::env::var_os("PATH");
        std::env::set_var("PATH", tmp.path());

        let result = check_libvmaf();

        match original_path {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }

        assert!(matches!(
            result,
            Err(VideoError::MissingDependency { ref name, .. }) if name == "ffmpeg"
        ));
    }

    #[test]
    fn check_libvmaf_errors_when_filter_absent_from_ffmpeg_output() {
        let tmp = tempfile::TempDir::new().unwrap();
        let fake_ffmpeg = tmp.path().join("ffmpeg");
        std::fs::write(
            &fake_ffmpeg,
            "#!/bin/sh\necho ' .. someotherfilter    Some other filter.'\nexit 0\n",
        )
        .unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&fake_ffmpeg).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&fake_ffmpeg, perms).unwrap();
        }

        let original_path = std::env::var_os("PATH");
        std::env::set_var("PATH", tmp.path());

        let result = check_libvmaf();

        match original_path {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }

        assert!(matches!(
            result,
            Err(VideoError::MissingDependency { ref name, .. }) if name == "libvmaf"
        ));
    }

    #[test]
    fn check_libvmaf_ok_when_present() {
        if std::process::Command::new("ffmpeg")
            .arg("-filters")
            .output()
            .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains("libvmaf"))
            .unwrap_or(false)
        {
            assert!(check_libvmaf().is_ok());
        } else {
            eprintln!("skipping: this ffmpeg build lacks libvmaf");
        }
    }

    #[test]
    fn extract_reference_segment_produces_a_clip_of_the_requested_length() {
        if !has_ffmpeg() {
            eprintln!("skipping: ffmpeg not present");
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let input = tmp.path().join("source.mp4");
        let gen = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=30:duration=3",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&input)
            .output()
            .unwrap();
        assert!(gen.status.success(), "fixture generation failed");

        let out = tmp.path().join("ref-0.mkv");
        extract_reference_segment(
            &input,
            Segment {
                start_secs: 0.5,
                len_secs: 1.0,
            },
            &out,
        )
        .unwrap();

        assert!(out.exists());
        let dur = crate::ffmpeg::ffprobe_duration_secs(&out).unwrap().unwrap();
        assert!((dur - 1.0).abs() < 0.2, "expected ~1.0s, got {dur}");
    }

    #[test]
    fn encode_segment_at_quality_produces_output() {
        if !has_ffmpeg() {
            eprintln!("skipping: ffmpeg not present");
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let reference = tmp.path().join("ref.mkv");
        let gen = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=30:duration=1",
                "-c:v",
                "libx264",
                "-crf",
                "0",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&reference)
            .output()
            .unwrap();
        assert!(gen.status.success(), "reference generation failed");

        let out = tmp.path().join("candidate.mkv");
        encode_segment_at_quality(&reference, &out, 50, VideoCodec::H264).unwrap();

        assert!(out.exists());
        assert!(std::fs::metadata(&out).unwrap().len() > 0);
    }

    #[test]
    fn vmaf_score_of_identical_content_is_near_100() {
        if !has_ffmpeg() {
            eprintln!("skipping: ffmpeg not present");
            return;
        }
        if !check_libvmaf().is_ok() {
            eprintln!("skipping: this ffmpeg build lacks libvmaf");
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let reference = tmp.path().join("ref.mkv");
        let gen = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x240:rate=30:duration=1",
                "-c:v",
                "libx264",
                "-crf",
                "0",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&reference)
            .output()
            .unwrap();
        assert!(gen.status.success());

        // Encode at very high quality — should score near-perfectly against
        // the (near-lossless) reference.
        let candidate = tmp.path().join("candidate.mkv");
        encode_segment_at_quality(&reference, &candidate, 100, VideoCodec::H264).unwrap();

        let score = vmaf_score(&candidate, &reference, tmp.path())
            .unwrap()
            .expect("expected a score");
        assert!(score > 90.0, "expected near-lossless score, got {score}");
    }

    #[test]
    fn vmaf_score_drops_with_lower_quality() {
        if !has_ffmpeg() {
            eprintln!("skipping: ffmpeg not present");
            return;
        }
        if !check_libvmaf().is_ok() {
            eprintln!("skipping: this ffmpeg build lacks libvmaf");
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let reference = tmp.path().join("ref.mkv");
        let gen = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x240:rate=30:duration=1",
                "-c:v",
                "libx264",
                "-crf",
                "0",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&reference)
            .output()
            .unwrap();
        assert!(gen.status.success());

        let high = tmp.path().join("high.mkv");
        encode_segment_at_quality(&reference, &high, 100, VideoCodec::H264).unwrap();
        let low = tmp.path().join("low.mkv");
        encode_segment_at_quality(&reference, &low, 1, VideoCodec::H264).unwrap();

        let high_score = vmaf_score(&high, &reference, tmp.path()).unwrap().unwrap();
        let low_score = vmaf_score(&low, &reference, tmp.path()).unwrap().unwrap();
        assert!(
            high_score > low_score,
            "expected quality 100 ({high_score}) to score higher than quality 1 ({low_score})"
        );
    }

    #[test]
    fn converges_to_lowest_passing_quality() {
        // Scores >=95 for quality >= 40, else 80 — lowest passing is 40.
        let (quality, reached) =
            binary_search_quality(95.0, |q| Some(if q >= 40 { 96.0 } else { 80.0 }));
        assert_eq!(quality, 40);
        assert!(reached);
    }

    #[test]
    fn falls_back_to_100_when_nothing_passes() {
        let (quality, reached) = binary_search_quality(95.0, |_| Some(50.0));
        assert_eq!(quality, 100);
        assert!(!reached);
    }

    #[test]
    fn none_score_is_treated_as_failing() {
        let (quality, reached) = binary_search_quality(95.0, |_| None);
        assert_eq!(quality, 100);
        assert!(!reached);
    }

    #[test]
    fn everything_passing_converges_to_quality_1() {
        let (quality, reached) = binary_search_quality(95.0, |_| Some(99.0));
        assert_eq!(quality, 1);
        assert!(reached);
    }

    #[test]
    fn find_visually_lossless_quality_converges_within_range() {
        if !has_ffmpeg() {
            eprintln!("skipping: ffmpeg not present");
            return;
        }
        if check_libvmaf().is_err() {
            eprintln!("skipping: this ffmpeg build lacks libvmaf");
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let input = tmp.path().join("source.mp4");
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
                "18",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&input)
            .output()
            .unwrap();
        assert!(gen.status.success(), "fixture generation failed");

        let (quality, _reached) = find_visually_lossless_quality(&input, VideoCodec::H264).unwrap();
        assert!((1..=100).contains(&quality));
    }
}
