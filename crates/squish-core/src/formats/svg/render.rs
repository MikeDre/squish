//! Rasterise SVG input with resvg.
//!
//! A vector has no native resolution, so the caller must say how big the
//! render should be (`SquishOptions::width`/`height`). Everything downstream —
//! crop, resize, encode — then treats the result as an ordinary raster.
//!
//! No network access: resvg resolves `<image>` hrefs from `data:` URIs and
//! from paths relative to `resources_dir`, and never fetches a URL.

use crate::error::SquishError;
use crate::options::SquishOptions;
use image::{DynamicImage, RgbaImage};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{fontdb, FontFamily, Group, Node, Options, Tree};
use roxmltree::ParsingOptions;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Arc, OnceLock};

/// Hard ceiling on one render: ~1 GB as RGBA8 (16384×16384). Without it
/// `--width 100000` asks for tens of gigabytes, and `Pixmap::new` reports the
/// failure only by returning `None` — far from the cause and impossible to
/// explain to the user.
const MAX_RENDER_PIXELS: u64 = 268_435_456;

/// Fallback family for text that names none. Deliberately generic: usvg
/// injects this name into every span's family list, and a concrete name here
/// (usvg's own default is "Times New Roman") would make `font_warnings` report
/// a missing font on any machine that happens not to have it.
const FALLBACK_FONT_FAMILY: &str = "sans-serif";

/// Families that always resolve to a database default. Warning about these
/// would be noise — and `FALLBACK_FONT_FAMILY` is one of them, which is how
/// usvg's injected fallback stays silent.
const GENERIC_FAMILIES: [&str; 5] = ["serif", "sans-serif", "monospace", "cursive", "fantasy"];

/// Render `input` at the size implied by `opts`, returning straight-alpha
/// RGBA8 pixels plus any font-substitution warnings.
pub fn rasterize(
    input: &[u8],
    opts: &SquishOptions,
    path: &Path,
) -> Result<(DynamicImage, Vec<String>), SquishError> {
    let source = std::str::from_utf8(input).map_err(|e| SquishError::DecodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;
    if !has_declared_geometry(source) {
        return Err(SquishError::MissingRenderSize {
            path: path.to_path_buf(),
            reason: "the file declares no viewBox and no absolute width/height, so it has \
                     no intrinsic size or aspect ratio"
                .into(),
        });
    }

    let db = font_db();
    let options = Options {
        fontdb: db.clone(),
        font_family: FALLBACK_FONT_FAMILY.to_string(),
        resources_dir: path.parent().map(|d| d.to_path_buf()),
        ..Options::default()
    };
    let tree = Tree::from_data(input, &options).map_err(|e| SquishError::DecodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;

    let intrinsic = tree.size();
    let (w, h) = resolve_size(intrinsic.width(), intrinsic.height(), opts, path)?;

    let mut pixmap = Pixmap::new(w, h).ok_or_else(|| SquishError::MissingRenderSize {
        path: path.to_path_buf(),
        reason: format!("could not allocate a {w}x{h} canvas"),
    })?;
    let transform =
        Transform::from_scale(w as f32 / intrinsic.width(), h as f32 / intrinsic.height());
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let img = RgbaImage::from_raw(w, h, demultiply(&pixmap)).ok_or_else(|| {
        SquishError::DecodeFailed {
            path: path.to_path_buf(),
            source: "rendered pixel buffer did not match the requested canvas".into(),
        }
    })?;
    let warnings = font_warnings(&tree, &db, path);
    Ok((DynamicImage::ImageRgba8(img), warnings))
}

/// The system font database, loaded once per process. A 200-file directory run
/// must not pay the font scan 200 times.
fn font_db() -> Arc<fontdb::Database> {
    static DB: OnceLock<Arc<fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    })
    .clone()
}

/// Whether the root `<svg>` carries enough geometry to have an intrinsic size
/// or aspect ratio: a `viewBox`, or both `width` and `height` in absolute
/// units. A percentage-only `width="100%"` with no `viewBox` carries neither,
/// and usvg would quietly substitute its 100×100 `default_size` — making
/// squish report an arbitrary canvas as if it were the file's own.
fn has_declared_geometry(source: &str) -> bool {
    let parsing = ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    let Ok(doc) = roxmltree::Document::parse_with_options(source, parsing) else {
        return false;
    };
    let root = doc.root_element();
    if root.attribute("viewBox").is_some() {
        return true;
    }
    matches!(
        (root.attribute("width"), root.attribute("height")),
        (Some(w), Some(h)) if is_absolute_length(w) && is_absolute_length(h)
    )
}

