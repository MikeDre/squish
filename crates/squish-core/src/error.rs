use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SquishError {
    #[error("unsupported format at {path}: {reason}")]
    UnsupportedFormat { path: PathBuf, reason: String },

    #[error("decode failed for {path}: {source}")]
    DecodeFailed {
        path: PathBuf,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("encode failed for {path}: {source}")]
    EncodeFailed {
        path: PathBuf,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

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
        let e = SquishError::UnsupportedFormat {
            path: PathBuf::from("/a.bmp"),
            reason: "v1 does not encode BMP".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("/a.bmp"));
        assert!(s.contains("v1 does not encode BMP"));
    }

    #[test]
    fn display_missing_dependency() {
        let e = SquishError::MissingDependency {
            name: "gifsicle".into(),
            install_hint: "brew install gifsicle".into(),
        };
        assert!(format!("{e}").contains("gifsicle"));
        assert!(format!("{e}").contains("brew install"));
    }
}
