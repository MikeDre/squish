use std::path::{Path, PathBuf};

/// Derive the output path for a compressed file.
///
/// Algorithm:
/// 1. Strip input extension. Append `_squished`. Append `.{target_ext}`.
/// 2. If that path doesn't exist, use it.
/// 3. If `force_overwrite`, use it anyway.
/// 4. Otherwise try `_squished_2`, `_squished_3`, … until one is free.
///
/// `target_ext` should be the desired output extension without the leading dot
/// (e.g., "png", "jpg", "webp"). When preserving input extension, pass the
/// original extension exactly — callers decide "jpg" vs "jpeg" case.
pub fn derive_output_path(
    input: &Path,
    target_ext: &str,
    force_overwrite: bool,
) -> PathBuf {
    let parent = input.parent().unwrap_or_else(|| Path::new(""));
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let base = parent.join(format!("{stem}_squished.{target_ext}"));
    if force_overwrite || !base.exists() {
        return base;
    }

    for n in 2u32.. {
        let candidate = parent.join(format!("{stem}_squished_{n}.{target_ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("u32 exhausted")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn basic_suffix_png() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("dog.png");
        let out = derive_output_path(&input, "png", false);
        assert_eq!(out, tmp.path().join("dog_squished.png"));
    }

    #[test]
    fn format_conversion_changes_extension() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("dog.png");
        let out = derive_output_path(&input, "webp", false);
        assert_eq!(out, tmp.path().join("dog_squished.webp"));
    }

    #[test]
    fn preserves_jpeg_spelling_when_caller_passes_jpeg() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("photo.jpeg");
        let out = derive_output_path(&input, "jpeg", false);
        assert_eq!(out, tmp.path().join("photo_squished.jpeg"));
    }

    #[test]
    fn collision_uses_numeric_suffix() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("dog.png");
        fs::write(tmp.path().join("dog_squished.png"), b"x").unwrap();

        let out = derive_output_path(&input, "png", false);
        assert_eq!(out, tmp.path().join("dog_squished_2.png"));
    }

    #[test]
    fn collision_increments_past_2() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("dog.png");
        fs::write(tmp.path().join("dog_squished.png"), b"x").unwrap();
        fs::write(tmp.path().join("dog_squished_2.png"), b"x").unwrap();
        fs::write(tmp.path().join("dog_squished_3.png"), b"x").unwrap();

        let out = derive_output_path(&input, "png", false);
        assert_eq!(out, tmp.path().join("dog_squished_4.png"));
    }

    #[test]
    fn force_overwrite_ignores_existing() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("dog.png");
        let target = tmp.path().join("dog_squished.png");
        fs::write(&target, b"x").unwrap();

        let out = derive_output_path(&input, "png", true);
        assert_eq!(out, target);
    }

    #[test]
    fn re_squish_produces_double_squished() {
        // With 3c=B (no filtering), a file already named *_squished.* is treated
        // as a regular input and produces *_squished_squished.*
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("dog_squished.png");
        let out = derive_output_path(&input, "png", false);
        assert_eq!(out, tmp.path().join("dog_squished_squished.png"));
    }
}
