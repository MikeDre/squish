//! Image metadata (EXIF/ICC) and orientation-correctness tests.
//!
//! `exif_sample.jpg` (60x40) was generated with Pillow: EXIF Orientation=6
//! (rotate 90° CW needed to display upright), a `Make` tag, a GPS IFD
//! (latitude/longitude), and a fake ICC profile. `exif_sample.png` (60x40)
//! has the same `Make` tag and ICC profile (PNG has no orientation
//! convention worth testing — see png.rs's doc comment). See git history for
//! the generating script if either fixture ever needs regenerating.
//!
//! Verification reads the *output* independently via `image`'s own JPEG/PNG
//! decoders (a different code path than squish-core's mozjpeg-marker-based
//! or oxipng-chunk-based handling), so these tests don't just check squish's
//! own bookkeeping.

use image::codecs::jpeg::JpegDecoder;
use image::metadata::Orientation;
use image::ImageDecoder;
use squish_core::{squish_file, SquishOptions};
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use tempfile::TempDir;

fn fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures");
    p.push(name);
    p
}

fn copy_fixture(name: &str) -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let dst = tmp.path().join(name);
    fs::copy(fixture(name), &dst).unwrap();
    (tmp, dst)
}

/// (exif TIFF chunk with prefix stripped, icc profile), both `None` if absent.
fn read_output_metadata(path: &std::path::Path) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let bytes = fs::read(path).unwrap();
    let mut decoder = JpegDecoder::new(Cursor::new(bytes)).unwrap();
    (
        decoder.exif_metadata().unwrap(),
        decoder.icc_profile().unwrap(),
    )
}

/// Same as `read_output_metadata`, but for a PNG output (eXIf/iCCP chunks).
fn read_output_png_metadata(path: &std::path::Path) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let bytes = fs::read(path).unwrap();
    let mut decoder = image::codecs::png::PngDecoder::new(Cursor::new(bytes)).unwrap();
    (
        decoder.exif_metadata().unwrap(),
        decoder.icc_profile().unwrap(),
    )
}

#[test]
fn default_strips_exif_but_preserves_icc_and_fixes_orientation() {
    let (_tmp, input) = copy_fixture("exif_sample.jpg");
    let r = squish_file(&input, &SquishOptions::default()).unwrap();

    // Orientation 6 (rotate 90 CW) on a 60x40 source must land as 40x60.
    assert_eq!(
        image::image_dimensions(&r.output_path).unwrap(),
        (40, 60),
        "orientation was not applied to pixels"
    );

    let (exif, icc) = read_output_metadata(&r.output_path);
    assert!(exif.is_none(), "EXIF should be stripped by default");
    assert!(icc.is_some(), "ICC profile should be preserved by default");
    assert_eq!(icc.unwrap(), b"fakeICCPROFILEDATA1234567890".to_vec());
}

#[test]
fn keep_metadata_preserves_exif_with_orientation_reset() {
    let (_tmp, input) = copy_fixture("exif_sample.jpg");
    let opts = SquishOptions {
        keep_metadata: true,
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();

    assert_eq!(image::image_dimensions(&r.output_path).unwrap(), (40, 60));

    let (exif, icc) = read_output_metadata(&r.output_path);
    let exif = exif.expect("--keep-metadata should preserve EXIF");
    assert!(icc.is_some(), "ICC profile should still be preserved");

    // The tags themselves survive (IFD0's Make = tag 0x010F, GPSInfo pointer
    // = tag 0x8825)...
    assert!(tiff_ifd0_has_tag(&exif, 0x010F), "Make tag should survive");
    assert!(
        tiff_ifd0_has_tag(&exif, 0x8825),
        "GPSInfo pointer should survive"
    );

    // ...but orientation must be reset to 1 (NoTransforms): the pixels are
    // already corrected, so a leftover non-1 tag would cause a compliant
    // viewer to rotate an already-upright image.
    assert_eq!(
        Orientation::from_exif_chunk(&exif),
        Some(Orientation::NoTransforms)
    );
}

// ----- PNG: lossless path (oxipng operates directly on source bytes) -----

#[test]
fn png_lossless_default_strips_exif_but_preserves_icc() {
    let (_tmp, input) = copy_fixture("exif_sample.png");
    let opts = SquishOptions {
        lossless: true,
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();

    let (exif, icc) = read_output_png_metadata(&r.output_path);
    assert!(exif.is_none(), "EXIF should be stripped by default");
    assert_eq!(
        icc.expect("ICC profile should be preserved by default"),
        b"fakeICCPROFILEDATA1234567890".to_vec()
    );
}

#[test]
fn png_lossless_keep_metadata_preserves_exif() {
    let (_tmp, input) = copy_fixture("exif_sample.png");
    let opts = SquishOptions {
        lossless: true,
        keep_metadata: true,
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();

    let (exif, icc) = read_output_png_metadata(&r.output_path);
    let exif = exif.expect("--keep-metadata should preserve EXIF");
    assert!(tiff_ifd0_has_tag(&exif, 0x010F), "Make tag should survive");
    assert!(icc.is_some());
}

// ----- PNG: lossy/quantized path (decode → imagequant → re-encode) -----

#[test]
fn png_lossy_default_strips_exif_but_preserves_icc() {
    let (_tmp, input) = copy_fixture("exif_sample.png");
    let r = squish_file(&input, &SquishOptions::default()).unwrap();

    let (exif, icc) = read_output_png_metadata(&r.output_path);
    assert!(exif.is_none(), "EXIF should be stripped by default");
    assert_eq!(
        icc.expect("ICC profile should be preserved by default"),
        b"fakeICCPROFILEDATA1234567890".to_vec()
    );
}

#[test]
fn png_lossy_keep_metadata_preserves_exif() {
    let (_tmp, input) = copy_fixture("exif_sample.png");
    let opts = SquishOptions {
        keep_metadata: true,
        ..Default::default()
    };
    let r = squish_file(&input, &opts).unwrap();

    let (exif, icc) = read_output_png_metadata(&r.output_path);
    let exif = exif.expect("--keep-metadata should preserve EXIF");
    assert!(tiff_ifd0_has_tag(&exif, 0x010F), "Make tag should survive");
    assert!(icc.is_some());
}

/// Minimal TIFF IFD0 tag-presence check: does `tiff` (a TIFF-structured EXIF
/// chunk, "Exif\0\0" prefix already stripped, as returned by
/// `image::ImageDecoder::exif_metadata`) contain an IFD0 entry for `tag`?
/// Deliberately hand-rolled rather than pulling in a full EXIF-parsing crate
/// just to check two tag IDs in a test.
fn tiff_ifd0_has_tag(tiff: &[u8], tag: u16) -> bool {
    let big_endian = match &tiff[0..4] {
        [0x49, 0x49, 42, 0] => false,
        [0x4d, 0x4d, 0, 42] => true,
        _ => panic!("not a TIFF header"),
    };
    let u16_at = |o: usize| -> u16 {
        let b = [tiff[o], tiff[o + 1]];
        if big_endian {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        }
    };
    let u32_at = |o: usize| -> u32 {
        let b = [tiff[o], tiff[o + 1], tiff[o + 2], tiff[o + 3]];
        if big_endian {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        }
    };

    let ifd0_offset = u32_at(4) as usize;
    let entry_count = u16_at(ifd0_offset) as usize;
    (0..entry_count).any(|i| {
        let entry_offset = ifd0_offset + 2 + i * 12;
        u16_at(entry_offset) == tag
    })
}
