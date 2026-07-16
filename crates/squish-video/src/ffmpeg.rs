//! Codec-specific argv construction. Delegates execution to squish-media.

use crate::options::{VideoCodec, VideoOptions};
use crate::VideoError;
use std::ffi::OsString;
use std::path::Path;

/// Which encode pass an argv is being built for.
///
/// Two-pass ABR (`--target-size` on a well-supported codec) runs an analysis
/// pass that writes stats to a shared `-passlogfile` prefix, then a final pass
/// that reads them. `Single` is CRF or single-pass ABR — no pass flags.
#[derive(Debug, Clone, Copy)]
pub enum EncodePass<'a> {
    /// One-shot encode (CRF, or single-pass ABR with the retry loop).
    Single,
    /// Two-pass analysis pass. Encodes video only (no audio) to the null muxer,
    /// writing rate-control stats to the given `-passlogfile` prefix.
    First(&'a Path),
    /// Two-pass final pass. Reads the stats at the given prefix and muxes the
    /// real output.
    Second(&'a Path),
}

/// Build the codec-specific portion of an ffmpeg argv. Returns the full argv
/// after `ffmpeg -y -i <input>` and before `<output>`.
///
/// `video_bitrate_kbps` switches rate control from CRF to average-bitrate
/// (used by `--target-size`). `pass` selects one-shot vs. two-pass ABR.
pub fn build_codec_args(
    out_ext: &str,
    opts: &VideoOptions,
    force_reencode: bool,
    video_bitrate_kbps: Option<u32>,
    pass: EncodePass,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();
    let codec = opts.effective_codec_for_ext_reencode(out_ext, force_reencode);

    if codec == VideoCodec::Copy {
        args.push("-c".into());
        args.push("copy".into());
    } else {
        args.push("-c:v".into());
        args.push(codec.ffmpeg_encoder().into());

        if let Some(kbps) = video_bitrate_kbps {
            args.push("-b:v".into());
            args.push(format!("{kbps}k").into());
            // VBV-constrain the encode so ABR can't overshoot the budget — but
            // only for encoders that accept it. SVT-AV1 rejects -maxrate outside
            // CRF mode ("Max Bitrate only supported with CRF mode") and errors
            // out, so AV1 uses plain VBR and leans on the retry loop instead.
            if codec != VideoCodec::AV1 {
                args.push("-maxrate".into());
                args.push(format!("{kbps}k").into());
                args.push("-bufsize".into());
                args.push(format!("{}k", kbps * 2).into());
            }
        } else if let Some(crf) = opts.effective_crf_for_codec(codec) {
            args.push("-crf".into());
            args.push(crf.to_string().into());
        }

        // Two-pass stats flags. `-pass` requires identical settings across both
        // invocations, so this is emitted for both First and Second.
        match pass {
            EncodePass::First(log) => {
                args.push("-pass".into());
                args.push("1".into());
                args.push("-passlogfile".into());
                args.push(log.into());
            }
            EncodePass::Second(log) => {
                args.push("-pass".into());
                args.push("2".into());
                args.push("-passlogfile".into());
                args.push(log.into());
            }
            EncodePass::Single => {}
        }

        match codec {
            VideoCodec::H264 | VideoCodec::H265 => {
                args.push("-preset".into());
                args.push("medium".into());
            }
            VideoCodec::AV1 => {
                args.push("-preset".into());
                args.push("6".into());
            }
            VideoCodec::Vp9 => {
                // CRF-mode VP9 needs `-b:v 0` to mean "pure constant quality";
                // in target-bitrate mode the real -b:v was emitted above.
                if video_bitrate_kbps.is_none() {
                    args.push("-b:v".into());
                    args.push("0".into());
                }
            }
            VideoCodec::Copy => unreachable!(),
        }

        if let EncodePass::First(_) = pass {
            // The analysis pass only needs the video stream: drop audio and
            // subtitles and mux nothing real — pair `-an -sn -f null` with a
            // `-` output (the caller passes it).
            args.push("-an".into());
            args.push("-sn".into());
            args.push("-f".into());
            args.push("null".into());
        } else {
            // ffmpeg defaults H.265 in MP4/MOV to the `hev1` codec tag, which
            // QuickTime/Safari/iOS refuse to decode. Force `hvc1` so the output
            // plays everywhere H.265 is supported. (Irrelevant for the null
            // analysis pass.)
            if codec == VideoCodec::H265 && matches!(out_ext, "mp4" | "m4v" | "mov") {
                args.push("-tag:v".into());
                args.push("hvc1".into());
            }

            args.push("-c:a".into());
            args.push("copy".into());
            args.push("-c:s".into());
            args.push("copy".into());
        }
    }

    args.push("-map_metadata".into());
    args.push("-1".into());

    args
}

/// Build and run a single-pass ffmpeg command to compress `input` to `output`.
pub fn run_ffmpeg(
    input: &Path,
    output: &Path,
    opts: &VideoOptions,
    force_reencode: bool,
    video_bitrate_kbps: Option<u32>,
) -> Result<(), VideoError> {
    let out_ext = output.extension().and_then(|e| e.to_str()).unwrap_or("");
    let args = build_codec_args(
        out_ext,
        opts,
        force_reencode,
        video_bitrate_kbps,
        EncodePass::Single,
    );
    squish_media::run_ffmpeg(input, output, &args)
}

/// Run a two-pass ABR encode: an analysis pass to the null muxer that records
/// rate-control stats under `passlog`, then the real encode that reads them.
/// `passlog` is the `-passlogfile` prefix; callers place it in a tempdir so the
/// `<prefix>-0.log` files never touch the source directory.
pub fn run_two_pass(
    input: &Path,
    output: &Path,
    opts: &VideoOptions,
    force_reencode: bool,
    video_bitrate_kbps: u32,
    passlog: &Path,
) -> Result<(), VideoError> {
    let out_ext = output.extension().and_then(|e| e.to_str()).unwrap_or("");

    let pass1 = build_codec_args(
        out_ext,
        opts,
        force_reencode,
        Some(video_bitrate_kbps),
        EncodePass::First(passlog),
    );
    // `-f null` in the argv makes the trailing output a discarded sink; `-` is
    // the portable stand-in for /dev/null (vs. NUL on Windows).
    squish_media::run_ffmpeg(input, Path::new("-"), &pass1)?;

    let pass2 = build_codec_args(
        out_ext,
        opts,
        force_reencode,
        Some(video_bitrate_kbps),
        EncodePass::Second(passlog),
    );
    squish_media::run_ffmpeg(input, output, &pass2)
}

fn ffprobe_csv(
    path: &Path,
    entries: &str,
    select: Option<&str>,
) -> Result<Option<String>, VideoError> {
    let mut cmd = std::process::Command::new("ffprobe");
    cmd.args(["-v", "error"]);
    if let Some(streams) = select {
        cmd.args(["-select_streams", streams]);
    }
    cmd.args(["-show_entries", entries, "-of", "csv=p=0"])
        .arg(path);
    let output = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            VideoError::MissingDependency {
                name: "ffprobe".into(),
                install_hint:
                    "ffprobe ships with ffmpeg; brew install ffmpeg or apt install ffmpeg".into(),
            }
        } else {
            VideoError::Io(e)
        }
    })?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
}

