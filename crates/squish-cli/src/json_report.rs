//! `--json` output schema: a single machine-readable document describing a
//! run, printed to stdout in place of the human summary. Kept as a dedicated
//! module (rather than deriving `Serialize` on `RunReport` directly) because
//! the wire format's field names and shape (e.g. `bytes_in`/`bytes_out`
//! instead of `input_bytes`/`output_bytes`) are a public, versioned contract
//! separate from the runner's internal types.
//!
//! Design note: errored files appear only in the top-level `errors` array
//! (`{input, message}`), not as `status: "error"` entries in `files` — the
//! runner's error collection loses per-file kind by the time errors reach
//! `RunReport`, and threading it through wasn't worth the churn for this
//! brief. `files` entries are always `"squished"` or `"skipped"`.

use crate::runner::RunReport;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Serialize)]
pub struct JsonReport {
    pub version: u32,
    pub files: Vec<FileEntry>,
    pub totals: Totals,
    pub errors: Vec<ErrorEntry>,
}

#[derive(Serialize)]
pub struct FileEntry {
    pub input: String,
    pub output: Option<String>,
    pub kind: String,
    pub format: Option<String>,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub saving_pct: f64,
    pub status: String,
}

#[derive(Serialize)]
pub struct Totals {
    pub files: usize,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub saving_pct: f64,
    pub by_kind: BTreeMap<String, KindTotals>,
}

#[derive(Serialize, Default)]
pub struct KindTotals {
    pub files: usize,
    pub bytes_in: u64,
    pub bytes_out: u64,
}

#[derive(Serialize)]
pub struct ErrorEntry {
    pub input: String,
    pub message: String,
}

fn saving_pct(bytes_in: u64, bytes_out: u64) -> f64 {
    if bytes_in == 0 {
        0.0
    } else {
        (1.0 - bytes_out as f64 / bytes_in as f64) * 100.0
    }
}

/// Best-effort file size for a planned (not-yet-touched) file; 0 if it can't
/// be stat'd (e.g. removed between listing and reporting).
fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// Build the report for a completed (non-dry-run) run.
pub fn build(report: &RunReport) -> JsonReport {
    let mut files = Vec::new();
    let mut by_kind: BTreeMap<String, KindTotals> = BTreeMap::new();

    for r in &report.results {
        push_squished(
            &mut files,
            &mut by_kind,
            "image",
            &r.input_path,
            &r.output_path,
            Some(r.format_out.extension()),
            r.input_bytes,
            r.output_bytes,
        );
    }
    for r in &report.video_results {
        push_squished(
            &mut files,
            &mut by_kind,
            "video",
            &r.input_path,
            &r.output_path,
            Some(r.format_out.extension()),
            r.input_bytes,
            r.output_bytes,
        );
    }
    for r in &report.audio_results {
        push_squished(
            &mut files,
            &mut by_kind,
            "audio",
            &r.input_path,
            &r.output_path,
            Some(r.format_out.extension()),
            r.input_bytes,
            r.output_bytes,
        );
    }
    for r in &report.code_results {
        push_squished(
            &mut files,
            &mut by_kind,
            "code",
            &r.input_path,
            &r.output_path,
            Some(r.format.extension()),
            r.input_bytes,
            r.output_bytes,
        );
    }
    for p in &report.skipped_unknown {
        files.push(FileEntry {
            input: p.display().to_string(),
            output: None,
            kind: "unknown".to_string(),
            format: None,
            bytes_in: file_size(p),
            bytes_out: 0,
            saving_pct: 0.0,
            status: "skipped".to_string(),
        });
    }
    for r in &report.already_optimal_images {
        push_already_optimal(
            &mut files,
            "image",
            &r.input_path,
            &r.output_path,
            Some(r.format_out.extension()),
            r.input_bytes,
        );
    }
    for r in &report.already_optimal_video {
        push_already_optimal(
            &mut files,
            "video",
            &r.input_path,
            &r.output_path,
            Some(r.format_out.extension()),
            r.input_bytes,
        );
    }
    for r in &report.already_optimal_audio {
        push_already_optimal(
            &mut files,
            "audio",
            &r.input_path,
            &r.output_path,
            Some(r.format_out.extension()),
            r.input_bytes,
        );
    }
    for r in &report.already_optimal_code {
        push_already_optimal(
            &mut files,
            "code",
            &r.input_path,
            &r.output_path,
            Some(r.format.extension()),
            r.input_bytes,
        );
    }

    let bytes_in = report.input_bytes();
    let bytes_out = report.output_bytes();
    let totals = Totals {
        files: report.total_files(),
        bytes_in,
        bytes_out,
        saving_pct: saving_pct(bytes_in, bytes_out),
        by_kind,
    };

    let errors = report
        .errors
        .iter()
        .map(|(p, msg)| ErrorEntry {
            input: p.display().to_string(),
            message: msg.clone(),
        })
        .collect();

    JsonReport {
        version: SCHEMA_VERSION,
        files,
        totals,
        errors,
    }
}

