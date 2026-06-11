//! Code minification library for squish (JS/TS/CSS/HTML/JSON).

pub mod error;
pub mod format;
pub mod languages;
pub mod options;
pub mod result;

pub use error::CodeError;
pub use format::{detect_code_format, CodeFormat};
pub use options::CodeOptions;
pub use result::CodeResult;

use squish_core::{derive_output_path_with_suffix_sep, in_place_target};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub fn squish_code(input: &Path, opts: &CodeOptions) -> Result<CodeResult, CodeError> {
    let start = Instant::now();
    let input_bytes = fs::metadata(input)?.len();

    let format = detect_code_format(input).ok_or_else(|| CodeError::UnsupportedFormat {
        path: input.to_path_buf(),
        reason: "unknown extension".into(),
    })?;

    // Read as UTF-8. Distinguish UTF-8 errors from generic IO errors.
    let source = match fs::read_to_string(input) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
            return Err(CodeError::ParseFailed {
                path: input.to_path_buf(),
                line: None,
                reason: "non-UTF-8 input".into(),
            });
        }
        Err(e) => return Err(CodeError::Io(e)),
    };

    let minified = match format {
        CodeFormat::Js | CodeFormat::Ts => languages::js::minify(&source, opts, input, format)?,
        CodeFormat::Css => languages::css::minify(&source, opts, input)?,
        CodeFormat::Html => languages::html::minify(&source, opts, input)?,
        CodeFormat::Json => languages::json::minify(&source, opts, input)?,
    };

    let output_ext = output_extension(format, input);
    let output_path = if opts.overwrite {
        match in_place_target(input, &output_ext) {
            Some(target) => target,
            None => {
                return Err(CodeError::InPlaceFormatChange {
                    path: input.to_path_buf(),
                    from: input
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase(),
                    to: output_ext.clone(),
                });
            }
        }
    } else {
        let suffix = opts.suffix.as_deref().unwrap_or("min");
        derive_output_path_with_suffix_sep(input, &output_ext, opts.force_overwrite, suffix, '.')
    };

    fs::write(&output_path, minified.code.as_bytes())?;

    let mut source_map_path: Option<PathBuf> = None;
    if let Some(map_text) = minified.source_map.as_deref() {
        let map_path = output_path.with_extension(format!("{output_ext}.map"));
        fs::write(&map_path, map_text)?;
        source_map_path = Some(map_path);
    }

    let output_bytes = fs::metadata(&output_path)?.len();

    Ok(CodeResult {
        input_path: input.to_path_buf(),
        output_path,
        source_map_path,
        input_bytes,
        output_bytes,
        format,
        duration: start.elapsed(),
    })
}

/// Pure: choose the output extension. JSX/TSX/TS/MTS/CTS all become `.js` for honesty
/// about content; everything else preserves its input extension.
pub fn output_extension(format: CodeFormat, input: &Path) -> String {
    let input_ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match format {
        CodeFormat::Js if input_ext == "jsx" => "js".into(),
        CodeFormat::Ts => "js".into(),
        CodeFormat::Js => input_ext, // js, mjs, cjs preserved
        CodeFormat::Css => "css".into(),
        CodeFormat::Html => input_ext, // html, htm preserved
        CodeFormat::Json => "json".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn missing_file_returns_io_error() {
        let err =
            squish_code(Path::new("/nonexistent/file.js"), &CodeOptions::default()).unwrap_err();
        assert!(matches!(err, CodeError::Io(_)));
    }

    #[test]
    fn unknown_extension_returns_unsupported() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("mystery.xyz");
        fs::write(&p, b"x").unwrap();
        let err = squish_code(&p, &CodeOptions::default()).unwrap_err();
        assert!(matches!(err, CodeError::UnsupportedFormat { .. }));
    }

    #[test]
    fn output_extension_ts_becomes_js() {
        assert_eq!(
            output_extension(CodeFormat::Ts, &PathBuf::from("a.ts")),
            "js"
        );
        assert_eq!(
            output_extension(CodeFormat::Ts, &PathBuf::from("a.tsx")),
            "js"
        );
    }

    #[test]
    fn output_extension_jsx_becomes_js() {
        assert_eq!(
            output_extension(CodeFormat::Js, &PathBuf::from("c.jsx")),
            "js"
        );
    }

    #[test]
    fn output_extension_js_preserves_input() {
        assert_eq!(
            output_extension(CodeFormat::Js, &PathBuf::from("a.js")),
            "js"
        );
        assert_eq!(
            output_extension(CodeFormat::Js, &PathBuf::from("a.mjs")),
            "mjs"
        );
        assert_eq!(
            output_extension(CodeFormat::Js, &PathBuf::from("a.cjs")),
            "cjs"
        );
    }

    #[test]
    fn output_extension_html_preserves_input() {
        assert_eq!(
            output_extension(CodeFormat::Html, &PathBuf::from("a.htm")),
            "htm"
        );
        assert_eq!(
            output_extension(CodeFormat::Html, &PathBuf::from("a.html")),
            "html"
        );
    }

    #[test]
    fn pipeline_writes_output() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("data.json");
        fs::write(&input, br#"{"a": 1}"#).unwrap();

        let result = squish_code(&input, &CodeOptions::default()).unwrap();
        assert!(result.output_path.exists());
        assert_eq!(result.format, CodeFormat::Json);
        assert!(result.source_map_path.is_none());
    }

    #[test]
    fn source_map_written_when_requested() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("a.js");
        fs::write(&input, b"console.log('hello world');").unwrap();

        let opts = CodeOptions {
            source_map: true,
            ..Default::default()
        };
        let result = squish_code(&input, &opts).unwrap();
        let map_path = result.source_map_path.expect("expected .map path");
        assert!(map_path.exists());
        let map = fs::read_to_string(&map_path).unwrap();
        assert!(map.contains("\"version\""));
    }

    #[test]
    fn overwrite_replaces_js_in_place() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("app.js");
        fs::write(&input, b"const   x   =   1 ;\n\n\n").unwrap();

        let opts = CodeOptions {
            overwrite: true,
            ..Default::default()
        };
        let r = squish_code(&input, &opts).unwrap();

        assert_eq!(r.output_path, input);
        assert!(!tmp.path().join("app.min.js").exists());
    }

    #[test]
    fn overwrite_refuses_when_ts_becomes_js() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("app.ts");
        fs::write(&input, b"const x: number = 1;\n").unwrap();

        let opts = CodeOptions {
            overwrite: true,
            ..Default::default()
        };
        let err = squish_code(&input, &opts).unwrap_err();
        assert!(matches!(err, CodeError::InPlaceFormatChange { .. }));
        assert!(input.exists());
        assert!(!tmp.path().join("app.js").exists());
    }
}
