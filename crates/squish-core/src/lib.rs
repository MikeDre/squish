//! Core image compression library for squish.

pub mod error;
pub mod format;
pub mod formats;
pub mod naming;
pub mod options;
pub mod result;

pub use error::SquishError;
pub use format::{detect_format, Format};
pub use naming::derive_output_path;
pub use options::SquishOptions;
pub use result::SquishResult;

use std::fs;
use std::path::Path;
use std::time::Instant;

/// Compress a single file. Reads `input`, dispatches by format, writes output
/// path (derived from `naming::derive_output_path`), returns a `SquishResult`.
///
/// On error, no output file is written.
pub fn squish_file(
    input: &Path,
    opts: &SquishOptions,
) -> Result<SquishResult, SquishError> {
    let start = Instant::now();
    let input_bytes_vec = fs::read(input)?;

    let format_in = detect_format(input, &input_bytes_vec).ok_or_else(|| {
        SquishError::UnsupportedFormat {
            path: input.to_path_buf(),
            reason: "could not identify format from extension or magic bytes".into(),
        }
    })?;

    let format_out = opts.output_format.unwrap_or(format_in);

    let output_bytes = dispatch_compress(format_out, &input_bytes_vec, opts, input)?;

    // Preserve extension spelling from input when output format matches input format
    // and the original extension was explicitly "jpeg" rather than canonical "jpg".
    let target_ext = if format_in == format_out {
        input
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_else(|| format_out.extension().to_string())
    } else {
        format_out.extension().to_string()
    };

    let output_path = derive_output_path(input, &target_ext, opts.force_overwrite);
    fs::write(&output_path, &output_bytes)?;

    Ok(SquishResult {
        input_path: input.to_path_buf(),
        output_path,
        input_bytes: input_bytes_vec.len() as u64,
        output_bytes: output_bytes.len() as u64,
        format_in,
        format_out,
        duration: start.elapsed(),
    })
}

fn dispatch_compress(
    format: Format,
    input: &[u8],
    opts: &SquishOptions,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    match format {
        Format::Png => formats::png::compress(input, opts, path),
        Format::Jpeg => formats::jpeg::compress(input, opts, path),
        Format::Webp => formats::webp::compress(input, opts, path),
        Format::Avif => formats::avif::compress(input, opts, path),
        Format::Svg => formats::svg::compress(input, opts, path),
        Format::Gif => formats::gif::compress(input, opts, path),
        other => Err(SquishError::UnsupportedFormat {
            path: path.to_path_buf(),
            reason: format!("{:?} compression not implemented yet", other),
        }),
    }
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

        let err = squish_file(&input, &SquishOptions::default()).unwrap_err();
        match err {
            SquishError::UnsupportedFormat { reason, .. } => {
                assert!(reason.contains("could not identify format"));
            }
            other => panic!("wrong error variant: {other:?}"),
        }
    }

    #[test]
    fn missing_file_returns_io_error() {
        let err = squish_file(Path::new("/nonexistent/path/xyz.png"), &SquishOptions::default())
            .unwrap_err();
        assert!(matches!(err, SquishError::Io(_)));
    }
}