/// Probe the container duration in seconds. Returns `Ok(None)` when ffprobe
/// can't determine it.
pub fn ffprobe_duration_secs(path: &Path) -> Result<Option<f64>, VideoError> {
    let Some(stdout) = ffprobe_csv(path, "format=duration", None)? else {
        return Ok(None);
    };
    Ok(stdout
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|d| d.is_finite() && *d > 0.0))
}

/// Sum the bitrates of all audio streams in kbps (audio is copied as-is, so
/// it eats into a `--target-size` budget). Streams whose bitrate ffprobe
/// reports as unknown are assumed to be 128 kbps. No audio → 0.
pub fn ffprobe_audio_bitrate_kbps(path: &Path) -> Result<u32, VideoError> {
    let Some(stdout) = ffprobe_csv(path, "stream=bit_rate", Some("a"))? else {
        return Ok(0);
    };
    let mut total_kbps: u32 = 0;
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        total_kbps += match line.parse::<u64>() {
            Ok(bps) => (bps / 1000) as u32,
            Err(_) => 128, // "N/A" — assume a typical stereo lossy stream
        };
    }
    Ok(total_kbps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{VideoCodec, VideoOptions};
    // See `crate::FFMPEG_TEST_LOCK`: shared with `crate::auto_quality`'s
    // tests, which include PATH-mutating tests that would otherwise race
    // against this module's real-ffprobe-subprocess tests.
    use crate::FFMPEG_TEST_LOCK as TEST_SERIAL;

    fn args(out_ext: &str, opts: VideoOptions) -> Vec<String> {
        build_codec_args(out_ext, &opts, false, None, EncodePass::Single)
            .into_iter()
            .map(|o| o.into_string().unwrap())
            .collect()
    }

    fn args_with_bitrate(out_ext: &str, opts: VideoOptions, kbps: u32) -> Vec<String> {
        build_codec_args(out_ext, &opts, false, Some(kbps), EncodePass::Single)
            .into_iter()
            .map(|o| o.into_string().unwrap())
            .collect()
    }

    fn args_for_pass(opts: VideoOptions, kbps: u32, pass: EncodePass) -> Vec<String> {
        build_codec_args("mp4", &opts, false, Some(kbps), pass)
            .into_iter()
            .map(|o| o.into_string().unwrap())
            .collect()
    }

    fn pos(a: &[String], flag: &str) -> Option<usize> {
        a.iter().position(|s| s == flag)
    }

    #[test]
    fn pass_one_analyses_with_no_audio_to_null() {
        let opts = VideoOptions {
            codec: Some(VideoCodec::H265),
            ..Default::default()
        };
        let log = std::path::Path::new("/tmp/ff2pass");
        let a = args_for_pass(opts, 1200, EncodePass::First(log));

        // Analysis pass: -pass 1 with the shared log prefix.
        let p = pos(&a, "-pass").expect("expected -pass");
        assert_eq!(a[p + 1], "1");
        let l = pos(&a, "-passlogfile").expect("expected -passlogfile");
        assert_eq!(a[l + 1], "/tmp/ff2pass");
        // No audio is encoded/copied in the analysis pass, and it targets the
        // null muxer (paired with a "-" output by the caller).
        assert!(a.contains(&"-an".to_string()));
        let f = pos(&a, "-f").expect("expected -f");
        assert_eq!(a[f + 1], "null");
        assert!(
            !a.windows(2).any(|w| w == ["-c:a", "copy"]),
            "pass 1 must not copy audio: {a:?}"
        );
        // Rate control is still ABR-constrained.
        assert!(pos(&a, "-b:v").is_some());
    }

    #[test]
    fn pass_two_encodes_for_real_with_audio() {
        let opts = VideoOptions {
            codec: Some(VideoCodec::H265),
            ..Default::default()
        };
        let log = std::path::Path::new("/tmp/ff2pass");
        let a = args_for_pass(opts, 1200, EncodePass::Second(log));

        let p = pos(&a, "-pass").expect("expected -pass");
        assert_eq!(a[p + 1], "2");
        let l = pos(&a, "-passlogfile").expect("expected -passlogfile");
        assert_eq!(a[l + 1], "/tmp/ff2pass");
        // The final pass muxes the real output: audio copied, no null muxer.
        assert!(a.windows(2).any(|w| w == ["-c:a", "copy"]));
        assert!(!a.contains(&"-an".to_string()));
        assert!(!a.contains(&"null".to_string()));
        // hvc1 tag still applied on the real H.265/MP4 output.
        let t = pos(&a, "-tag:v").expect("expected -tag:v on pass 2");
        assert_eq!(a[t + 1], "hvc1");
    }

    #[test]
    fn single_pass_has_no_pass_flags() {
        let opts = VideoOptions {
            codec: Some(VideoCodec::H264),
            ..Default::default()
        };
        let a = args_for_pass(opts, 1200, EncodePass::Single);
        assert!(!a.contains(&"-pass".to_string()));
        assert!(!a.contains(&"-passlogfile".to_string()));
        assert!(!a.contains(&"-an".to_string()));
    }

    #[test]
    fn target_bitrate_emits_b_v_instead_of_crf() {
        let opts = VideoOptions {
            codec: Some(VideoCodec::H265),
            ..Default::default()
        };
        let a = args_with_bitrate("mp4", opts, 1200);
        let pos = a.iter().position(|s| s == "-b:v").expect("expected -b:v");
        assert_eq!(a[pos + 1], "1200k");
        assert!(!a.contains(&"-crf".to_string()));
    }

    #[test]
    fn vbv_codecs_constrain_bitrate_with_maxrate() {
        // H.264/H.265/VP9 accept VBV (-maxrate/-bufsize) to stop ABR overshoot.
        for codec in [VideoCodec::H264, VideoCodec::H265, VideoCodec::Vp9] {
            let ext = if codec == VideoCodec::Vp9 {
                "webm"
            } else {
                "mp4"
            };
            let opts = VideoOptions {
                codec: Some(codec),
                ..Default::default()
            };
            let a = args_with_bitrate(ext, opts, 1200);
            assert!(a.contains(&"-maxrate".to_string()), "{codec:?}: {a:?}");
            assert!(a.contains(&"-bufsize".to_string()), "{codec:?}: {a:?}");
        }
    }

    #[test]
    fn av1_bitrate_is_plain_vbr_without_maxrate() {
        // SVT-AV1 rejects -maxrate/-bufsize ("Max Bitrate only supported with
        // CRF mode") and errors out, so target-size AV1 must use plain VBR.
        let opts = VideoOptions {
            codec: Some(VideoCodec::AV1),
            ..Default::default()
        };
        let a = args_with_bitrate("mp4", opts, 1200);
        let pos = a.iter().position(|s| s == "-b:v").expect("expected -b:v");
        assert_eq!(a[pos + 1], "1200k");
        assert!(
            !a.contains(&"-maxrate".to_string()),
            "AV1 must not set -maxrate: {a:?}"
        );
        assert!(
            !a.contains(&"-bufsize".to_string()),
            "AV1 must not set -bufsize: {a:?}"
        );
    }

    #[test]
    fn target_bitrate_vp9_replaces_b_v_zero() {
        let opts = VideoOptions {
            codec: Some(VideoCodec::Vp9),
            ..Default::default()
        };
        let a = args_with_bitrate("webm", opts, 900);
        let positions: Vec<usize> = a
            .iter()
            .enumerate()
            .filter(|(_, s)| *s == "-b:v")
            .map(|(i, _)| i)
            .collect();
        assert_eq!(positions.len(), 1, "exactly one -b:v expected: {a:?}");
        assert_eq!(a[positions[0] + 1], "900k");
        assert!(!a.contains(&"-crf".to_string()));
    }

    #[test]
    fn ffprobe_duration_of_sample_fixture() {
        let _guard = TEST_SERIAL.lock().unwrap();
        if !std::process::Command::new("ffprobe")
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            eprintln!("skipping: ffprobe not present");
            return;
        }
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("tests/fixtures/sample.mp4");
        let d = ffprobe_duration_secs(&p).unwrap().expect("duration");
        assert!((d - 2.0).abs() < 0.2, "expected ~2.0s, got {d}");
    }

    #[test]
    fn ffprobe_audio_bitrate_zero_for_silent_fixture() {
        let _guard = TEST_SERIAL.lock().unwrap();
        if !std::process::Command::new("ffprobe")
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            eprintln!("skipping: ffprobe not present");
            return;
        }
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("tests/fixtures/sample.mp4");
        assert_eq!(ffprobe_audio_bitrate_kbps(&p).unwrap(), 0);
    }

    #[test]
    fn force_reencode_replaces_fast_copy() {
        let opts = VideoOptions {
            fast: true,
            ..Default::default()
        };
        let a: Vec<String> = build_codec_args("mp4", &opts, true, None, EncodePass::Single)
            .into_iter()
            .map(|o| o.into_string().unwrap())
            .collect();
        assert!(a.contains(&"-c:v".to_string()));
        assert!(a.contains(&"libx265".to_string()));
        assert!(!a.windows(2).any(|w| w == ["-c", "copy"]));
    }

    #[test]
    fn h265_mp4_emits_hvc1_tag() {
        let opts = VideoOptions {
            codec: Some(VideoCodec::H265),
            ..Default::default()
        };
        let a = args("mp4", opts);
        let pos = a
            .iter()
            .position(|s| s == "-tag:v")
            .expect("expected -tag:v");
        assert_eq!(a[pos + 1], "hvc1");
    }

    #[test]
    fn h264_does_not_emit_hvc1() {
        let opts = VideoOptions {
            codec: Some(VideoCodec::H264),
            ..Default::default()
        };
        let a = args("mp4", opts);
        assert!(!a.contains(&"-tag:v".to_string()));
    }

    #[test]
    fn copy_codec_only_emits_c_copy() {
        let opts = VideoOptions {
            codec: Some(VideoCodec::Copy),
            ..Default::default()
        };
        let a = args("mp4", opts);
        assert!(a.contains(&"-c".to_string()));
        assert!(a.contains(&"copy".to_string()));
        assert!(!a.contains(&"-c:v".to_string()));
    }

    #[test]
    fn vp9_emits_bv_zero() {
        let opts = VideoOptions {
            codec: Some(VideoCodec::Vp9),
            ..Default::default()
        };
        let a = args("webm", opts);
        let pos = a.iter().position(|s| s == "-b:v").expect("expected -b:v");
        assert_eq!(a[pos + 1], "0");
    }
}
