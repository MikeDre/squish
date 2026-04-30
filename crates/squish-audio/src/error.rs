use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AudioError {
    #[error("unsupported audio format at {path}: {reason}")]
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
        let e = AudioError::UnsupportedFormat {
            path: PathBuf::from("/a.xyz"),
            reason: "not audio".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("/a.xyz"));
        assert!(s.contains("not audio"));
    }

    #[test]
    fn display_ffmpeg_failed() {
        let e = AudioError::FfmpegFailed {
            path: PathBuf::from("/a.mp3"),
            stderr: "no encoder".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("/a.mp3"));
        assert!(s.contains("no encoder"));
    }

    #[test]
    fn display_missing_dependency() {
        let e = AudioError::MissingDependency {
            name: "ffmpeg".into(),
            install_hint: "brew install ffmpeg".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("ffmpeg"));
        assert!(s.contains("brew install"));
    }

    #[test]
    fn display_invalid_option() {
        let e = AudioError::InvalidOption {
            reason: "--bitrate not allowed with FLAC".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("bitrate"));
    }

    #[test]
    fn display_not_audio() {
        let e = AudioError::NotAudio {
            path: PathBuf::from("/movie.mp4"),
        };
        let s = format!("{e}");
        assert!(s.contains("/movie.mp4"));
        assert!(s.contains("video stream"));
    }

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "x");
        let e: AudioError = io_err.into();
        assert!(matches!(e, AudioError::Io(_)));
    }
}
