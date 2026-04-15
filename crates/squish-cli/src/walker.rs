use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Discover all candidate files from the provided input paths.
/// - Files: included if present.
/// - Directories: walked (top-level only unless `recursive`).
/// - Symlinks: not followed.
pub fn collect_worklist(inputs: &[PathBuf], recursive: bool) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for input in inputs {
        if input.is_file() {
            files.push(input.clone());
            continue;
        }
        if input.is_dir() {
            let walker = WalkDir::new(input)
                .follow_links(false)
                .max_depth(if recursive { usize::MAX } else { 1 });
            for entry in walker.into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() {
                    files.push(entry.into_path());
                }
            }
        }
    }
    files
}

/// True if `path` looks like a file squish itself produced. UNUSED — the spec
/// intentionally treats these as regular inputs (design decision 3c=B).
#[allow(dead_code)]
pub fn looks_already_squished(path: &Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|stem| {
            stem.ends_with("_squished")
                || (stem.rsplit_once("_squished_").map_or(false, |(_, suffix)| {
                    suffix.chars().all(|c| c.is_ascii_digit())
                }))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn single_file_returns_single_entry() {
        let tmp = TempDir::new().unwrap();
        let f = tmp.path().join("x.png");
        fs::write(&f, b"x").unwrap();
        assert_eq!(collect_worklist(&[f.clone()], false), vec![f]);
    }

    #[test]
    fn directory_non_recursive_skips_subdirs() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.png"), b"x").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub/b.png"), b"x").unwrap();

        let list = collect_worklist(&[tmp.path().to_path_buf()], false);
        assert_eq!(list.len(), 1);
        assert!(list[0].ends_with("a.png"));
    }

    #[test]
    fn directory_recursive_includes_subdirs() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.png"), b"x").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub/b.png"), b"x").unwrap();

        let list = collect_worklist(&[tmp.path().to_path_buf()], true);
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn looks_already_squished_matches() {
        assert!(looks_already_squished(Path::new("dog_squished.png")));
        assert!(looks_already_squished(Path::new("dog_squished_2.png")));
        assert!(looks_already_squished(Path::new("dog_squished_99.png")));
        assert!(!looks_already_squished(Path::new("dog.png")));
        assert!(!looks_already_squished(Path::new("_squished_notanumber.png")));
    }
}
