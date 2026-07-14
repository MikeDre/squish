use crate::error::SquishError;
use crate::options::SquishOptions;
use image::{DynamicImage, GenericImageView};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Compress a GIF by shelling out to `gifsicle`. Handles both static and animated.
/// Requires `gifsicle` on PATH — returns [`SquishError::MissingDependency`] otherwise.
pub fn compress(input: &[u8], opts: &SquishOptions, path: &Path) -> Result<Vec<u8>, SquishError> {
    if which_binary("gifsicle").is_none() {
        return Err(SquishError::MissingDependency {
            name: "gifsicle".into(),
            install_hint: "brew install gifsicle (macOS) / apt install gifsicle (Linux)".into(),
        });
    }
    let crop = match opts.crop {
        None => None,
        Some(spec) => {
            let (w, h) = gif_dimensions(input).ok_or_else(|| SquishError::DecodeFailed {
                path: path.to_path_buf(),
                source: "could not read GIF dimensions for --crop".into(),
            })?;
            spec.resolve(opts.gravity, w, h)
                .map_err(|reason| SquishError::InvalidCrop {
                    path: path.to_path_buf(),
                    reason,
                })?
        }
    };
    optimize_via_gifsicle(input, opts, path, crop)
}

/// Encode an already-decoded raster as a single-frame GIF. Used for cross-format
/// conversion (e.g. PNG → GIF). Note: animation is only preserved on GIF → GIF.
pub fn encode_raster(
    img: &DynamicImage,
    opts: &SquishOptions,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    let (w, h) = img.dimensions();
    let rgba = img.to_rgba8().into_raw();

    let mut gif_bytes: Vec<u8> = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut gif_bytes);
        let frame =
            image::Frame::new(image::ImageBuffer::from_raw(w, h, rgba).ok_or_else(|| {
                SquishError::EncodeFailed {
                    path: path.to_path_buf(),
                    source: "failed to allocate GIF frame buffer".into(),
                }
            })?);
        encoder
            .encode_frame(frame)
            .map_err(|e| SquishError::EncodeFailed {
                path: path.to_path_buf(),
                source: Box::new(e),
            })?;
    }

    // Run it through gifsicle for the same optimization pass as native GIF input.
    // If gifsicle is missing, return the unoptimized GIF rather than failing —
    // the caller explicitly asked for GIF, so we shouldn't block on a missing tool.
    if which_binary("gifsicle").is_some() {
        // Crop (if any) was already applied to the decoded raster upstream.
        optimize_via_gifsicle(&gif_bytes, opts, path, None)
    } else {
        Ok(gif_bytes)
    }
}

fn optimize_via_gifsicle(
    input: &[u8],
    opts: &SquishOptions,
    path: &Path,
    crop: Option<crate::crop::CropRect>,
) -> Result<Vec<u8>, SquishError> {
    let mut cmd = Command::new("gifsicle");
    cmd.arg("-O3")
        .arg("--no-comments")
        .arg("--no-names")
        .arg("--no-extensions");

    if !opts.lossless {
        // Map quality (0-100, higher = better) to gifsicle lossy (0-200, higher = worse).
        let q = opts.effective_quality(crate::format::Format::Gif);
        let lossy = (100 - q as u32) * 2;
        cmd.arg(format!("--lossy={lossy}"));
    }

    if let Some(r) = crop {
        // gifsicle crops every input named after the flag; stdin counts.
        cmd.arg(format!("--crop={},{}+{}x{}", r.x, r.y, r.w, r.h));
    }

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| SquishError::EncodeFailed {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input)
        .map_err(|e| SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;

    let output = child
        .wait_with_output()
        .map_err(|e| SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(SquishError::EncodeFailed {
            path: path.to_path_buf(),
            source: format!("gifsicle exited {}: {}", output.status, stderr).into(),
        });
    }

    Ok(output.stdout)
}

/// Read the logical-screen dimensions from a GIF header (bytes 6..10,
/// two little-endian u16s). Returns None for anything too short or not GIF.
fn gif_dimensions(input: &[u8]) -> Option<(u32, u32)> {
    if input.len() < 10 || !input.starts_with(b"GIF") {
        return None;
    }
    let w = u16::from_le_bytes([input[6], input[7]]) as u32;
    let h = u16::from_le_bytes([input[8], input[9]]) as u32;
    if w == 0 || h == 0 {
        None
    } else {
        Some((w, h))
    }
}

/// Cross-platform `which` — return Some(path) if binary is on PATH.
fn which_binary(name: &str) -> Option<std::path::PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let with_exe = dir.join(format!("{name}.exe"));
            if with_exe.is_file() {
                return Some(with_exe);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gif_dimensions_reads_logical_screen() {
        // GIF89a header, 640x480 logical screen (little-endian u16 pairs).
        let mut header = b"GIF89a".to_vec();
        header.extend_from_slice(&640u16.to_le_bytes());
        header.extend_from_slice(&480u16.to_le_bytes());
        header.extend_from_slice(&[0, 0, 0]); // rest of the descriptor
        assert_eq!(gif_dimensions(&header), Some((640, 480)));
    }

    #[test]
    fn gif_dimensions_rejects_non_gif() {
        assert_eq!(gif_dimensions(b"PNG whatever"), None);
        assert_eq!(gif_dimensions(b"GIF89a"), None); // too short
    }
}
