use crate::error::SquishError;
use crate::options::SquishOptions;
use std::path::Path;
use usvg::{Options, Tree, WriteOptions};

/// Minify an SVG by parsing it into usvg's normalized tree (drops comments,
/// unused defs, metadata) and re-serializing with compact output options.
pub fn compress(
    input: &[u8],
    _opts: &SquishOptions,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    let tree = Tree::from_data(input, &Options::default()).map_err(|e| {
        SquishError::DecodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        }
    })?;

    let write_opts = WriteOptions {
        indent: usvg::Indent::None,
        attributes_indent: usvg::Indent::None,
        use_single_quote: false,
        preserve_text: false,
        coordinates_precision: 3,
        transforms_precision: 3,
        ..WriteOptions::default()
    };

    let xml = tree.to_string(&write_opts);
    Ok(xml.into_bytes())
}
