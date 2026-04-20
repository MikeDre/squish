use crate::error::SquishError;
use crate::options::SquishOptions;
use image::{DynamicImage, GenericImageView};
use libheif_rs::{
    Channel, ColorSpace, CompressionFormat, EncoderQuality, HeifContext, Image, LibHeif, RgbChroma,
};
use std::path::Path;

pub fn compress(
    input: &[u8],
    opts: &SquishOptions,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    let lib = LibHeif::new();

    let ctx = HeifContext::read_from_bytes(input).map_err(|e| SquishError::DecodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;
    let handle = ctx.primary_image_handle().map_err(|e| SquishError::DecodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;
    let image = lib
        .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)
        .map_err(|e| SquishError::DecodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;

    encode_heif_image(&lib, image, opts, path)
}

/// Encode an already-decoded raster as HEIC.
pub fn encode_raster(
    img: &DynamicImage,
    opts: &SquishOptions,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    let (w, h) = img.dimensions();
    let rgba = img.to_rgba8().into_raw();

    let mut heif_image = Image::new(w, h, ColorSpace::Rgb(RgbChroma::Rgba))
        .map_err(|e| SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
    heif_image
        .create_plane(Channel::Interleaved, w, h, 8)
        .map_err(|e| SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;

    {
        let mut planes = heif_image.planes_mut();
        let plane = planes.interleaved.as_mut().ok_or_else(|| SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: "missing interleaved plane after create_plane".into(),
        })?;
        let stride = plane.stride;
        let row_bytes = (w as usize) * 4;
        for y in 0..(h as usize) {
            let src_start = y * row_bytes;
            let dst_start = y * stride;
            plane.data[dst_start..dst_start + row_bytes]
                .copy_from_slice(&rgba[src_start..src_start + row_bytes]);
        }
    }

    let lib = LibHeif::new();
    encode_heif_image(&lib, heif_image, opts, path)
}

fn encode_heif_image(
    lib: &LibHeif,
    image: Image,
    opts: &SquishOptions,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    let mut out_ctx = HeifContext::new().map_err(|e| SquishError::EncodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;

    let mut encoder = lib
        .encoder_for_format(CompressionFormat::Hevc)
        .map_err(|e| SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;

    let q = opts.effective_quality(crate::format::Format::Heic);
    if opts.lossless {
        encoder
            .set_quality(EncoderQuality::LossLess)
            .map_err(|e| SquishError::EncodeFailed { path: path.to_path_buf(), source: Box::new(e) })?;
    } else {
        encoder
            .set_quality(EncoderQuality::Lossy(q))
            .map_err(|e| SquishError::EncodeFailed { path: path.to_path_buf(), source: Box::new(e) })?;
    }

    out_ctx
        .encode_image(&image, &mut encoder, None)
        .map_err(|e| SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;

    out_ctx.write_to_bytes().map_err(|e| SquishError::EncodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })
}
