//! Local usage ledger: records bytes saved & files squished per squish batch,
//! and renders a small report on demand. No data ever leaves the machine.

use chrono::{DateTime, Datelike, Local, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One `squish` batch's contribution to the ledger.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// Schema version. Bump when the on-disk shape changes.
    pub v: u32,
    /// Batch end timestamp, UTC.
    pub ts: DateTime<Utc>,
    /// Per-kind totals; absent kinds = 0 files. Kinds: "image", "video",
    /// "audio", "code".
    pub by_kind: BTreeMap<String, KindStats>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct KindStats {
    /// Files in this batch for this kind.
    pub n: u64,
    /// Total input bytes for this kind.
    #[serde(rename = "in")]
    pub bytes_in: u64,
    /// Total output bytes for this kind.
    #[serde(rename = "out")]
    pub bytes_out: u64,
}

impl Record {
    /// Did this batch record any successful squishes?
    pub fn is_empty(&self) -> bool {
        self.by_kind.values().all(|k| k.n == 0)
    }

    /// Sum of (bytes_in - bytes_out) across all kinds. Signed: negative when
    /// the output grew (rare, honest).
    #[allow(dead_code)]
    pub fn saved_bytes(&self) -> i64 {
        self.by_kind
            .values()
            .map(|k| k.bytes_in as i64 - k.bytes_out as i64)
            .sum()
    }

    /// Total files across all kinds.
    #[allow(dead_code)]
    pub fn file_count(&self) -> u64 {
        self.by_kind.values().map(|k| k.n).sum()
    }
}

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Append one record to `path` as a single JSON Lines entry. Creates the
/// parent directory if needed. Opens the file with O_APPEND so concurrent
/// writers can safely interleave (each record is a single short line).
pub fn append_record(path: &Path, record: &Record) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut line = serde_json::to_string(record)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    line.push('\n');
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    f.write_all(line.as_bytes())
}

/// Load all records from a JSONL file. Missing file → empty Vec.
/// Malformed lines are silently skipped (so future-version records we can't
/// parse don't break the report).
pub fn load_records(path: &Path) -> std::io::Result<Vec<Record>> {
    let f = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let reader = BufReader::new(f);
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(rec) = serde_json::from_str::<Record>(trimmed) {
            out.push(rec);
        }
    }
    Ok(out)
}

/// Default data-file path. `None` if the platform data dir cannot be resolved
/// (broken HOME / unsupported platform). Production callers check for None
/// and silently skip recording in that case.
pub fn default_data_file() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("squish").join("usage.jsonl"))
}

const KINDS: &[&str] = &["image", "video", "audio", "code"];

/// A per-window aggregate, ready to render.
#[derive(Debug, Clone, Default)]
struct WindowTotals {
    files: u64,
    bytes_in: u64,
    bytes_out: u64,
    by_kind: BTreeMap<String, KindStats>,
}

impl WindowTotals {
    fn accumulate(&mut self, rec: &Record) {
        for (kind, k) in &rec.by_kind {
            if k.n == 0 {
                continue;
            }
            self.files += k.n;
            self.bytes_in += k.bytes_in;
            self.bytes_out += k.bytes_out;
            let entry = self.by_kind.entry(kind.clone()).or_default();
            entry.n += k.n;
            entry.bytes_in += k.bytes_in;
            entry.bytes_out += k.bytes_out;
        }
    }

    fn saved_bytes(&self) -> i64 {
        self.bytes_in as i64 - self.bytes_out as i64
    }

    fn saved_percent(&self) -> f64 {
        if self.bytes_in == 0 {
            0.0
        } else {
            (self.bytes_in as f64 - self.bytes_out as f64) / self.bytes_in as f64 * 100.0
        }
    }
}

/// First moment of the calendar month of `now`, in local time, as a UTC
/// instant suitable for comparing with record `ts` fields.
fn month_start_utc(now: DateTime<Local>) -> DateTime<Utc> {
    let local = Local
        .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .single()
        .expect("first-of-month is unambiguous local time");
    local.with_timezone(&Utc)
}

