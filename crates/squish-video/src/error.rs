use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VideoError {
    #[error("unsupported video format at {path}: {reason}")]
    UnsupportedFormat { path: PathBuf, reason: String },

    #[error("ffmpeg failed for {path}: {stderr}")]
    FfmpegFailed { path: PathBuf, stderr: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("missing required dependency: {name}. {install_hint}")]
    MissingDependency { name: String, install_hint: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_unsupported_format() {
        let e = VideoError::UnsupportedFormat {
            path: PathBuf::from("/a.rar"),
            reason: "not a video file".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("/a.rar"));
        assert!(s.contains("not a video file"));
    }

    #[test]
    fn display_ffmpeg_failed() {
        let e = VideoError::FfmpegFailed {
            path: PathBuf::from("/a.mp4"),
            stderr: "codec not found".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("/a.mp4"));
        assert!(s.contains("codec not found"));
    }

    #[test]
    fn display_missing_dependency() {
        let e = VideoError::MissingDependency {
            name: "ffmpeg".into(),
            install_hint: "brew install ffmpeg".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("ffmpeg"));
        assert!(s.contains("brew install"));
    }
}
