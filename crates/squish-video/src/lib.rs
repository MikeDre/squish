//! Video compression library for squish (ffmpeg-backed).

pub mod error;
pub mod ffmpeg;
pub mod format;
pub mod options;
pub mod result;

pub use error::VideoError;
pub use format::{detect_video_format, detect_video_from_bytes, VideoFormat};
pub use options::{VideoCodec, VideoOptions};
pub use result::VideoResult;

use squish_core::derive_output_path;
use std::path::Path;
use std::time::Instant;

/// Compress a single video file. Shells out to system ffmpeg.
///
/// On error, any partial output file is cleaned up.
pub fn squish_video(
    input: &Path,
    opts: &VideoOptions,
) -> Result<VideoResult, VideoError> {
    ffmpeg::check_ffmpeg()?;

    let start = Instant::now();
    let input_bytes = std::fs::metadata(input)?.len();

    let format_in = detect_video_format(input).ok_or_else(|| VideoError::UnsupportedFormat {
        path: input.to_path_buf(),
        reason: "could not identify video format from extension or magic bytes".into(),
    })?;

    let format_out = format_in;

    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| format_out.extension().to_string());

    let output_path = derive_output_path(input, &ext, opts.force_overwrite);

    ffmpeg::run_ffmpeg(input, &output_path, opts)?;

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
}