#[allow(clippy::too_many_arguments)]
fn push_squished(
    files: &mut Vec<FileEntry>,
    by_kind: &mut BTreeMap<String, KindTotals>,
    kind: &str,
    input: &Path,
    output: &Path,
    format: Option<&str>,
    bytes_in: u64,
    bytes_out: u64,
) {
    files.push(FileEntry {
        input: input.display().to_string(),
        output: Some(output.display().to_string()),
        kind: kind.to_string(),
        format: format.map(str::to_string),
        bytes_in,
        bytes_out,
        saving_pct: saving_pct(bytes_in, bytes_out),
        status: "squished".to_string(),
    });
    let kt = by_kind.entry(kind.to_string()).or_default();
    kt.files += 1;
    kt.bytes_in += bytes_in;
    kt.bytes_out += bytes_out;
}

/// A file the never-grow guarantee (Brief 12) discarded the encode for:
/// output was left byte-identical to input, so `bytes_in == bytes_out` and
/// `saving_pct` is 0. Deliberately excluded from `totals` — the totals
/// reflect genuine squishes, matching "Squished N files" in the human
/// summary not counting these either.
fn push_already_optimal(
    files: &mut Vec<FileEntry>,
    kind: &str,
    input: &Path,
    output: &Path,
    format: Option<&str>,
    bytes: u64,
) {
    files.push(FileEntry {
        input: input.display().to_string(),
        output: Some(output.display().to_string()),
        kind: kind.to_string(),
        format: format.map(str::to_string),
        bytes_in: bytes,
        bytes_out: bytes,
        saving_pct: 0.0,
        status: "skipped".to_string(),
    });
}

/// Build the report for a `--dry-run` (nothing was actually written, so
/// every planned file is `"skipped"` with `bytes_out: 0`).
pub fn build_dry_run(
    image_files: &[PathBuf],
    video_files: &[PathBuf],
    audio_files: &[PathBuf],
    code_files: &[PathBuf],
    skipped_unknown: &[PathBuf],
) -> JsonReport {
    let mut files = Vec::new();
    for (kind, paths) in [
        ("image", image_files),
        ("video", video_files),
        ("audio", audio_files),
        ("code", code_files),
        ("unknown", skipped_unknown),
    ] {
        for p in paths {
            files.push(FileEntry {
                input: p.display().to_string(),
                output: None,
                kind: kind.to_string(),
                format: None,
                bytes_in: file_size(p),
                bytes_out: 0,
                saving_pct: 0.0,
                status: "skipped".to_string(),
            });
        }
    }

    JsonReport {
        version: SCHEMA_VERSION,
        files,
        totals: Totals {
            files: 0,
            bytes_in: 0,
            bytes_out: 0,
            saving_pct: 0.0,
            by_kind: BTreeMap::new(),
        },
        errors: Vec::new(),
    }
}

/// Serialize and print the report as the sole line of stdout output.
pub fn print(report: &JsonReport) {
    println!(
        "{}",
        serde_json::to_string(report).expect("JsonReport is always serializable")
    );
}
