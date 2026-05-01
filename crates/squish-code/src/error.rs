use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CodeError {
    #[error("unsupported code format at {path}: {reason}")]
    UnsupportedFormat { path: PathBuf, reason: String },

    #[error("parse failed for {path}{}: {reason}", .line.map(|l| format!(" at line {l}")).unwrap_or_default())]
    ParseFailed {
        path: PathBuf,
        line: Option<u32>,
        reason: String,
    },

    #[error("minify failed for {path}: {reason}")]
    MinifyFailed { path: PathBuf, reason: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid option: {reason}")]
    InvalidOption { reason: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_unsupported_format() {
        let e = CodeError::UnsupportedFormat {
            path: PathBuf::from("/a.xyz"),
            reason: "unknown extension".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("/a.xyz"));
        assert!(s.contains("unknown extension"));
    }

    #[test]
    fn display_parse_failed_with_line() {
        let e = CodeError::ParseFailed {
            path: PathBuf::from("/a.js"),
            line: Some(42),
            reason: "unexpected token".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("/a.js"));
        assert!(s.contains("line 42"));
        assert!(s.contains("unexpected token"));
    }

    #[test]
    fn display_parse_failed_without_line() {
        let e = CodeError::ParseFailed {
            path: PathBuf::from("/a.json"),
            line: None,
            reason: "trailing comma".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("/a.json"));
        assert!(s.contains("trailing comma"));
        assert!(!s.contains("line"));
    }

    #[test]
    fn display_minify_failed() {
        let e = CodeError::MinifyFailed {
            path: PathBuf::from("/a.js"),
            reason: "internal panic".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("/a.js"));
        assert!(s.contains("internal panic"));
    }

    #[test]
    fn display_invalid_option() {
        let e = CodeError::InvalidOption {
            reason: "--source-map needs JS/TS/CSS files".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("source-map"));
    }

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "x");
        let e: CodeError = io_err.into();
        assert!(matches!(e, CodeError::Io(_)));
    }
}