/// A length usvg can turn into pixels without a viewport — anything but a
/// percentage.
fn is_absolute_length(value: &str) -> bool {
    !value.trim_end().ends_with('%')
}

/// The canvas to paint onto, or an error explaining what the user must pass.
fn resolve_size(
    intrinsic_w: f32,
    intrinsic_h: f32,
    opts: &SquishOptions,
    path: &Path,
) -> Result<(u32, u32), SquishError> {
    let Some((w, h)) = opts.render_size(intrinsic_w, intrinsic_h) else {
        return Err(SquishError::MissingRenderSize {
            path: path.to_path_buf(),
            reason: "converting an SVG to a raster format needs --width or --height".into(),
        });
    };
    if w as u64 * h as u64 > MAX_RENDER_PIXELS {
        return Err(SquishError::MissingRenderSize {
            path: path.to_path_buf(),
            reason: format!(
                "requested render size {w}x{h} is over the {MAX_RENDER_PIXELS} pixel limit"
            ),
        });
    }
    Ok((w, h))
}

/// One warning naming every font family the SVG asked for that this machine
/// cannot supply. usvg substitutes silently, so without this the output just
/// looks wrong.
///
/// Only the visible tree is walked: text inside a pattern or marker is not
/// reported. Those are rare enough that a missed warning beats a false one.
fn font_warnings(tree: &Tree, db: &fontdb::Database, path: &Path) -> Vec<String> {
    let mut missing = BTreeSet::new();
    collect_missing_fonts(tree.root(), db, &mut missing);
    if missing.is_empty() {
        return Vec::new();
    }
    let names = missing
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(", ");
    vec![format!(
        "{}: font {names} not found; substituted the default font",
        path.display()
    )]
}

