use ignore::overrides::OverrideBuilder;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

/// Directories always pruned from a recursive walk unless
/// `--no-default-excludes` is passed. Not applied to explicitly-named input
/// paths — only to directories discovered while walking.
const DEFAULT_EXCLUDES: &[&str] = &[".git", "node_modules", "target"];

/// Exclusion settings shared by the initial directory walk (`collect_worklist`)
/// and live watch-mode events (`watch::should_process`), so both apply the
/// same rules.
#[derive(Debug, Clone, Default)]
pub struct ExcludeOptions {
    /// User-supplied `--exclude` globs, matched relative to each input path's
    /// own root (see `collect_worklist`).
    pub globs: Vec<String>,
    /// `--gitignore`: also respect `.gitignore`/`.git/info/exclude`/the global
    /// gitignore while walking. Off by default so existing behavior doesn't
    /// change for anyone not asking for it.
    pub gitignore: bool,
    /// `--no-default-excludes`: don't prune `.git`/`node_modules`/`target`.
    pub no_default_excludes: bool,
}

/// Discover all candidate files from the provided input paths.
/// - Files: included if present, *never* excluded — explicit args always win,
///   even if they match a `--exclude` glob or a default-excluded name.
/// - Directories: walked (top-level only unless `recursive`), filtered by
///   `excludes`.
/// - Symlinks: not followed.
pub fn collect_worklist(
    inputs: &[PathBuf],
    recursive: bool,
    excludes: &ExcludeOptions,
) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for input in inputs {
        if input.is_file() {
            files.push(input.clone());
            continue;
        }
        if input.is_dir() {
            files.extend(walk_dir(input, recursive, excludes));
        }
    }
    files
}

/// Build the `.git`/`node_modules`/`target` + `--exclude` override set for
/// `root`. Shared by the directory walk and watch mode's single-path check
/// (`is_excluded`) so both apply identical rules.
fn build_overrides(root: &Path, excludes: &ExcludeOptions) -> ignore::overrides::Override {
    let mut builder = OverrideBuilder::new(root);
    if !excludes.no_default_excludes {
        for pat in DEFAULT_EXCLUDES {
            builder
                .add(&format!("!{pat}"))
                .expect("built-in exclude glob is always valid");
        }
    }
    for pat in &excludes.globs {
        if let Err(e) = builder.add(&format!("!{pat}")) {
            eprintln!("warning: ignoring invalid --exclude glob {pat:?}: {e}");
        }
    }
    builder
        .build()
        .expect("override globs were validated individually above")
}

/// True if a single path (e.g. a live watch-mode event) would be excluded by
/// `--exclude`/default-excludes rooted at `root`. Does not consult
/// `.gitignore` — that requires walking the tree, which a one-off event
/// doesn't do; `--gitignore` only affects the initial directory walk.
pub fn is_excluded(path: &Path, root: &Path, excludes: &ExcludeOptions) -> bool {
    let overrides = build_overrides(root, excludes);
    matches!(
        overrides.matched(path, path.is_dir()),
        ignore::Match::Ignore(_)
    )
}

fn walk_dir(root: &Path, recursive: bool, excludes: &ExcludeOptions) -> Vec<PathBuf> {
    let overrides = build_overrides(root, excludes);

    let walker = WalkBuilder::new(root)
        .follow_links(false)
        .hidden(false) // preserve pre-existing behavior: dotfiles are included
        .parents(false)
        .ignore(false) // don't honor ripgrep-style .ignore files, only gitignore
        .git_ignore(excludes.gitignore)
        .git_global(excludes.gitignore)
        .git_exclude(excludes.gitignore)
        // The ignore crate otherwise only honors .gitignore inside an actual
        // git repository (a real .git dir present); --gitignore should work
        // on any directory tree, git repo or not.
        .require_git(false)
        .overrides(overrides)
        .max_depth(if recursive { None } else { Some(1) })
        .build();

    walker
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_some_and(|ft| ft.is_file()))
        .map(ignore::DirEntry::into_path)
        .collect()
}

