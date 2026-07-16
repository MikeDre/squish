//! Video compression library for squish (ffmpeg-backed).

mod auto_quality;
pub mod ffmpeg;
pub mod format;
pub mod options;
pub mod result;

pub use format::{detect_video_format, detect_video_from_bytes, VideoFormat};
pub use options::{VideoCodec, VideoOptions};
pub use result::VideoResult;
pub use squish_media::MediaError as VideoError;

use squish_core::{derive_output_path_with_suffix, in_place_target, in_place_temp_path};
use std::path::Path;
use std::time::Instant;

/// Choose the output file extension. When the output format equals the input
/// (the common case), preserve the caller's exact extension (e.g. `m4v` stays
/// `m4v`). For transcode-only inputs (output format differs, e.g. DV→MP4), use
/// the target format's canonical extension.
fn resolve_output_ext(input: &Path, format_in: VideoFormat, format_out: VideoFormat) -> String {
    if format_out == format_in {
        input
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_else(|| format_out.extension().to_string())
    } else {
        format_out.extension().to_string()
    }
}

/// Single-pass ABR toward a size budget: encode, and if the output overshoots
/// `target`, re-encode with the bitrate scaled by the observed overshoot (up to
/// 3 attempts total). Used for codecs without well-supported two-pass (AV1) and
/// as the overshoot backstop after a two-pass encode.
fn single_pass_to_target(
    input: &Path,
    encode_path: &Path,
    opts: &VideoOptions,
    force_reencode: bool,
    initial_kbps: u32,
    target: u64,
) -> Result<(), VideoError> {
    let mut kbps = initial_kbps;
    for attempt in 1..=3 {
        ffmpeg::run_ffmpeg(input, encode_path, opts, force_reencode, Some(kbps))?;
        let size = std::fs::metadata(encode_path)?.len();
        if size <= target || attempt == 3 {
            break;
        }
        let ratio = target as f64 / size as f64;
        kbps = ((kbps as f64 * ratio * 0.98) as u32).max(20);
    }
    Ok(())
}

