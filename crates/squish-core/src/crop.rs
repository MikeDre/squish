//! Crop-zone types: parsing `--crop` specs and resolving them against a
//! concrete image size.

/// A requested crop, before resolution against a concrete image size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CropSpec {
    /// Largest rectangle of this aspect ratio that fits, anchored by gravity.
    Aspect { w: u32, h: u32 },
    /// Exact pixel rectangle: width x height at offset (x, y).
    Exact { w: u32, h: u32, x: u32, y: u32 },
}

/// Where an aspect-ratio crop is anchored within the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Gravity {
    #[default]
    Center,
    North,
    South,
    East,
    West,
    NorthWest,
    NorthEast,
    SouthWest,
    SouthEast,
}

/// A resolved pixel rectangle, guaranteed within image bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl CropSpec {
    /// Parse a `--crop` value: `W:H` (aspect, e.g. `16:9`) or `WxH+X+Y`
    /// (exact rect, e.g. `800x600+120+40`).
    pub fn parse(s: &str) -> Result<CropSpec, String> {
        let err =
            || format!("expected an aspect ratio like 16:9 or a rect like 800x600+120+40, got \"{s}\"");
        if let Some((w, h)) = s.split_once(':') {
            let w: u32 = w.parse().map_err(|_| err())?;
            let h: u32 = h.parse().map_err(|_| err())?;
            if w == 0 || h == 0 {
                return Err(format!("aspect ratio sides must be non-zero, got \"{s}\""));
            }
            return Ok(CropSpec::Aspect { w, h });
        }
        let (size, offsets) = s.split_once('+').ok_or_else(err)?;
        let (w, h) = size.split_once('x').ok_or_else(err)?;
        let (x, y) = offsets.split_once('+').ok_or_else(err)?;
        let w: u32 = w.parse().map_err(|_| err())?;
        let h: u32 = h.parse().map_err(|_| err())?;
        let x: u32 = x.parse().map_err(|_| err())?;
        let y: u32 = y.parse().map_err(|_| err())?;
        if w == 0 || h == 0 {
            return Err(format!("crop rect must have non-zero size, got \"{s}\""));
        }
        Ok(CropSpec::Exact { w, h, x, y })
    }

    /// Resolve against a concrete image size. `Ok(None)` means the crop is a
    /// no-op (it covers the full image, or the image has a zero dimension).
    /// `Err` explains why the spec cannot apply to this image.
    pub fn resolve(
        &self,
        gravity: Gravity,
        img_w: u32,
        img_h: u32,
    ) -> Result<Option<CropRect>, String> {
        if img_w == 0 || img_h == 0 {
            return Ok(None);
        }
        match *self {
            CropSpec::Aspect { w, h } => {
                // Largest w:h rect inside img_w × img_h, via u64 cross-products
                // so there is no float drift.
                let (rw, rh) = (w as u64, h as u64);
                let (iw, ih) = (img_w as u64, img_h as u64);
                let (cw, ch) = if iw * rh > ih * rw {
                    // Image is wider than the ratio: full height, narrower width.
                    ((ih * rw / rh).max(1) as u32, img_h)
                } else {
                    // Image is taller than (or matches) the ratio: full width.
                    (img_w, (iw * rh / rw).max(1) as u32)
                };
                if cw == img_w && ch == img_h {
                    return Ok(None);
                }
                let (x, y) = gravity.position(img_w - cw, img_h - ch);
                Ok(Some(CropRect { x, y, w: cw, h: ch }))
            }
            CropSpec::Exact { w, h, x, y } => {
                if x >= img_w || y >= img_h {
                    return Err(format!(
                        "crop offset +{x}+{y} lies outside the {img_w}x{img_h} image"
                    ));
                }
                let cw = w.min(img_w - x);
                let ch = h.min(img_h - y);
                if x == 0 && y == 0 && cw == img_w && ch == img_h {
                    return Ok(None);
                }
                Ok(Some(CropRect { x, y, w: cw, h: ch }))
            }
        }
    }
}

impl Gravity {
    /// Parse a `--gravity` value (case-insensitive compass names).
    pub fn parse(s: &str) -> Result<Gravity, String> {
        match s.to_ascii_lowercase().as_str() {
            "center" => Ok(Gravity::Center),
            "north" => Ok(Gravity::North),
            "south" => Ok(Gravity::South),
            "east" => Ok(Gravity::East),
            "west" => Ok(Gravity::West),
            "northwest" => Ok(Gravity::NorthWest),
            "northeast" => Ok(Gravity::NorthEast),
            "southwest" => Ok(Gravity::SouthWest),
            "southeast" => Ok(Gravity::SouthEast),
            _ => Err(format!(
                "expected one of center, north, south, east, west, northwest, \
                 northeast, southwest, southeast; got \"{s}\""
            )),
        }
    }