fn collect_missing_fonts(group: &Group, db: &fontdb::Database, out: &mut BTreeSet<String>) {
    for node in group.children() {
        match node {
            Node::Group(g) => collect_missing_fonts(g, db, out),
            Node::Text(text) => {
                for chunk in text.chunks() {
                    for span in chunk.spans() {
                        for family in span.font().families() {
                            if let FontFamily::Named(name) = family {
                                if !is_generic(name) && !is_installed(name, db) {
                                    out.insert(name.clone());
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn is_generic(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    GENERIC_FAMILIES.contains(&lower.as_str())
}

/// Whether the family exists at all. Deliberately ignores weight, style and
/// stretch: a family installed without the exact weight still renders in the
/// right typeface, so warning about it would be misleading.
fn is_installed(name: &str, db: &fontdb::Database) -> bool {
    db.query(&fontdb::Query {
        families: &[fontdb::Family::Name(name)],
        ..fontdb::Query::default()
    })
    .is_some()
}

/// tiny-skia pixmaps are **premultiplied**; `image::RgbaImage` expects straight
/// alpha. Handing `pixmap.take()` straight to `from_raw` compiles, runs, and
/// silently darkens every semi-transparent pixel.
fn demultiply(pixmap: &Pixmap) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixmap.width() as usize * pixmap.height() as usize * 4);
    for px in pixmap.pixels() {
        let c = px.demultiply();
        out.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn opts(width: Option<u32>, height: Option<u32>) -> SquishOptions {
        SquishOptions {
            width,
            height,
            ..SquishOptions::default()
        }
    }

    /// A 200×100 SVG with a viewBox and a single opaque rect.
    fn wide_svg() -> &'static str {
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100" viewBox="0 0 200 100"><rect width="200" height="100" fill="red"/></svg>"#
    }

    #[test]
    fn renders_at_the_requested_width() {
        let (img, _warnings) = rasterize(
            wide_svg().as_bytes(),
            &opts(Some(800), None),
            &PathBuf::from("a.svg"),
        )
        .expect("render should succeed");
        assert_eq!((img.width(), img.height()), (800, 400));
    }

    #[test]
    fn renders_at_the_requested_height() {
        let (img, _warnings) = rasterize(
            wide_svg().as_bytes(),
            &opts(None, Some(400)),
            &PathBuf::from("a.svg"),
        )
        .expect("render should succeed");
        assert_eq!((img.width(), img.height()), (800, 400));
    }

    #[test]
    fn upscales_a_tiny_icon() {
        let icon = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24" fill="blue"/></svg>"#;
        let (img, _warnings) = rasterize(
            icon.as_bytes(),
            &opts(Some(512), None),
            &PathBuf::from("i.svg"),
        )
        .expect("render should succeed");
        assert_eq!((img.width(), img.height()), (512, 512));
    }

    #[test]
    fn no_size_requested_is_an_error() {
        let err = rasterize(
            wide_svg().as_bytes(),
            &opts(None, None),
            &PathBuf::from("a.svg"),
        )
        .unwrap_err();
        assert!(matches!(err, SquishError::MissingRenderSize { .. }));
        assert!(format!("{err}").contains("--width"));
    }

    #[test]
    fn no_declared_geometry_is_an_error() {
        let bare =
            r#"<svg xmlns="http://www.w3.org/2000/svg"><rect width="10" height="10"/></svg>"#;
        let err = rasterize(
            bare.as_bytes(),
            &opts(Some(512), None),
            &PathBuf::from("b.svg"),
        )
        .unwrap_err();
        assert!(matches!(err, SquishError::MissingRenderSize { .. }));
        assert!(format!("{err}").contains("viewBox"));
    }

    #[test]
    fn percentage_size_without_a_viewbox_is_an_error() {
        // Common in web SVGs: carries neither a size nor a ratio, and usvg
        // would silently substitute its 100×100 default.
        let pct = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100%" height="100%"><rect width="10" height="10"/></svg>"#;
        let err = rasterize(
            pct.as_bytes(),
            &opts(Some(512), None),
            &PathBuf::from("c.svg"),
        )
        .unwrap_err();
        assert!(matches!(err, SquishError::MissingRenderSize { .. }));
    }

    #[test]
    fn a_doctype_does_not_hide_the_geometry() {
        // roxmltree rejects DTDs unless allow_dtd is set; if the geometry probe
        // forgets that, every SVG with a DOCTYPE looks geometry-less.
        let dtd = concat!(
            r#"<?xml version="1.0"?>"#,
            r#"<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">"#,
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10" fill="red"/></svg>"#
        );
        let (img, _warnings) = rasterize(
            dtd.as_bytes(),
            &opts(Some(100), None),
            &PathBuf::from("d.svg"),
        )
        .expect("a DOCTYPE must not defeat the geometry probe");
        assert_eq!((img.width(), img.height()), (100, 100));
    }

    #[test]
    fn an_absurd_render_size_is_rejected_before_allocating() {
        let err = rasterize(
            wide_svg().as_bytes(),
            &opts(Some(100_000), None),
            &PathBuf::from("a.svg"),
        )
        .unwrap_err();
        assert!(matches!(err, SquishError::MissingRenderSize { .. }));
        assert!(format!("{err}").contains("pixel"));
    }

    #[test]
    fn semi_transparent_pixels_are_demultiplied() {
        // tiny-skia pixmaps are premultiplied; `image` expects straight alpha.
        // Skipping the demultiply step darkens this to (128, 0, 0, 128).
        let half = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 4"><rect width="4" height="4" fill="#ff0000" fill-opacity="0.5"/></svg>"##;
        let (img, _warnings) = rasterize(
            half.as_bytes(),
            &opts(Some(4), None),
            &PathBuf::from("h.svg"),
        )
        .expect("render should succeed");
        let px = img.to_rgba8().get_pixel(2, 2).0;
        assert_eq!(px[3], 128, "alpha should be ~50%: {px:?}");
        assert!(
            px[0] >= 250,
            "red must survive demultiplication, got {px:?} (premultiplied leak?)"
        );
        assert_eq!((px[1], px[2]), (0, 0), "no green/blue: {px:?}");
    }

    #[test]
    fn warns_about_a_font_this_machine_cannot_supply() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 50"><text x="0" y="20" font-family="Definitely Not An Installed Face" font-size="16">squish</text></svg>"#;
        let (_img, warnings) = rasterize(
            svg.as_bytes(),
            &opts(Some(200), None),
            &PathBuf::from("t.svg"),
        )
        .expect("render should still succeed");
        assert_eq!(warnings.len(), 1, "one warning per file: {warnings:?}");
        assert!(
            warnings[0].contains("Definitely Not An Installed Face"),
            "the warning must name the family: {warnings:?}"
        );
        assert!(warnings[0].contains("t.svg"), "and the file: {warnings:?}");
    }

    #[test]
    fn sans_serif_keyword_never_warns() {
        // The bare CSS keyword `sans-serif` parses to `FontFamily::SansSerif`
        // (svgtypes 0.16.1's font.rs, verified empirically below), never to
        // `FontFamily::Named("sans-serif")` — so this never reaches
        // `is_generic` at all; it's filtered one level up, by variant
        // matching in `collect_missing_fonts`. This guards that user-facing
        // behaviour (no warning for the generic keyword), and this must hold
        // on a CI box with almost no fonts installed.
        //
        // Coverage of `is_generic` itself lives in
        // `quoted_generic_family_name_never_warns` below, which forces the
        // `Named` arm via CSS quoting.
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 50"><text x="0" y="20" font-family="sans-serif" font-size="16">squish</text></svg>"#;
        let (_img, warnings) = rasterize(
            svg.as_bytes(),
            &opts(Some(200), None),
            &PathBuf::from("g.svg"),
        )
        .expect("render should succeed");
        assert!(
            warnings.is_empty(),
            "expected no warnings, got {warnings:?}"
        );
    }

    #[test]
    fn quoted_generic_family_name_never_warns() {
        // A *quoted* CSS family name is parsed as a literal name, not a
        // keyword: `font-family="'Monospace'"` arrives as
        // `FontFamily::Named("Monospace")`, confirmed empirically (printing
        // `span.font().families()` showed `Named("Monospace")`, never
        // `FontFamily::Monospace`). That takes the `Named` arm in
        // `collect_missing_fonts` and puts `is_generic`'s case-insensitive
        // check on the spot — unlike the bare keyword in
        // `sans_serif_keyword_never_warns`, which never reaches it. Without
        // `is_generic`, this family is a literal name absent from almost any
        // CI box's font set, and the test would fail (verified by
        // temporarily deleting the `is_generic` check: this test failed with
        // a "Monospace" warning while `is_generic` was gone).
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 50"><text x="0" y="20" font-family="'Monospace'" font-size="16">squish</text></svg>"#;
        let (_img, warnings) = rasterize(
            svg.as_bytes(),
            &opts(Some(200), None),
            &PathBuf::from("q.svg"),
        )
        .expect("render should succeed");
        assert!(
            warnings.is_empty(),
            "expected no warnings, got {warnings:?}"
        );
    }

    #[test]
    fn text_naming_no_family_never_warns() {
        // usvg injects `Options::font_family` into every span, so a concrete
        // fallback name would warn on any machine lacking that face.
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 50"><text x="0" y="20" font-size="16">squish</text></svg>"#;
        let (_img, warnings) = rasterize(
            svg.as_bytes(),
            &opts(Some(200), None),
            &PathBuf::from("n.svg"),
        )
        .expect("render should succeed");
        assert!(
            warnings.is_empty(),
            "expected no warnings, got {warnings:?}"
        );
    }

    #[test]
    fn an_svg_without_text_never_warns() {
        let (_img, warnings) = rasterize(
            wide_svg().as_bytes(),
            &opts(Some(200), None),
            &PathBuf::from("p.svg"),
        )
        .expect("render should succeed");
        assert!(
            warnings.is_empty(),
            "expected no warnings, got {warnings:?}"
        );
    }

    #[test]
    fn a_transparent_canvas_stays_transparent() {
        let empty = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><rect x="0" y="0" width="2" height="2" fill="black"/></svg>"#;
        let (img, _warnings) = rasterize(
            empty.as_bytes(),
            &opts(Some(8), None),
            &PathBuf::from("e.svg"),
        )
        .expect("render should succeed");
        assert_eq!(
            img.to_rgba8().get_pixel(7, 7).0[3],
            0,
            "corner is unpainted"
        );
    }
}
