use crate::error::SquishError;
use crate::options::SquishOptions;
use oxvg_ast::parse::roxmltree::parse_with_options;
use oxvg_ast::serialize::{Node, Options as SerializeOptions};
use oxvg_ast::visitor::Info;
use oxvg_optimiser::Jobs;
use roxmltree::ParsingOptions;
use std::path::Path;

/// Minify an SVG using `oxvg_optimiser` (a Rust port of SVGO).
///
/// Runs the default SVGO-equivalent plugin set: strips comments / metadata /
/// editor data, collapses default attributes, rewrites path data to use
/// relative coordinates with safe precision rounding, shortens colors, and
/// removes redundant groups and defs. The transformations are
/// render-equivalent within sub-pixel precision tolerance and preserve
/// `viewBox`.
///
/// `SquishOptions` is currently ignored for SVG: there is no traditional
/// quality knob for vector data, and the default jobs are already
/// render-equivalent (so `--lossless` is effectively the default). Resize
/// (`--max-width`/`--max-height`) and format conversion (`--format`) are
/// handled upstream in `squish_file` and never reach this handler.
///
/// Size guard: if the optimised output is not strictly smaller than the
/// input, the input bytes are returned unchanged. This protects already-
/// minified SVGs from ever being grown by squish.
pub fn compress(input: &[u8], _opts: &SquishOptions, path: &Path) -> Result<Vec<u8>, SquishError> {
    let source = std::str::from_utf8(input).map_err(|e| SquishError::DecodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;

    let parsed: Result<String, SquishError> = parse_with_options(
        source,
        ParsingOptions {
            allow_dtd: true,
            ..ParsingOptions::default()
        },
        |dom, allocator| -> Result<String, SquishError> {
            let info = Info {
                path: Some(path.to_path_buf()),
                multipass_count: 0,
                allocator,
            };
            Jobs::default()
                .run(dom, &info)
                .map_err(|e| SquishError::EncodeFailed {
                    path: path.to_path_buf(),
                    source: Box::new(std::io::Error::other(e.to_string())),
                })?;
            dom.serialize_with_options(SerializeOptions::default())
                .map_err(|e| SquishError::EncodeFailed {
                    path: path.to_path_buf(),
                    source: Box::new(std::io::Error::other(e.to_string())),
                })
        },
    )
    .map_err(|e| SquishError::DecodeFailed {
        path: path.to_path_buf(),
        source: Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e.to_string(),
        )),
    })?;

    let minified = parsed?.into_bytes();

    // Size guard: never grow the input.
    if minified.len() < input.len() {
        Ok(minified)
    } else {
        Ok(input.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Build a representative regression SVG: contains `viewBox`, root
    /// `fill="none"`, and many `<path>` elements using compact path-data
    /// notation. Large enough that the current usvg-based handler regresses
    /// size (re-emitted whitespace + inlined inheritance + added `<defs/>`).
    fn synthetic_regression_svg() -> String {
        let mut s = String::from(
            r#"<svg width="300" height="100" viewBox="0 0 300 100" fill="none" xmlns="http://www.w3.org/2000/svg">"#,
        );
        for i in 0..30u32 {
            let x = i as f64;
            // Compact-notation path data: no spaces between command and args.
            s.push_str(&format!(
                r#"<path d="M{x}.111 {x}.222C{x}.333 {x}.444 {x}.555 {x}.666 {x}.777 {x}.888Z"/>"#
            ));
        }
        s.push_str("</svg>");
        s
    }

    #[test]
    fn shrinks_typical_svg_and_preserves_viewbox() {
        let input = synthetic_regression_svg();
        let input_bytes = input.as_bytes();
        let opts = SquishOptions::default();
        let output = compress(input_bytes, &opts, &PathBuf::from("regression.svg"))
            .expect("compress should succeed");
        let output_str = std::str::from_utf8(&output).expect("output is utf-8");

        // Regression guard: never larger than input.
        assert!(
            output.len() < input_bytes.len(),
            "expected shrinkage; got {} → {} bytes",
            input_bytes.len(),
            output.len()
        );
        // Correctness: viewBox must be preserved.
        assert!(
            output_str.contains("viewBox="),
            "viewBox must be preserved; output head: {}",
            &output_str[..output_str.len().min(200)]
        );
        // No empty `<defs/>` boilerplate when input had no defs.
        assert!(
            !output_str.contains("<defs/>") && !output_str.contains("<defs />"),
            "no <defs/> should be emitted when input had no defs"
        );
    }

    #[test]
    fn idempotent_size_when_rerun() {
        let input = synthetic_regression_svg();
        let opts = SquishOptions::default();
        let once = compress(input.as_bytes(), &opts, &PathBuf::from("a.svg")).expect("first pass");
        let twice = compress(&once, &opts, &PathBuf::from("b.svg")).expect("second pass");
        assert!(
            twice.len() <= once.len(),
            "second pass grew the output: {} → {}",
            once.len(),
            twice.len()
        );
    }
}