    /// Place a rect anchored by this gravity, given the leftover space on
    /// each axis (image size minus crop size).
    fn position(self, slack_x: u32, slack_y: u32) -> (u32, u32) {
        let (hx, hy) = (slack_x / 2, slack_y / 2);
        match self {
            Gravity::Center => (hx, hy),
            Gravity::North => (hx, 0),
            Gravity::South => (hx, slack_y),
            Gravity::West => (0, hy),
            Gravity::East => (slack_x, hy),
            Gravity::NorthWest => (0, 0),
            Gravity::NorthEast => (slack_x, 0),
            Gravity::SouthWest => (0, slack_y),
            Gravity::SouthEast => (slack_x, slack_y),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parsing ---

    #[test]
    fn parses_aspect_ratio() {
        assert_eq!(CropSpec::parse("16:9").unwrap(), CropSpec::Aspect { w: 16, h: 9 });
        assert_eq!(CropSpec::parse("1:1").unwrap(), CropSpec::Aspect { w: 1, h: 1 });
    }

    #[test]
    fn parses_exact_rect() {
        assert_eq!(
            CropSpec::parse("800x600+120+40").unwrap(),
            CropSpec::Exact { w: 800, h: 600, x: 120, y: 40 }
        );
    }

    #[test]
    fn rejects_malformed_specs() {
        for bad in ["", "16", "16:", ":9", "0:9", "16:0", "800x600", "800x600+5",
                    "x600+1+1", "800x+1+1", "0x600+1+1", "800x0+1+1", "16:9:4", "a:b"] {
            assert!(CropSpec::parse(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn parses_gravity_names() {
        assert_eq!(Gravity::parse("center").unwrap(), Gravity::Center);
        assert_eq!(Gravity::parse("NORTH").unwrap(), Gravity::North);
        assert_eq!(Gravity::parse("southwest").unwrap(), Gravity::SouthWest);
        assert!(Gravity::parse("middle").is_err());
    }

    // --- aspect resolution ---

    #[test]
    fn aspect_square_on_landscape_crops_width() {
        let r = CropSpec::Aspect { w: 1, h: 1 }
            .resolve(Gravity::Center, 640, 480).unwrap().unwrap();
        assert_eq!((r.x, r.y, r.w, r.h), (80, 0, 480, 480));
    }

    #[test]
    fn aspect_16_9_on_4_3_crops_height() {
        let r = CropSpec::Aspect { w: 16, h: 9 }
            .resolve(Gravity::Center, 640, 480).unwrap().unwrap();
        assert_eq!((r.x, r.y, r.w, r.h), (0, 60, 640, 360));
    }

    #[test]
    fn aspect_matching_image_is_noop() {
        assert_eq!(
            CropSpec::Aspect { w: 4, h: 3 }.resolve(Gravity::Center, 640, 480).unwrap(),
            None
        );
        // Equivalent ratio, different terms
        assert_eq!(
            CropSpec::Aspect { w: 8, h: 6 }.resolve(Gravity::Center, 640, 480).unwrap(),
            None
        );
    }

    #[test]
    fn gravity_anchors_aspect_crop() {
        let spec = CropSpec::Aspect { w: 1, h: 1 }; // 480x480 inside 640x480, slack_x=160
        let at = |g| spec.resolve(g, 640, 480).unwrap().unwrap();
        assert_eq!((at(Gravity::West).x, at(Gravity::West).y), (0, 0));
        assert_eq!((at(Gravity::East).x, at(Gravity::East).y), (160, 0));
        assert_eq!((at(Gravity::Center).x, at(Gravity::Center).y), (80, 0));
        // Vertical slack: 16:9 on 640x480 → 640x360, slack_y=120
        let tall = CropSpec::Aspect { w: 16, h: 9 };
        assert_eq!(tall.resolve(Gravity::North, 640, 480).unwrap().unwrap().y, 0);
        assert_eq!(tall.resolve(Gravity::South, 640, 480).unwrap().unwrap().y, 120);
        assert_eq!(tall.resolve(Gravity::SouthEast, 640, 480).unwrap().unwrap().y, 120);
    }

    // --- exact resolution ---

    #[test]
    fn exact_rect_passes_through_in_bounds() {
        let r = CropSpec::Exact { w: 300, h: 200, x: 10, y: 20 }
            .resolve(Gravity::Center, 640, 480).unwrap().unwrap();
        assert_eq!((r.x, r.y, r.w, r.h), (10, 20, 300, 200));
    }

    #[test]
    fn exact_rect_clamps_overhang() {
        let r = CropSpec::Exact { w: 9999, h: 9999, x: 600, y: 400 }
            .resolve(Gravity::Center, 640, 480).unwrap().unwrap();
        assert_eq!((r.x, r.y, r.w, r.h), (600, 400, 40, 80));
    }

    #[test]
    fn exact_rect_outside_image_is_error() {
        assert!(CropSpec::Exact { w: 10, h: 10, x: 640, y: 0 }
            .resolve(Gravity::Center, 640, 480).is_err());
        assert!(CropSpec::Exact { w: 10, h: 10, x: 0, y: 480 }
            .resolve(Gravity::Center, 640, 480).is_err());
    }

    #[test]
    fn exact_full_image_is_noop() {
        assert_eq!(
            CropSpec::Exact { w: 640, h: 480, x: 0, y: 0 }
                .resolve(Gravity::Center, 640, 480).unwrap(),
            None
        );
    }

    #[test]
    fn zero_sized_image_is_noop() {
        assert_eq!(
            CropSpec::Aspect { w: 1, h: 1 }.resolve(Gravity::Center, 0, 480).unwrap(),
            None
        );
    }
}