/// Render the usage report. `now` is injected for testability (production
/// callers pass `Local::now()`).
pub fn render_report(records: &[Record], now: DateTime<Local>) -> String {
    if records.is_empty() {
        return "no usage recorded yet — squish some files and try again.\n".to_string();
    }

    let month_pivot = month_start_utc(now);
    let mut this_month = WindowTotals::default();
    let mut all_time = WindowTotals::default();
    for r in records {
        all_time.accumulate(r);
        if r.ts >= month_pivot {
            this_month.accumulate(r);
        }
    }

    let earliest = records.iter().map(|r| r.ts).min().expect("non-empty");
    let earliest_local = earliest.with_timezone(&Local).format("%Y-%m-%d");
    let month_label = now.format("%B %Y");

    let mut out = String::new();
    out.push_str(&format!("This month ({month_label})\n"));
    write_window(&mut out, &this_month);
    out.push('\n');
    out.push_str(&format!("All time (since {earliest_local})\n"));
    write_window(&mut out, &all_time);
    out
}

fn write_window(out: &mut String, w: &WindowTotals) {
    out.push_str(&format!("  Files squished:    {}\n", w.files));
    out.push_str(&format!(
        "  Total saved:       {} ({:.1}%)\n",
        format_bytes(w.saved_bytes()),
        w.saved_percent()
    ));
    for kind in KINDS {
        if let Some(ks) = w.by_kind.get(*kind) {
            if ks.n == 0 {
                continue;
            }
            let saved = ks.bytes_in as i64 - ks.bytes_out as i64;
            let pct = if ks.bytes_in == 0 {
                0.0
            } else {
                (ks.bytes_in as f64 - ks.bytes_out as f64) / ks.bytes_in as f64 * 100.0
            };
            out.push_str(&format!(
                "    {:<5}  {:>6} files   {} saved ({:.1}%)\n",
                kind,
                ks.n,
                format_bytes(saved),
                pct
            ));
        }
    }
}

/// Human-readable signed byte count: "1.2 GB", "-340 KB", "12 B".
fn format_bytes(b: i64) -> String {
    let sign = if b < 0 { "-" } else { "" };
    let mut v = b.unsigned_abs() as f64;
    for unit in ["B", "KB", "MB", "GB", "TB"] {
        if v < 1024.0 || unit == "TB" {
            return if unit == "B" {
                format!("{sign}{} {unit}", v as u64)
            } else {
                format!("{sign}{:.1} {unit}", v)
            };
        }
        v /= 1024.0;
    }
    unreachable!()
}

use crate::runner::RunReport;

/// Pure gate predicate. All callers pass the three booleans explicitly; the
/// env-var read is done once in `append_batch`.
pub fn should_record(dry_run: bool, no_stats_flag: bool, env_no_stats: bool) -> bool {
    !dry_run && !no_stats_flag && !env_no_stats
}

/// Build a `Record` from a finished `RunReport`. Sums per-kind input/output
/// bytes and file counts. `is_empty()` will be true if no successes occurred.
pub fn build_record(report: &RunReport) -> Record {
    let mut by_kind: BTreeMap<String, KindStats> = BTreeMap::new();

    let mut accumulate = |kind: &str, n: u64, in_: u64, out_: u64| {
        if n == 0 {
            return;
        }
        let e = by_kind.entry(kind.to_string()).or_default();
        e.n += n;
        e.bytes_in += in_;
        e.bytes_out += out_;
    };

    let img_n = report.results.len() as u64;
    let img_in: u64 = report.results.iter().map(|r| r.input_bytes).sum();
    let img_out: u64 = report.results.iter().map(|r| r.output_bytes).sum();
    accumulate("image", img_n, img_in, img_out);

    let vid_n = report.video_results.len() as u64;
    let vid_in: u64 = report.video_results.iter().map(|r| r.input_bytes).sum();
    let vid_out: u64 = report.video_results.iter().map(|r| r.output_bytes).sum();
    accumulate("video", vid_n, vid_in, vid_out);

    let aud_n = report.audio_results.len() as u64;
    let aud_in: u64 = report.audio_results.iter().map(|r| r.input_bytes).sum();
    let aud_out: u64 = report.audio_results.iter().map(|r| r.output_bytes).sum();
    accumulate("audio", aud_n, aud_in, aud_out);

    let cod_n = report.code_results.len() as u64;
    let cod_in: u64 = report.code_results.iter().map(|r| r.input_bytes).sum();
    let cod_out: u64 = report.code_results.iter().map(|r| r.output_bytes).sum();
    accumulate("code", cod_n, cod_in, cod_out);

    Record {
        v: 1,
        ts: Utc::now(),
        by_kind,
    }
}