/// Compress a single video file. Shells out to system ffmpeg.
///
/// On error, any partial output file is cleaned up.
pub fn squish_video(input: &Path, opts: &VideoOptions) -> Result<VideoResult, VideoError> {
    squish_media::check_ffmpeg()?;

    let start = Instant::now();
    let input_bytes = std::fs::metadata(input)?.len();

    let format_in = detect_video_format(input).ok_or_else(|| VideoError::UnsupportedFormat {
        path: input.to_path_buf(),
        reason: "could not identify video format from extension or magic bytes".into(),
    })?;

    let format_out = opts
        .output_format
        .unwrap_or_else(|| format_in.output_format());
    let ext = resolve_output_ext(input, format_in, format_out);

    let force_reencode = format_out != format_in;

    // A size budget needs a bitrate-controlled re-encode: compute the video
    // bitrate from the probed duration, reserving room for copied audio.
    let video_bitrate_kbps = match opts.target_size {
        None => None,
        Some(target) => {
            let ext = resolve_output_ext(input, format_in, format_out);
            let codec = opts.effective_codec_for_ext_reencode(&ext, force_reencode);
            if codec == VideoCodec::Copy {
                return Err(VideoError::InvalidOption {
                    reason: "--target-size requires re-encoding and cannot be combined with \
                             --fast / --codec copy"
                        .into(),
                });
            }
            let duration = ffmpeg::ffprobe_duration_secs(input)?;
            let audio_kbps = ffmpeg::ffprobe_audio_bitrate_kbps(input)?;
            let kbps = duration
                .and_then(|d| options::target_video_bitrate_kbps(target, d, audio_kbps))
                .ok_or_else(|| VideoError::InvalidOption {
                    reason: format!(
                        "cannot honour --target-size {target} bytes for {}: duration unknown \
                         or budget too small for this length of video",
                        input.display()
                    ),
                })?;
            Some(kbps)
        }
    };

    let (encode_path, rename_to) = if opts.overwrite {
        match in_place_target(input, &ext) {
            Some(target) => {
                let tmp = in_place_temp_path(&target);
                (tmp, Some(target))
            }
            None => {
                return Err(VideoError::InPlaceFormatChange {
                    path: input.to_path_buf(),
                    from: input
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase(),
                    to: ext.clone(),
                });
            }
        }
    } else {
        let suffix = opts.suffix.as_deref().unwrap_or("squished");
        (
            derive_output_path_with_suffix(input, &ext, opts.force_overwrite, suffix),
            None,
        )
    };

    match (video_bitrate_kbps, opts.target_size) {
        (Some(initial_kbps), Some(target)) => {
            let codec = opts.effective_codec_for_ext_reencode(&ext, force_reencode);
            if codec.supports_two_pass() {
                // Two-pass ABR is the primary strategy: an analysis pass lets
                // the encoder distribute the bitrate budget accurately, so the
                // output lands near the target in one encode. Pass-log temp
                // files live in a tempdir, never the source directory.
                let passdir = tempfile::tempdir()?;
                let passlog = passdir.path().join("squish2pass");
                ffmpeg::run_two_pass(
                    input,
                    &encode_path,
                    opts,
                    force_reencode,
                    initial_kbps,
                    &passlog,
                )?;
                // Overshoot backstop (should rarely trigger): if two-pass still
                // exceeded the budget, fall back to the single-pass retry loop
                // starting from a bitrate scaled by the observed overshoot.
                let size = std::fs::metadata(&encode_path)?.len();
                if size > target {
                    let ratio = target as f64 / size as f64;
                    let scaled = ((initial_kbps as f64 * ratio * 0.98) as u32).max(20);
                    single_pass_to_target(
                        input,
                        &encode_path,
                        opts,
                        force_reencode,
                        scaled,
                        target,
                    )?;
                }
            } else {
                // Codecs without well-supported two-pass (AV1): single-pass ABR
                // overshoots on short clips, so retry with the bitrate scaled by
                // the observed overshoot until the output fits.
                single_pass_to_target(
                    input,
                    &encode_path,
                    opts,
                    force_reencode,
                    initial_kbps,
                    target,
                )?;
            }
        }
        _ => ffmpeg::run_ffmpeg(input, &encode_path, opts, force_reencode, None)?,
    }

    let output_path = match rename_to {
        Some(target) => {
            std::fs::rename(&encode_path, &target)?;
            target
        }
        None => encode_path,
    };

    let output_bytes = std::fs::metadata(&output_path)?.len();

    Ok(VideoResult {
        input_path: input.to_path_buf(),
        output_path,
        input_bytes,
        output_bytes,
        format_in,
        format_out,
        duration: start.elapsed(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn unknown_format_returns_unsupported() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("mystery.xyz");
        fs::write(&input, b"random bytes not matching any magic").unwrap();

        let err = squish_video(&input, &VideoOptions::default()).unwrap_err();
        match err {
            VideoError::UnsupportedFormat { reason, .. } => {
                assert!(reason.contains("could not identify video format"));
            }
            // ffmpeg check may fail first if not installed
            VideoError::MissingDependency { .. } => {}
            other => panic!("wrong error variant: {other:?}"),
        }
    }

    #[test]
    fn missing_file_returns_error() {
        let err = squish_video(
            Path::new("/nonexistent/video.mp4"),
            &VideoOptions::default(),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            VideoError::Io(_) | VideoError::MissingDependency { .. }
        ));
    }

    #[test]
    fn output_ext_preserves_input_when_no_remap() {
        // m4v parses to Mp4 but the caller's exact extension is preserved.
        let p = Path::new("clip.m4v");
        assert_eq!(
            resolve_output_ext(p, VideoFormat::Mp4, VideoFormat::Mp4),
            "m4v"
        );
    }

    #[test]
    fn output_ext_uses_target_when_remapped() {
        let p = Path::new("clip.dv");
        assert_eq!(
            resolve_output_ext(p, VideoFormat::Dv, VideoFormat::Mp4),
            "mp4"
        );
    }

    #[test]
    fn output_format_override_replaces_default() {
        // No file IO — pure path/format logic via resolve_output_ext.
        let input = Path::new("clip.mov");
        let format_in = VideoFormat::Mov;
        let opts = VideoOptions {
            output_format: Some(VideoFormat::Mp4),
            ..Default::default()
        };
        // The new selection logic Task 2 introduces:
        let format_out = opts
            .output_format
            .unwrap_or_else(|| format_in.output_format());
        assert_eq!(format_out, VideoFormat::Mp4);
        let ext = resolve_output_ext(input, format_in, format_out);
        assert_eq!(ext, "mp4");
        // Force-reencode is true because output container differs from input.
        assert!(format_out != format_in);
    }

    #[test]
    fn output_format_none_preserves_input_default() {
        // Regression guard: with no override, behaviour is exactly as today.
        let input = Path::new("clip.mov");
        let format_in = VideoFormat::Mov;
        let opts = VideoOptions::default();
        let format_out = opts
            .output_format
            .unwrap_or_else(|| format_in.output_format());
        assert_eq!(format_out, VideoFormat::Mov);
        let ext = resolve_output_ext(input, format_in, format_out);
        assert_eq!(ext, "mov");
    }
}
