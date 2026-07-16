//! Perceptual quality measurement for video `--quality auto`, using
//! segment-sampled VMAF scoring via ffmpeg's libvmaf filter. See
//! docs/superpowers/specs/2026-07-16-quality-auto-video-design.md.

use crate::options::{VideoCodec, VideoOptions};
use crate::VideoError;
use std::path::Path;

/// VMAF score at/above which video output is considered visually lossless.
pub(crate) const VISUALLY_LOSSLESS_THRESHOLD: f64 = 95.0;

/// One sampled segment of the source video: start offset and length, both in
/// seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Segment {
    pub start_secs: f64,
    pub len_secs: f64,
}

/// Duration at/under which the whole clip is scored as a single segment
/// instead of sampling three.
const SINGLE_SEGMENT_THRESHOLD_SECS: f64 = 8.0;
/// Length of each sampled segment, in seconds, for clips longer than the
/// single-segment threshold.
const SEGMENT_LEN_SECS: f64 = 2.0;
/// Relative positions (fraction of duration) sampled for longer clips —
/// avoids exact start/end, which can have black frames or atypical pacing.
const SEGMENT_POSITIONS: [f64; 3] = [0.10, 0.50, 0.85];

/// Compute the segments to sample for VMAF scoring. Clips at or under 8s
/// are scored whole (avoids seek-boundary edge cases on inputs shorter than
/// the sampling window would need anyway); longer clips get three 2s
/// segments at 10%/50%/85% of duration, each clamped so it never runs past
/// `duration_secs - SEGMENT_LEN_SECS`.
pub(crate) fn sample_segments(duration_secs: f64) -> Vec<Segment> {
    if duration_secs <= SINGLE_SEGMENT_THRESHOLD_SECS {
        return vec![Segment {
            start_secs: 0.0,
            len_secs: duration_secs.max(0.0),
        }];
    }

    let max_start = (duration_secs - SEGMENT_LEN_SECS).max(0.0);
    SEGMENT_POSITIONS
        .iter()
        .map(|&pos| Segment {
            start_secs: (duration_secs * pos).min(max_start),
            len_secs: SEGMENT_LEN_SECS,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_clip_yields_one_whole_segment() {
        let segs = sample_segments(2.0);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0], Segment { start_secs: 0.0, len_secs: 2.0 });
    }

    #[test]
    fn exactly_at_threshold_yields_one_whole_segment() {
        let segs = sample_segments(8.0);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].len_secs, 8.0);
    }

    #[test]
    fn long_clip_yields_three_positioned_segments() {
        let segs = sample_segments(60.0);
        assert_eq!(segs.len(), 3);
        assert!((segs[0].start_secs - 6.0).abs() < 0.001);
        assert!((segs[1].start_secs - 30.0).abs() < 0.001);
        assert!((segs[2].start_secs - 51.0).abs() < 0.001);
        for s in &segs {
            assert_eq!(s.len_secs, 2.0);
            assert!(s.start_secs + s.len_secs <= 60.0);
        }
    }

    #[test]
    fn last_segment_clamps_when_near_the_end() {
        // duration=8.01: the 85% position (6.8085s) would run past
        // duration - 2.0 = 6.01s, so it must clamp to 6.01.
        let segs = sample_segments(8.01);
        assert_eq!(segs.len(), 3);
        assert!((segs[2].start_secs - 6.01).abs() < 0.001);
        assert!(segs[2].start_secs + segs[2].len_secs <= 8.01 + 0.001);
    }
}
