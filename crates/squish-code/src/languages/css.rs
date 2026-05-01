use crate::error::CodeError;
use crate::languages::MinifyOutput;
use crate::options::CodeOptions;
use lightningcss::printer::PrinterOptions;
use lightningcss::stylesheet::{MinifyOptions, ParserOptions, StyleSheet};
use std::path::Path;

pub fn minify(input: &str, opts: &CodeOptions, path: &Path) -> Result<MinifyOutput, CodeError> {
    let mut stylesheet = StyleSheet::parse(input, ParserOptions::default()).map_err(|e| {
        // lightningcss errors carry a position with line info on the inner type.
        let line = e.loc.as_ref().map(|loc| loc.line);
        CodeError::ParseFailed {
            path: path.to_path_buf(),
            line,
            reason: e.kind.to_string(),
        }
    })?;

    stylesheet
        .minify(MinifyOptions::default())
        .map_err(|e| CodeError::MinifyFailed {
            path: path.to_path_buf(),
            reason: format!("{e:?}"),
        })?;

    let mut source_map = if opts.source_map {
        Some(parcel_sourcemap::SourceMap::new(""))
    } else {
        None
    };

    let printer_opts = PrinterOptions {
        minify: true,
        source_map: source_map.as_mut(),
        ..PrinterOptions::default()
    };

    let result = stylesheet
        .to_css(printer_opts)
        .map_err(|e| CodeError::MinifyFailed {
            path: path.to_path_buf(),
            reason: format!("{e:?}"),
        })?;

    let source_map_string = if let Some(mut sm) = source_map {
        Some(
            sm.to_json(None)
                .map_err(
                    |e: parcel_sourcemap::SourceMapError| CodeError::MinifyFailed {
                        path: path.to_path_buf(),
                        reason: format!("source map serialization: {e:?}"),
                    },
                )?,
        )
    } else {
        None
    };

    Ok(MinifyOutput {
        code: result.code,
        source_map: source_map_string,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p() -> PathBuf {
        PathBuf::from("test.css")
    }

    #[test]
    fn minifies_css_with_whitespace_and_comments() {
        let input = r#"
        /* a comment */
        .button {
            color: red;
            padding: 10px;
        }
        "#;
        let out = minify(input, &CodeOptions::default(), &p()).unwrap();
        assert!(out.code.len() < input.len());
        assert!(!out.code.contains("/*"));
        assert!(out.code.contains(".button"));
    }

    #[test]
    fn output_is_parseable_css() {
        let input = ".x{color:#fff}";
        let out = minify(input, &CodeOptions::default(), &p()).unwrap();
        StyleSheet::parse(&out.code, ParserOptions::default()).unwrap();
    }

    #[test]
    fn rejects_broken_css() {
        // A @layer rule with multiple names and a block body is structurally invalid
        // in CSS and triggers AtRuleBodyInvalid in lightningcss (not subject to
        // value-level error recovery).
        let input = "@layer foo, bar { .x { color: red } }";
        let err = minify(input, &CodeOptions::default(), &p()).unwrap_err();
        assert!(matches!(err, CodeError::ParseFailed { .. }));
    }

    #[test]
    fn source_map_emitted_when_requested() {
        let input = ".a{color:red}";
        let opts = CodeOptions {
            source_map: true,
            ..Default::default()
        };
        let out = minify(input, &opts, &p()).unwrap();
        let sm = out.source_map.expect("source map should be emitted");
        assert!(sm.contains("\"version\":3"));
    }

    #[test]
    fn no_source_map_by_default() {
        let input = ".a{color:red}";
        let out = minify(input, &CodeOptions::default(), &p()).unwrap();
        assert!(out.source_map.is_none());
    }
}