/// True if `path` looks like a file squish itself produced. UNUSED — the spec
/// intentionally treats these as regular inputs (design decision 3c=B).
#[allow(dead_code)]
pub fn looks_already_squished(path: &Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|stem| {
            stem.ends_with("_squished")
                || (stem
                    .rsplit_once("_squished_")
                    .is_some_and(|(_, suffix)| suffix.chars().all(|c| c.is_ascii_digit())))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn no_excludes() -> ExcludeOptions {
        ExcludeOptions::default()
    }

    #[test]
    fn single_file_returns_single_entry() {
        let tmp = TempDir::new().unwrap();
        let f = tmp.path().join("x.png");
        fs::write(&f, b"x").unwrap();
        assert_eq!(
            collect_worklist(std::slice::from_ref(&f), false, &no_excludes()),
            vec![f]
        );
    }

    #[test]
    fn directory_non_recursive_skips_subdirs() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.png"), b"x").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub/b.png"), b"x").unwrap();

        let list = collect_worklist(&[tmp.path().to_path_buf()], false, &no_excludes());
        assert_eq!(list.len(), 1);
        assert!(list[0].ends_with("a.png"));
    }

    #[test]
    fn directory_recursive_includes_subdirs() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.png"), b"x").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub/b.png"), b"x").unwrap();

        let list = collect_worklist(&[tmp.path().to_path_buf()], true, &no_excludes());
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn default_excludes_prune_git_node_modules_target() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.png"), b"x").unwrap();
        for dir in [".git", "node_modules", "target"] {
            fs::create_dir(tmp.path().join(dir)).unwrap();
            fs::write(tmp.path().join(dir).join("b.png"), b"x").unwrap();
        }

        let list = collect_worklist(&[tmp.path().to_path_buf()], true, &no_excludes());
        assert_eq!(list.len(), 1);
        assert!(list[0].ends_with("a.png"));
    }

    #[test]
    fn no_default_excludes_flag_restores_git_node_modules_target() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("node_modules")).unwrap();
        fs::write(tmp.path().join("node_modules/b.png"), b"x").unwrap();

        let opts = ExcludeOptions {
            no_default_excludes: true,
            ..Default::default()
        };
        let list = collect_worklist(&[tmp.path().to_path_buf()], true, &opts);
        assert_eq!(list.len(), 1);
        assert!(list[0].ends_with("b.png"));
    }

    #[test]
    fn custom_exclude_glob_is_pruned() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.png"), b"x").unwrap();
        fs::create_dir(tmp.path().join("vendor")).unwrap();
        fs::write(tmp.path().join("vendor/b.png"), b"x").unwrap();

        let opts = ExcludeOptions {
            globs: vec!["vendor/**".to_string()],
            ..Default::default()
        };
        let list = collect_worklist(&[tmp.path().to_path_buf()], true, &opts);
        assert_eq!(list.len(), 1);
        assert!(list[0].ends_with("a.png"));
    }

    #[test]
    fn explicit_file_arg_is_never_excluded() {
        // Even though the file matches a --exclude glob (and even sits
        // inside a default-excluded directory name), naming it directly on
        // the command line always wins.
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("node_modules");
        fs::create_dir(&dir).unwrap();
        let f = dir.join("a.min.js");
        fs::write(&f, b"x").unwrap();

        let opts = ExcludeOptions {
            globs: vec!["*.min.js".to_string()],
            ..Default::default()
        };
        let list = collect_worklist(std::slice::from_ref(&f), false, &opts);
        assert_eq!(list, vec![f]);
    }

    #[test]
    fn gitignore_flag_respects_dotgitignore() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(".gitignore"), "ignored.png\n").unwrap();
        fs::write(tmp.path().join("a.png"), b"x").unwrap();
        fs::write(tmp.path().join("ignored.png"), b"x").unwrap();

        // Default: .gitignore is not consulted, so all 3 files on disk turn
        // up (including .gitignore itself — the walker doesn't filter by
        // file type, that happens later during kind classification).
        let without = collect_worklist(&[tmp.path().to_path_buf()], true, &no_excludes());
        assert_eq!(without.len(), 3);

        let opts = ExcludeOptions {
            gitignore: true,
            ..Default::default()
        };
        let with = collect_worklist(&[tmp.path().to_path_buf()], true, &opts);
        assert_eq!(with.len(), 2);
        assert!(!with.iter().any(|p| p.ends_with("ignored.png")));
    }

    #[test]
    fn looks_already_squished_matches() {
        assert!(looks_already_squished(Path::new("dog_squished.png")));
        assert!(looks_already_squished(Path::new("dog_squished_2.png")));
        assert!(looks_already_squished(Path::new("dog_squished_99.png")));
        assert!(!looks_already_squished(Path::new("dog.png")));
        assert!(!looks_already_squished(Path::new(
            "_squished_notanumber.png"
        )));
    }
}
