//! `--watch` mode: keep running, squish files as they appear or change.
//!
//! Two loop-prevention layers:
//! 1. Output-looking files (`*_squished.*`, custom suffixes, `*.min.*`,
//!    `*.map`) are never picked up, so squish never re-squishes its own
//!    siblings — even across restarts.
//! 2. Every path squish itself writes is remembered and its next watch event
//!    is swallowed once (covers `--overwrite`, where output == input).

use crate::runner::{self, RunConfig};
use crate::walker::{self, ExcludeOptions};
use anyhow::Result;
use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// True when `path` looks like something squish wrote: a `_<suffix>` /
/// `_<suffix>_N` sibling, a `.min.*` minified file, or a `.map` source map.
pub fn is_squish_output(path: &Path, suffix: &str) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    if name.ends_with(".map") {
        return true;
    }
    let stem = match name.rsplit_once('.') {
        Some((s, _ext)) => s,
        None => name,
    };
    // Minified code outputs: app.min.js → stem "app.min".
    if stem.ends_with(".min") {
        return true;
    }
    // Collision-numbered outputs: dog_squished_2 → dog_squished.
    let mut base = stem;
    if let Some((head, tail)) = base.rsplit_once('_') {
        if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
            base = head;
        }
    }
    match base.rsplit_once('_') {
        Some((prefix, suf)) => !prefix.is_empty() && suf == suffix,
        None => false,
    }
}

/// Decide whether a watch event for `path` should trigger a squish.
/// `written` is the set of paths squish itself produced; a hit consumes the
/// entry (skip once, react to later genuine edits). `roots` are the
/// user-supplied watch paths, used to root `--exclude` glob matching the
/// same way the initial directory walk does (see `walker::is_excluded`).
pub fn should_process(
    path: &Path,
    suffix: &str,
    written: &mut HashSet<PathBuf>,
    roots: &[PathBuf],
    excludes: &ExcludeOptions,
) -> bool {
    if written.remove(path) {
        return false;
    }
    if is_squish_output(path, suffix) {
        return false;
    }
    if !path.is_file() {
        return false;
    }
    let root: &Path = roots
        .iter()
        .map(PathBuf::as_path)
        .find(|r| path.starts_with(r))
        .unwrap_or_else(|| roots.first().map(PathBuf::as_path).unwrap_or(path));
    !walker::is_excluded(path, root, excludes)
}

/// Run an initial pass over `paths`, then watch them forever, squishing
/// changed/new files in debounced batches. Only returns on watcher error.
pub fn run_watch(
    paths: &[PathBuf],
    cfg: &RunConfig,
    recursive: bool,
    no_stats: bool,
    excludes: &ExcludeOptions,
) -> Result<()> {
    let suffix = cfg
        .opts
        .suffix
        .clone()
        .unwrap_or_else(|| "squished".to_string());
    let mut written: HashSet<PathBuf> = HashSet::new();

    // Initial pass over whatever already exists.
    let worklist = walker::collect_worklist(paths, recursive, excludes);
    if !worklist.is_empty() {
        let report = runner::run(&worklist, cfg)?;
        crate::stats::append_batch(&report, cfg.dry_run, no_stats);
        remember_outputs(&report, &mut written);
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(500), tx)?;
    let mode = if recursive {
        RecursiveMode::Recursive
    } else {
        RecursiveMode::NonRecursive
    };
    for p in paths {
        debouncer.watcher().watch(p, mode)?;
    }
    eprintln!("watching {} path(s) — press Ctrl-C to stop", paths.len());

    loop {
        let events = match rx.recv() {
            Ok(Ok(events)) => events,
            Ok(Err(e)) => {
                eprintln!("watch error: {e}");
                continue;
            }
            Err(_) => return Ok(()), // channel closed
        };

        let batch: Vec<PathBuf> = events
            .into_iter()
            .map(|e| e.path)
            .filter(|p| should_process(p, &suffix, &mut written, paths, excludes))
            .collect();
        if batch.is_empty() {
            continue;
        }

        match runner::run(&batch, cfg) {
            Ok(report) => {
                crate::stats::append_batch(&report, cfg.dry_run, no_stats);
                remember_outputs(&report, &mut written);
            }
            Err(e) => eprintln!("watch batch failed: {e}"),
        }
    }
}

