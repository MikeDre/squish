use std::path::Path;

/// Supported image formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Png,
    Jpeg,
    Webp,
    Avif,
    Svg,
    Gif,
    Heic,
    Tiff,
}

impl Format {
    /// Canonical lowercase extension (no leading dot).
    pub fn extension(&self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Webp => "webp",
            Format::Avif => "avif",
            Format::Svg => "svg",
            Format::Gif => "gif",
            Format::Heic => "heic",
            Format::Tiff => "tiff",
        }
    }

    /// Parse from a user-provided string (e.g., `--format webp`).
    /// Accepts `jpg` and `jpeg` as synonyms.
    pub fn parse(s: &str) -> Option<Format> {
        match s.to_ascii_lowercase().as_str() {
            "png" => Some(Format::Png),
            "jpg" | "jpeg" => Some(Format::Jpeg),
            "webp" => Some(Format::Webp),
            "avif" => Some(Format::Avif),
            "svg" => Some(Format::Svg),
            "gif" => Some(Format::Gif),
            "heic" | "heif" => Some(Format::Heic),
            "tiff" | "tif" => Some(Format::Tiff),
            _ => None,
        }
    }
}

/// Detect format from path extension and the first bytes of the file.
/// Extension is primary; magic bytes are a fallback for mislabeled files.
/// Returns `None` for unrecognized types.
pub fn detect_format(path: &Path, head: &[u8]) -> Option<Format> {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if let Some(fmt) = Format::parse(ext) {
            return Some(fmt);
        }
    }
    detect_by_magic(head)
}

fn detect_by_magic(head: &[u8]) -> Option<Format> {
    // PNG: 89 50 4E 47 0D 0A 1A 0A
    if head.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(Format::Png);
    }
    // JPEG: FF D8 FF
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(Format::Jpeg);
    }
    // GIF: 47 49 46 38 (GIF8)
    if head.starts_with(b"GIF8") {
        return Some(Format::Gif);
    }
    // RIFF....WEBP (WebP is RIFF at 0, "WEBP" at 8)
    if head.len() >= 12 && head.starts_with(b"RIFF") && &head[8..12] == b"WEBP" {
        return Some(Format::Webp);
    }
    // AVIF: starts with ftyp box containing "avif"
    if head.len() >= 12 && &head[4..8] == b"ftyp" && &head[8..12] == b"avif" {
        return Some(Format::Avif);
    }
    // HEIC: ftyp box with "heic" / "heix" / "mif1"
    if head.len() >= 12 && &head[4..8] == b"ftyp" {
        let brand = &head[8..12];
        if brand == b"heic" || brand == b"heix" || brand == b"mif1" || brand == b"heim" {
            return Some(Format::Heic);
        }
    }
    // TIFF: 49 49 2A 00 (little-endian) or 4D 4D 00 2A (big-endian)
    if head.starts_with(&[0x49, 0x49, 0x2A, 0x00]) || head.starts_with(&[0x4D, 0x4D, 0x00, 0x2A]) {
        return Some(Format::Tiff);
    }
    // SVG: text-based, starts with `<?xml` or `<svg`
    if head.starts_with(b"<?xml") || head.starts_with(b"<svg") {
        return Some(Format::Svg);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parse_accepts_case_variants() {
        assert_eq!(Format::parse("PNG"), Some(Format::Png));
        assert_eq!(Format::parse("jpg"), Some(Format::Jpeg));
        assert_eq!(Format::parse("jpeg"), Some(Format::Jpeg));
        assert_eq!(Format::parse("JPEG"), Some(Format::Jpeg));
        assert_eq!(Format::parse("heif"), Some(Format::Heic));
        assert_eq!(Format::parse("tif"), Some(Format::Tiff));
        assert_eq!(Format::parse("bmp"), None);
    }

    #[test]
    fn extension_is_canonical() {
        assert_eq!(Format::Jpeg.extension(), "jpg");
        assert_eq!(Format::Tiff.extension(), "tiff");
    }

    #[test]
    fn detect_from_extension_png() {
        let path = PathBuf::from("x.png");
        assert_eq!(detect_format(&path, &[]), Some(Format::Png));
    }

    #[test]
    fn detect_from_extension_jpeg_both_spellings() {
        assert_eq!(detect_format(&PathBuf::from("x.jpg"), &[]), Some(Format::Jpeg));
        assert_eq!(detect_format(&PathBuf::from("x.jpeg"), &[]), Some(Format::Jpeg));
        assert_eq!(detect_format(&PathBuf::from("X.JPEG"), &[]), Some(Format::Jpeg));
    }

    #[test]
    fn detect_from_magic_png_when_ext_lies() {
        let head = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0];
        let path = PathBuf::from("lying.xyz");
        assert_eq!(detect_format(&path, &head), Some(Format::Png));
    }

    #[test]
    fn detect_from_magic_jpeg() {
        let head = [0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(detect_format(&PathBuf::from("x.xyz"), &head), Some(Format::Jpeg));
    }

    #[test]
    fn detect_from_magic_webp() {
        let mut head = [0u8; 12];
        head[0..4].copy_from_slice(b"RIFF");
        head[8..12].copy_from_slice(b"WEBP");
        assert_eq!(detect_format(&PathBuf::from("x.xyz"), &head), Some(Format::Webp));
    }

    #[test]
    fn detect_svg_from_xml_prologue() {
        assert_eq!(detect_format(&PathBuf::from("x"), b"<?xml version=\"1.0\"?>"), Some(Format::Svg));
        assert_eq!(detect_format(&PathBuf::from("x"), b"<svg xmlns=..."), Some(Format::Svg));
    }

    #[test]
    fn detect_returns_none_for_unknown() {
        assert_eq!(detect_format(&PathBuf::from("x.xyz"), b"random bytes"), None);
    }

    #[test]
    fn detect_tiff_both_endians() {
        assert_eq!(detect_format(&PathBuf::from("x"), &[0x49, 0x49, 0x2A, 0x00]), Some(Format::Tiff));
        assert_eq!(detect_format(&PathBuf::from("x"), &[0x4D, 0x4D, 0x00, 0x2A]), Some(Format::Tiff));
    }
}
