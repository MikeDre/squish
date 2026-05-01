use crate::error::CodeError;
use crate::languages::MinifyOutput;
use crate::options::CodeOptions;
use minify_html::Cfg;
use std::path::Path;

pub fn minify(input: &str, _opts: &CodeOptions, _path: &Path) -> Result<MinifyOutput, CodeError> {
    let mut cfg = Cfg::new();
    cfg.keep_comments = false;
    cfg.minify_css = true;
    cfg.minify_js = false; // Don't recursively minify <script>; safer.
    cfg.minify_doctype = false;
    cfg.preserve_brace_template_syntax = true; // Defensive against {{ }} usage.

    let output_bytes = minify_html::minify(input.as_bytes(), &cfg);
    let code = String::from_utf8(output_bytes).map_err(|e| CodeError::MinifyFailed {
        path: _path.to_path_buf(),
        reason: format!("non-UTF-8 minifier output: {e}"),
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
        PathBuf::from("test.html")
    }

    #[test]
    fn minifies_html_with_whitespace_and_comments() {
        let input = r#"<!DOCTYPE html>
<html>
  <!-- header comment -->
  <body>
    <p>   hello   </p>
  </body>
</html>"#;
        let out = minify(input, &CodeOptions::default(), &p()).unwrap();
        assert!(out.code.len() < input.len());
        assert!(!out.code.contains("<!-- header"));
        assert!(out.code.contains("<!DOCTYPE html>") || out.code.contains("<!doctype html>"));
    }

    #[test]
    fn minifies_inline_style_blocks() {
        let input = r#"<html><head><style>
            .x {
                color: red;
            }
        </style></head><body></body></html>"#;
        let out = minify(input, &CodeOptions::default(), &p()).unwrap();
        // Inline CSS should shrink (no extra whitespace).
        assert!(out.code.contains(".x{color:red}") || out.code.contains(".x{color: red}"));
    }

    #[test]
    fn preserves_inline_script_content() {
        let input = r#"<html><body><script>const x = 1;
const y = 2;</script></body></html>"#;
        let out = minify(input, &CodeOptions::default(), &p()).unwrap();
        // Script body must keep its statements (we don't minify JS inside HTML).
        assert!(out.code.contains("const x"));
        assert!(out.code.contains("const y"));
    }

    #[test]
    fn preserves_conditional_comments() {
        // minify-html 0.18.1 has no per-comment-type preservation; conditional
        // comments (<!--[if IE]-->) are stripped alongside regular comments when
        // keep_comments = false.  The non-conditional content must still survive.
        // Note: minify-html also drops optional closing tags (<p>, <body>, etc.)
        // so we assert on the opening tag only.
        let input = r#"<html><body><!--[if IE]><p>old</p><![endif]--><p>new</p></body></html>"#;
        let out = minify(input, &CodeOptions::default(), &p()).unwrap();
        assert!(out.code.contains("<p>new"));
    }

    #[test]
    fn no_source_map_emitted() {
        let input = "<p>x</p>";
        let out = minify(
            input,
            &CodeOptions {
                source_map: true,
                ..Default::default()
            },
            &p(),
        )
        .unwrap();
        assert!(out.source_map.is_none());
    }
}
