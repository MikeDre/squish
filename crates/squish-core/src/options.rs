use crate::format::Format;

/// Compression options. None fields mean "use format default".
#[derive(Debug, Clone, Default)]
pub struct SquishOptions {
    /// Quality 0..=100. `None` means format default.
    /// Ignored when `lossless` is true.
    pub quality: Option<u8>,

    /// If true, use lossless compression. Overrides `quality`.
    pub lossless: bool,

    /// Output format. `None` means preserve input format.
    pub output_format: Option<Format>,

    /// If true, overwrite existing `_squished` outputs.
    /// If false, append a numeric suffix (`_squished_2`, `_squished_3`, …).
    pub force_overwrite: bool,
}

impl SquishOptions {
    /// Effective quality for a given format. Callers should use this rather than
    /// reading `self.quality` directly so per-format defaults stay consistent.
    pub fn effective_quality(&self, format: Format) -> u8 {
        if self.lossless {
            return 100;
        }
        self.quality.unwrap_or_else(|| default_quality(format))
    }
}

/// Sensible per-format defaults, chosen to match TinyPNG-style "it just works".
pub fn default_quality(format: Format) -> u8 {
    match format {
        Format::Png => 75,   // imagequant target
        Format::Jpeg => 80,  // mozjpeg default-ish
        Format::Webp => 80,
        Format::Avif => 65,  // AVIF is more efficient at lower numbers
        Format::Heic => 75,
        Format::Tiff => 80,  // TIFF → JPEG conversion uses JPEG quality
        Format::Svg | Format::Gif => 100, // lossless-only formats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_all_none() {
        let o = SquishOptions::default();
        assert!(o.quality.is_none());
        assert!(!o.lossless);
        assert!(o.output_format.is_none());
        assert!(!o.force_overwrite);
    }

    #[test]
    fn effective_quality_uses_override_when_set() {
        let o = SquishOptions { quality: Some(42), ..Default::default() };
        assert_eq!(o.effective_quality(Format::Jpeg), 42);
    }

    #[test]
    fn effective_quality_falls_back_to_default() {
        let o = SquishOptions::default();
        assert_eq!(o.effective_quality(Format::Jpeg), default_quality(Format::Jpeg));
    }

    #[test]
    fn lossless_forces_max_quality() {
        let o = SquishOptions { quality: Some(30), lossless: true, ..Default::default() };
        assert_eq!(o.effective_quality(Format::Webp), 100);
    }
}
