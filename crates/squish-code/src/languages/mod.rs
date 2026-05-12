//! Per-language minifiers. Each module exposes `pub fn minify` returning `MinifyOutput`.

pub mod css;
pub mod html;
pub mod js;
pub mod json;

#[derive(Debug, Clone)]
pub struct MinifyOutput {
    pub code: String,
    pub source_map: Option<String>,
}
