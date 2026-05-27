//! Video compression library for squish (ffmpeg-backed).

pub mod ffmpeg;
pub mod format;
pub mod options;
pub mod result;

pub use squish_media::MediaError as VideoError;
pub use format::{detect_video_format, detect_video_from_bytes, VideoFormat};
pub use options::{VideoCodec, VideoOptions};
pub use result::VideoResult;

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

/// Compress a single video file. Shells out to system ffmpeg.
///
/// On error, any partial output file is cleaned up.
pub fn squish_video(
    input: &Path,
    opts: &VideoOptions,
) -> Result<VideoResult, VideoError> {
    squish_media::check_ffmpeg()?;

    let start = Instant::now();
    let input_bytes = std::fs::metadata(input)?.len();

    let format_in = detect_video_format(input).ok_or_else(|| VideoError::UnsupportedFormat {
        path: input.to_path_buf(),
        reason: "could not identify video format from extension or magic bytes".into(),
    })?;

    let format_out = format_in.output_format();
    let ext = resolve_output_ext(input, format_in, format_out);

    let force_reencode = format_out != format_in;

    let (encode_path, rename_to) = if opts.overwrite {
        match in_place_target(input, &ext) {
            Some(target) => {
                // ffmpeg infers the output muxer from the file extension, so the
                // temp path must end in the real extension (the bare `.sq-…tmp`
                // name from in_place_temp_path leaves ffmpeg unable to choose a
                // muxer). Append the target extension; `.sq-` is still present so
                // stray temps remain identifiable.
                let base = in_place_temp_path(&target);
                let tmp = base.with_extension(format!(
                    "{}.{ext}",
                    base.extension().and_then(|e| e.to_str()).unwrap_or("tmp")
                ));
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

    ffmpeg::run_ffmpeg(input, &encode_path, opts, force_reencode)?;

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
        assert!(matches!(err, VideoError::Io(_) | VideoError::MissingDependency { .. }));
    }

    #[test]
    fn output_ext_preserves_input_when_no_remap() {
        // m4v parses to Mp4 but the caller's exact extension is preserved.
        let p = Path::new("clip.m4v");
        assert_eq!(resolve_output_ext(p, VideoFormat::Mp4, VideoFormat::Mp4), "m4v");
    }

    #[test]
    fn output_ext_uses_target_when_remapped() {
        let p = Path::new("clip.dv");
        assert_eq!(resolve_output_ext(p, VideoFormat::Dv, VideoFormat::Mp4), "mp4");
    }
}