/// Read the `SQUISH_NO_STATS` env var. Non-empty value = opt out.
pub fn env_no_stats() -> bool {
    std::env::var_os("SQUISH_NO_STATS")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

/// Production wrapper: append one batch's record to the default data file
/// if all gates are open and the batch had at least one successful squish.
/// Silently no-ops on any io/path failure — stats must never fail squish.
pub fn append_batch(report: &RunReport, dry_run: bool, no_stats_flag: bool) {
    if !should_record(dry_run, no_stats_flag, env_no_stats()) {
        return;
    }
    let record = build_record(report);
    if record.is_empty() {
        return;
    }
    let Some(path) = default_data_file() else {
        return;
    };
    let _ = append_record(&path, &record);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_record() -> Record {
        let mut by_kind = BTreeMap::new();
        by_kind.insert(
            "image".to_string(),
            KindStats {
                n: 4,
                bytes_in: 12345,
                bytes_out: 3456,
            },
        );
        by_kind.insert(
            "video".to_string(),
            KindStats {
                n: 1,
                bytes_in: 1_000_000,
                bytes_out: 400_000,
            },
        );
        Record {
            v: 1,
            ts: Utc.with_ymd_and_hms(2026, 5, 28, 12, 34, 56).unwrap(),
            by_kind,
        }
    }

    #[test]
    fn record_json_roundtrip() {
        let r = sample_record();
        let s = serde_json::to_string(&r).expect("serialize");
        let back: Record = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(r, back);
    }

    #[test]
    fn record_uses_in_out_keys_in_json() {
        // Locks the on-disk shape: KindStats fields must serialize as "in"/"out"
        // (not "bytes_in"/"bytes_out"), per spec.
        let r = sample_record();
        let s = serde_json::to_string(&r).expect("serialize");
        assert!(s.contains("\"in\":12345"), "expected \"in\":12345 in {s}");
        assert!(s.contains("\"out\":3456"), "expected \"out\":3456 in {s}");
        assert!(!s.contains("bytes_in"));
        assert!(!s.contains("bytes_out"));
    }

    #[test]
    fn saved_bytes_signed_honest() {
        let mut r = sample_record();
        // Force "code" output > input → record.saved_bytes() must subtract.
        r.by_kind.insert(
            "code".to_string(),
            KindStats {
                n: 1,
                bytes_in: 100,
                bytes_out: 500,
            },
        );
        // image: 12345-3456 = 8889, video: 1_000_000-400_000 = 600_000, code: -400
        assert_eq!(r.saved_bytes(), 8889 + 600_000 - 400);
    }

    #[test]
    fn is_empty_when_all_kinds_zero() {
        let mut by_kind = BTreeMap::new();
        by_kind.insert("image".to_string(), KindStats::default());
        let r = Record {
            v: 1,
            ts: Utc::now(),
            by_kind,
        };
        assert!(r.is_empty());
        assert_eq!(r.file_count(), 0);
    }

    #[test]
    fn append_then_load_one_record() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("usage.jsonl");
        let r = sample_record();
        append_record(&path, &r).expect("append");
        let loaded = load_records(&path).expect("load");
        assert_eq!(loaded, vec![r]);
    }

    #[test]
    fn append_creates_parent_dir_lazily() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("nested/deeper/usage.jsonl");
        assert!(!path.parent().unwrap().exists());
        append_record(&path, &sample_record()).expect("append should mkdir -p");
        assert!(path.exists());
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("does-not-exist.jsonl");
        let loaded = load_records(&path).expect("missing file → Ok(empty)");
        assert!(loaded.is_empty());
    }

    #[test]
    fn load_skips_malformed_lines() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("usage.jsonl");
        std::fs::write(
            &path,
            b"{this is not json}\n\
              {\"v\":1,\"ts\":\"2026-05-28T12:34:56Z\",\"by_kind\":{}}\n\
              \n\
              not-json-either\n",
        )
        .unwrap();
        let loaded = load_records(&path).expect("load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].v, 1);
    }

    #[test]
    fn append_uses_o_append_so_lines_concat() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("usage.jsonl");
        let mut r1 = sample_record();
        let mut r2 = sample_record();
        r2.ts = Utc.with_ymd_and_hms(2026, 5, 29, 0, 0, 0).unwrap();
        r1.by_kind.get_mut("image").unwrap().n = 1;
        r2.by_kind.get_mut("image").unwrap().n = 2;
        append_record(&path, &r1).unwrap();
        append_record(&path, &r2).unwrap();
        let loaded = load_records(&path).expect("load");
        assert_eq!(loaded, vec![r1, r2]);
    }

    use chrono::Local;

    fn dt(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap()
    }

    fn rec_with(ts: DateTime<Utc>, kind: &str, n: u64, in_: u64, out_: u64) -> Record {
        let mut by_kind = BTreeMap::new();
        by_kind.insert(
            kind.to_string(),
            KindStats {
                n,
                bytes_in: in_,
                bytes_out: out_,
            },
        );
        Record { v: 1, ts, by_kind }
    }

    #[test]
    fn report_zero_state_when_no_records() {
        let now = Local.with_ymd_and_hms(2026, 5, 28, 12, 0, 0).unwrap();
        let s = render_report(&[], now);
        assert!(
            s.contains("no usage recorded yet"),
            "zero-state line missing: {s}"
        );
    }

    #[test]
    fn report_mentions_both_windows_and_kinds() {
        let last_month = dt(2026, 4, 15);
        let this_month = dt(2026, 5, 27);
        let recs = vec![
            rec_with(last_month, "image", 10, 10_000_000, 5_000_000),
            rec_with(this_month, "video", 2, 800_000_000, 400_000_000),
        ];
        let now = Local.with_ymd_and_hms(2026, 5, 28, 12, 0, 0).unwrap();
        let s = render_report(&recs, now);
        assert!(
            s.contains("This month"),
            "missing 'This month' header in {s}"
        );
        assert!(s.contains("All time"), "missing 'All time' header in {s}");
        let this_month_section = s.split("All time").next().unwrap();
        assert!(this_month_section.contains("video"));
        assert!(
            !this_month_section.contains("image"),
            "this-month must not include the April record"
        );
        let all_time_section = s.split("All time").nth(1).unwrap();
        assert!(all_time_section.contains("image"));
        assert!(all_time_section.contains("video"));
    }

    #[test]
    fn report_includes_file_counts_and_saved_bytes() {
        let recs = vec![rec_with(dt(2026, 5, 28), "image", 3, 1_048_576, 524_288)];
        let now = Local.with_ymd_and_hms(2026, 5, 28, 12, 0, 0).unwrap();
        let s = render_report(&recs, now);
        assert!(
            s.contains("Files squished:"),
            "missing 'Files squished:' label in {s}"
        );
        assert!(s.contains("image"));
    }

    #[test]
    fn report_omits_kinds_with_no_files_in_window() {
        let recs = vec![rec_with(dt(2026, 5, 28), "audio", 1, 100, 50)];
        let now = Local.with_ymd_and_hms(2026, 5, 28, 12, 0, 0).unwrap();
        let s = render_report(&recs, now);
        let lines: Vec<&str> = s.lines().collect();
        let kind_rows: Vec<&&str> = lines
            .iter()
            .filter(|l| l.starts_with("    "))
            .filter(|l| {
                let t = l.trim_start();
                t.starts_with("image ")
                    || t.starts_with("video ")
                    || t.starts_with("audio ")
                    || t.starts_with("code ")
            })
            .collect();
        assert!(
            kind_rows.iter().all(|l| l.contains("audio")),
            "only audio rows expected, got: {kind_rows:?}"
        );
    }

    use std::time::Duration;

    #[test]
    fn should_record_only_when_all_gates_open() {
        assert!(should_record(false, false, false));
        assert!(!should_record(true, false, false), "dry-run blocks");
        assert!(!should_record(false, true, false), "--no-stats blocks");
        assert!(!should_record(false, false, true), "env opt-out blocks");
    }

    #[test]
    fn build_record_from_empty_report_is_empty() {
        let report = RunReport {
            results: Vec::new(),
            video_results: Vec::new(),
            audio_results: Vec::new(),
            code_results: Vec::new(),
            errors: Vec::new(),
            skipped_unknown: Vec::new(),
            total_wall: Duration::from_millis(0),
        };
        let rec = build_record(&report);
        assert!(rec.is_empty(), "no successes → record must be empty");
    }
}
