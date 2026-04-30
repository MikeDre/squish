//! ffmpeg/ffprobe binary detection, command building, and execution.

use crate::error::AudioError;
use std::process::Command;

/// Check that ffmpeg is available on PATH.
pub fn check_ffmpeg() -> Result<(), AudioError> {
    match Command::new("ffmpeg").arg("-version").output() {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(AudioError::MissingDependency {
            name: "ffmpeg".into(),
            install_hint: "brew install ffmpeg (macOS) or apt install ffmpeg (Linux)".into(),
        }),
    }
}

/// Check that ffprobe is available on PATH.
pub fn check_ffprobe() -> Result<(), AudioError> {
    match Command::new("ffprobe").arg("-version").output() {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(AudioError::MissingDependency {
            name: "ffprobe".into(),
            install_hint: "ffprobe ships with ffmpeg; brew install ffmpeg or apt install ffmpeg"
                .into(),
        }),
    }
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

    #[test]
    fn check_ffprobe_returns_ok_when_available() {
        if Command::new("ffprobe").arg("-version").output().is_ok() {
            assert!(check_ffprobe().is_ok());
        }
    }
}