fn remember_outputs(report: &runner::RunReport, written: &mut HashSet<PathBuf>) {
    for r in &report.results {
        written.insert(r.output_path.clone());
    }
    for r in &report.video_results {
        written.insert(r.output_path.clone());
    }
    for r in &report.audio_results {
        written.insert(r.output_path.clone());
    }
    for r in &report.code_results {
        written.insert(r.output_path.clone());
        if let Some(map) = &r.source_map_path {
            written.insert(map.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn detects_default_suffix_outputs() {
        assert!(is_squish_output(&p("/a/dog_squished.png"), "squished"));
        assert!(is_squish_output(&p("/a/dog_squished_2.png"), "squished"));
        assert!(is_squish_output(&p("/a/dog_squished_13.jpg"), "squished"));
        assert!(!is_squish_output(&p("/a/dog.png"), "squished"));
        assert!(!is_squish_output(&p("/a/squished.png"), "squished"));
        assert!(!is_squish_output(&p("/a/dog_squishedish.png"), "squished"));
    }

    #[test]
    fn detects_custom_suffix_outputs() {
        assert!(is_squish_output(&p("/a/dog_tiny.png"), "tiny"));
        assert!(!is_squish_output(&p("/a/dog_squished.png"), "tiny"));
    }

    #[test]
    fn detects_minified_code_outputs() {
        assert!(is_squish_output(&p("/d/app.min.js"), "squished"));
        assert!(is_squish_output(&p("/d/style.min.css"), "squished"));
        assert!(is_squish_output(&p("/d/app.min.js.map"), "squished"));
        assert!(!is_squish_output(&p("/d/app.js"), "squished"));
        assert!(!is_squish_output(&p("/d/min.js"), "squished"));
    }

    #[test]
    fn should_process_skips_written_once() {
        let mut written = HashSet::new();
        let tmp = tempfile::TempDir::new().unwrap();
        let file = tmp.path().join("dog.png");
        std::fs::write(&file, b"x").unwrap();
        let roots = [tmp.path().to_path_buf()];
        let excludes = ExcludeOptions::default();

        written.insert(file.clone());
        // First event after our own write: swallowed, entry consumed.
        assert!(!should_process(
            &file,
            "squished",
            &mut written,
            &roots,
            &excludes
        ));
        // Next event is a genuine edit: processed.
        assert!(should_process(
            &file,
            "squished",
            &mut written,
            &roots,
            &excludes
        ));
    }

    #[test]
    fn should_process_ignores_outputs_and_missing_files() {
        let mut written = HashSet::new();
        let tmp = tempfile::TempDir::new().unwrap();
        let out = tmp.path().join("dog_squished.png");
        std::fs::write(&out, b"x").unwrap();
        let roots = [tmp.path().to_path_buf()];
        let excludes = ExcludeOptions::default();
        assert!(!should_process(
            &out,
            "squished",
            &mut written,
            &roots,
            &excludes
        ));

        // Deleted/missing paths produce events too; never process them.
        let gone = tmp.path().join("gone.png");
        assert!(!should_process(
            &gone,
            "squished",
            &mut written,
            &roots,
            &excludes
        ));
    }

    #[test]
    fn should_process_accepts_regular_new_file() {
        let mut written = HashSet::new();
        let tmp = tempfile::TempDir::new().unwrap();
        let file = tmp.path().join("dog.png");
        std::fs::write(&file, b"x").unwrap();
        let roots = [tmp.path().to_path_buf()];
        let excludes = ExcludeOptions::default();
        assert!(should_process(
            &file,
            "squished",
            &mut written,
            &roots,
            &excludes
        ));
    }
}
