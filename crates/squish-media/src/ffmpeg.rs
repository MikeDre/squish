//! ffmpeg binary detection and invocation.

use crate::error::MediaError;
use std::process::Command;

/// Check that ffmpeg is available on PATH.
pub fn check_ffmpeg() -> Result<(), MediaError> {
    match Command::new("ffmpeg").arg("-version").output() {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(MediaError::MissingDependency {
            name: "ffmpeg".into(),
            install_hint: "brew install ffmpeg (macOS) or apt install ffmpeg (Linux)".into(),
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
}
