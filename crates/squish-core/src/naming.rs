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
pub fn derive_output_path(input: &Path, target_ext: &str, force_overwrite: bool) -> PathBuf {
    derive_output_path_with_suffix(input, target_ext, force_overwrite, "squished")
}

/// Like `derive_output_path` but with a custom suffix instead of "squished".
/// Uses `_` as the separator between stem, suffix, and collision counter.
pub fn derive_output_path_with_suffix(
    input: &Path,
    target_ext: &str,
    force_overwrite: bool,
    suffix: &str,
) -> PathBuf {
    derive_output_path_with_suffix_sep(input, target_ext, force_overwrite, suffix, '_')
}

/// Like `derive_output_path_with_suffix` but with a custom separator
/// between stem, suffix, and collision counter. Use `_` for media (default)
/// and `.` for code minification (e.g. `app.min.js`).
pub fn derive_output_path_with_suffix_sep(
    input: &Path,
    target_ext: &str,
    force_overwrite: bool,
    suffix: &str,
    separator: char,
) -> PathBuf {
    let parent = input.parent().unwrap_or_else(|| Path::new(""));
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let base = parent.join(format!("{stem}{separator}{suffix}.{target_ext}"));
    if force_overwrite || !base.exists() {
        return base;
    }

    for n in 2u32.. {
        let candidate = parent.join(format!(
            "{stem}{separator}{suffix}{separator}{n}.{target_ext}"
        ));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("u32 exhausted")
}

/// Returns the in-place overwrite target (the input path itself) when the
/// requested `output_ext` matches the input's extension (case-insensitive),
/// or `None` when they differ — the caller must refuse, because overwriting in
/// place would change the file's extension.
pub fn in_place_target(input: &Path, output_ext: &str) -> Option<PathBuf> {
    let input_ext = input
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    if !input_ext.is_empty() && input_ext == output_ext.to_ascii_lowercase() {
        Some(input.to_path_buf())
    } else {
        None
    }
}

/// A unique sibling temp path next to `target`, in the same directory so a
/// later `rename` onto `target` is atomic. Encodes pid + nanos for uniqueness.
pub fn in_place_temp_path(target: &Path) -> PathBuf {
    let parent = target.parent().unwrap_or_else(|| Path::new(""));
    let stem = target.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
    let ext = target.extension().and_then(|e| e.to_str()).unwrap_or("");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    parent.join(format!(".{stem}.{ext}.sq-{pid}-{nanos}.tmp"))
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
    fn custom_suffix() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("dog.png");
        let out = derive_output_path_with_suffix(&input, "png", false, "compressed");
        assert_eq!(out, tmp.path().join("dog_compressed.png"));
    }

    #[test]
    fn custom_suffix_collision() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("dog.png");
        fs::write(tmp.path().join("dog_min.png"), b"x").unwrap();
        let out = derive_output_path_with_suffix(&input, "png", false, "min");
        assert_eq!(out, tmp.path().join("dog_min_2.png"));
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

    #[test]
    fn derives_with_dot_separator() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("app.js");
        let out = derive_output_path_with_suffix_sep(&input, "js", false, "min", '.');
        assert_eq!(out, tmp.path().join("app.min.js"));
    }

    #[test]
    fn derives_with_underscore_via_default_unchanged() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("dog.png");
        let out = derive_output_path_with_suffix(&input, "png", false, "squished");
        assert_eq!(out, tmp.path().join("dog_squished.png"));
    }

    #[test]
    fn dot_separator_collision_uses_dot() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("app.js");
        fs::write(tmp.path().join("app.min.js"), b"x").unwrap();

        let out = derive_output_path_with_suffix_sep(&input, "js", false, "min", '.');
        assert_eq!(out, tmp.path().join("app.min.2.js"));
    }

    #[test]
    fn in_place_target_matches_extension() {
        let p = Path::new("/dir/clip.mp4");
        assert_eq!(in_place_target(p, "mp4"), Some(PathBuf::from("/dir/clip.mp4")));
    }

    #[test]
    fn in_place_target_is_case_insensitive() {
        let p = Path::new("/dir/clip.MP4");
        assert_eq!(in_place_target(p, "mp4"), Some(PathBuf::from("/dir/clip.MP4")));
    }

    #[test]
    fn in_place_target_refuses_on_ext_change() {
        let p = Path::new("/dir/clip.dv");
        assert_eq!(in_place_target(p, "mp4"), None);
    }

    #[test]
    fn in_place_target_refuses_when_no_extension() {
        let p = Path::new("/dir/clip");
        assert_eq!(in_place_target(p, "mp4"), None);
    }

    #[test]
    fn in_place_temp_path_is_unique_sibling() {
        let target = Path::new("/dir/clip.mp4");
        let tmp = in_place_temp_path(target);
        assert_eq!(tmp.parent(), target.parent());
        assert_ne!(tmp, target.to_path_buf());
        assert_eq!(tmp.extension().and_then(|e| e.to_str()), Some("tmp"));
    }
}
