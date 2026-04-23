//! ffmpeg binary detection, command building, and execution.

use crate::error::VideoError;
use crate::options::{VideoCodec, VideoOptions};
use std::path::Path;
use std::process::Command;

/// Check that ffmpeg is available on PATH.
pub fn check_ffmpeg() -> Result<(), VideoError> {
    match Command::new("ffmpeg").arg("-version").output() {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(VideoError::MissingDependency {
            name: "ffmpeg".into(),
            install_hint: "brew install ffmpeg (macOS) or apt install ffmpeg (Linux)".into(),
        }),
    }
}

/// Build and run an ffmpeg command to compress `input` to `output`.
pub fn run_ffmpeg(
    input: &Path,
    output: &Path,
    opts: &VideoOptions,
) -> Result<(), VideoError> {
    let mut cmd = Command::new("ffmpeg");

    // Overwrite output without asking
    cmd.arg("-y");

    // Input file
    cmd.arg("-i").arg(input);

    let codec = opts.effective_codec();

    if codec == VideoCodec::Copy {
        // Fast passthrough: copy all streams
        cmd.arg("-c").arg("copy");
    } else {
        // Video codec
        cmd.arg("-c:v").arg(codec.ffmpeg_encoder());

        // CRF quality
        if let Some(crf) = opts.effective_crf() {
            cmd.arg("-crf").arg(crf.to_string());
        }

        // Preset
        match codec {
            VideoCodec::H264 | VideoCodec::H265 => {
                cmd.arg("-preset").arg("medium");
            }
            VideoCodec::AV1 => {
                cmd.arg("-preset").arg("6");
            }
            VideoCodec::Copy => unreachable!(),
        }

        // Copy audio stream as-is
        cmd.arg("-c:a").arg("copy");

        // Copy subtitle streams
        cmd.arg("-c:s").arg("copy");
    }

    // Strip metadata
    cmd.arg("-map_metadata").arg("-1");

    // Output file
    cmd.arg(output);

    let result = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            VideoError::MissingDependency {
                name: "ffmpeg".into(),
                install_hint: "brew install ffmpeg (macOS) or apt install ffmpeg (Linux)".into(),
            }
        } else {
            VideoError::Io(e)
        }
    })?;

    if !result.status.success() {
        // Clean up partial output
        let _ = std::fs::remove_file(output);
        let stderr = String::from_utf8_lossy(&result.stderr).to_string();
        return Err(VideoError::FfmpegFailed {
            path: input.to_path_buf(),
            stderr,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_ffmpeg_returns_ok_when_available() {
        if Command::new("ffmpeg").arg("-version").output().is_ok() {
            assert!(check_ffmpeg().is_ok());
        }
    }
}
