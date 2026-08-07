//! SVG handling.
//!
//! Minification and rasterisation are unrelated engines with unrelated
//! failure modes — oxvg rewrites XML, resvg paints pixels — so they live in
//! separate modules. `compress` is re-exported here so callers keep using
//! `formats::svg::compress`.

mod minify;
mod render;

pub use minify::compress;
pub use render::rasterize;
