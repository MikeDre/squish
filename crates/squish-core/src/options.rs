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

    /// Maximum output width in pixels. Images wider than this are scaled down
    /// proportionally. `None` means no width constraint. Never upscales.
    pub max_width: Option<u32>,

    /// Maximum output height in pixels. Images taller than this are scaled down
    /// proportionally. `None` means no height constraint. Never upscales.
    pub max_height: Option<u32>,
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

    /// Whether a resize is requested.
    pub fn needs_resize(&self) -> bool {
        self.max_width.is_some() || self.max_height.is_some()
    }

    /// Compute the target dimensions for a given input size, respecting
    /// max_width/max_height constraints. Returns `None` if no resize needed
    /// (image already fits or no constraints set). Never upscales.
    pub fn resize_dimensions(&self, width: u32, height: u32) -> Option<(u32, u32)> {
        if width == 0 || height == 0 {
            return None;
        }

        let scale_w = self.max_width
            .map(|mw| if width > mw { mw as f64 / width as f64 } else { 1.0 })
            .unwrap_or(1.0);

        let scale_h = self.max_height
            .map(|mh| if height > mh { mh as f64 / height as f64 } else { 1.0 })
            .unwrap_or(1.0);

        let scale = scale_w.min(scale_h);

        if scale >= 1.0 {
            return None; // already fits, don't upscale
        }

        let new_w = (width as f64 * scale).round() as u32;
        let new_h = (height as f64 * scale).round() as u32;
        Some((new_w.max(1), new_h.max(1)))
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
        assert!(o.max_width.is_none());
        assert!(o.max_height.is_none());
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

    #[test]
    fn resize_no_constraints_returns_none() {
        let o = SquishOptions::default();
        assert_eq!(o.resize_dimensions(4000, 3000), None);
    }

    #[test]
    fn resize_within_bounds_returns_none() {
        let o = SquishOptions { max_width: Some(2000), max_height: Some(2000), ..Default::default() };
        assert_eq!(o.resize_dimensions(1000, 800), None);
    }

    #[test]
    fn resize_width_constrained() {
        let o = SquishOptions { max_width: Some(2000), ..Default::default() };
        let (w, h) = o.resize_dimensions(4000, 3000).unwrap();
        assert_eq!(w, 2000);
        assert_eq!(h, 1500);
    }

    #[test]
    fn resize_height_constrained() {
        let o = SquishOptions { max_height: Some(1000), ..Default::default() };
        let (w, h) = o.resize_dimensions(4000, 3000).unwrap();
        // scale = 1000/3000 = 0.333
        assert_eq!(w, 1333);
        assert_eq!(h, 1000);
    }

    #[test]
    fn resize_both_constraints_uses_more_limiting() {
        let o = SquishOptions { max_width: Some(2000), max_height: Some(1000), ..Default::default() };
        let (w, h) = o.resize_dimensions(4000, 3000).unwrap();
        // width scale = 0.5, height scale = 0.333 → use 0.333
        assert_eq!(w, 1333);
        assert_eq!(h, 1000);
    }

    #[test]
    fn resize_never_upscales() {
        let o = SquishOptions { max_width: Some(5000), max_height: Some(5000), ..Default::default() };
        assert_eq!(o.resize_dimensions(1000, 800), None);
    }

    #[test]
    fn needs_resize_false_by_default() {
        assert!(!SquishOptions::default().needs_resize());
    }

    #[test]
    fn needs_resize_true_with_max_width() {
        let o = SquishOptions { max_width: Some(2000), ..Default::default() };
        assert!(o.needs_resize());
    }
}
