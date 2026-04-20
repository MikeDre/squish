use crate::error::SquishError;
use crate::options::SquishOptions;
use image::{DynamicImage, GenericImageView};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Compress a GIF by shelling out to `gifsicle`. Handles both static and animated.
/// Requires `gifsicle` on PATH — returns [`SquishError::MissingDependency`] otherwise.
pub fn compress(
    input: &[u8],
    opts: &SquishOptions,
    path: &Path,
) -> Result<Vec<u8>, SquishError> {
    if which_binary("gifsicle").is_none() {
        return Err(SquishError::MissingDependency {
            name: "gifsicle".into(),
            install_hint: "brew install gifsicle (macOS) / apt install gifsicle (Linux)".into(),
        });
    }
    optimize_via_gifsicle(input, opts, path)
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
        let frame = image::Frame::new(
            image::ImageBuffer::from_raw(w, h, rgba).ok_or_else(|| SquishError::EncodeFailed {
                path: path.to_path_buf(),
                source: "failed to allocate GIF frame buffer".into(),
            })?,
        );
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
        optimize_via_gifsicle(&gif_bytes, opts, path)
    } else {
        Ok(gif_bytes)
    }
}

fn optimize_via_gifsicle(
    input: &[u8],
    opts: &SquishOptions,
    path: &Path,
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
