use crate::error::CodeError;
use crate::languages::MinifyOutput;
use crate::options::CodeOptions;
use std::path::Path;

pub fn minify(input: &str, _opts: &CodeOptions, path: &Path) -> Result<MinifyOutput, CodeError> {
    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|e| CodeError::ParseFailed {
            path: path.to_path_buf(),
            line: Some(e.line() as u32),
            reason: e.to_string(),
        })?;
    let code = serde_json::to_string(&value).map_err(|e| CodeError::MinifyFailed {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;
    Ok(MinifyOutput {
        code,
        source_map: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p() -> PathBuf {
        PathBuf::from("test.json")
    }

    #[test]
    fn minifies_pretty_json() {
        let input = r#"{
  "name": "squish",
  "version": "0.2.1"
}"#;
        let out = minify(input, &CodeOptions::default(), &p()).unwrap();
        assert!(out.code.len() < input.len());
        assert_eq!(out.code, r#"{"name":"squish","version":"0.2.1"}"#);
    }

    #[test]
    fn round_trips_to_same_value() {
        let input = r#"{"a":[1,2,3],"b":{"nested":true}}"#;
        let out = minify(input, &CodeOptions::default(), &p()).unwrap();
        let original: serde_json::Value = serde_json::from_str(input).unwrap();
        let minified: serde_json::Value = serde_json::from_str(&out.code).unwrap();
        assert_eq!(original, minified);
    }

    #[test]
    fn rejects_malformed_json() {
        let input = r#"{x: 1}"#; // unquoted key
        let err = minify(input, &CodeOptions::default(), &p()).unwrap_err();
        assert!(matches!(err, CodeError::ParseFailed { .. }));
    }

    #[test]
    fn rejects_jsonc_with_comments() {
        let input = r#"{"a":1 /* comment */}"#;
        let err = minify(input, &CodeOptions::default(), &p()).unwrap_err();
        assert!(matches!(err, CodeError::ParseFailed { .. }));
    }

    #[test]
    fn handles_unicode() {
        let input = r#"{"greeting": "héllo wörld"}"#;
        let out = minify(input, &CodeOptions::default(), &p()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out.code).unwrap();
        assert_eq!(parsed["greeting"], "héllo wörld");
    }

    #[test]
    fn no_source_map_emitted() {
        let input = r#"{"a":1}"#;
        let out = minify(
            input,
            &CodeOptions {
                source_map: true,
                ..Default::default()
            },
            &p(),
        )
        .unwrap();
        assert!(
            out.source_map.is_none(),
            "JSON should never emit source maps"
        );
    }
}
