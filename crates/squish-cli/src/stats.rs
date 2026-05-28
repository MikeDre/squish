//! Local usage ledger: records bytes saved & files squished per squish batch,
//! and renders a small report on demand. No data ever leaves the machine.

use chrono::{DateTime, Utc};
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
    pub fn saved_bytes(&self) -> i64 {
        self.by_kind
            .values()
            .map(|k| k.bytes_in as i64 - k.bytes_out as i64)
            .sum()
    }

    /// Total files across all kinds.
    pub fn file_count(&self) -> u64 {
        self.by_kind.values().map(|k| k.n).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_record() -> Record {
        let mut by_kind = BTreeMap::new();
        by_kind.insert(
            "image".to_string(),
            KindStats { n: 4, bytes_in: 12345, bytes_out: 3456 },
        );
        by_kind.insert(
            "video".to_string(),
            KindStats { n: 1, bytes_in: 1_000_000, bytes_out: 400_000 },
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
            KindStats { n: 1, bytes_in: 100, bytes_out: 500 },
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
}
