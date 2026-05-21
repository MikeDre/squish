use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MediaError {
    #[error("unsupported format at {path}: {reason}")]
    UnsupportedFormat { path: PathBuf, reason: String },

    #[error("ffmpeg failed for {path}: {stderr}")]
    FfmpegFailed { path: PathBuf, stderr: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("missing required dependency: {name}. {install_hint}")]
    MissingDependency { name: String, install_hint: String },

    #[error("invalid option: {reason}")]
    InvalidOption { reason: String },

    #[error("not an audio file (video stream present): {path}")]
    NotAudio { path: PathBuf },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_unsupported_format() {
        let e = MediaError::UnsupportedFormat {
            path: PathBuf::from("/a.rar"),
            reason: "not a media file".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("/a.rar"));
        assert!(s.contains("not a media file"));
    }

    #[test]
    fn display_ffmpeg_failed() {
        let e = MediaError::FfmpegFailed {
            path: PathBuf::from("/a.mp4"),
            stderr: "codec not found".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("/a.mp4"));
        assert!(s.contains("codec not found"));
    }

    #[test]
    fn display_missing_dependency() {
        let e = MediaError::MissingDependency {
            name: "ffmpeg".into(),
            install_hint: "brew install ffmpeg".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("ffmpeg"));
        assert!(s.contains("brew install"));
    }

    #[test]
    fn display_invalid_option() {
        let e = MediaError::InvalidOption {
            reason: "--bitrate not allowed with FLAC".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("bitrate"));
    }

    #[test]
    fn display_not_audio() {
        let e = MediaError::NotAudio {
            path: PathBuf::from("/movie.mp4"),
        };
        let s = format!("{e}");
        assert!(s.contains("/movie.mp4"));
        assert!(s.contains("video stream"));
    }

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "x");
        let e: MediaError = io_err.into();
        assert!(matches!(e, MediaError::Io(_)));
    }
}
