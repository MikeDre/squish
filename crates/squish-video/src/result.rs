use crate::format::VideoFormat;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct VideoResult {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub format_in: VideoFormat,
    pub format_out: VideoFormat,
    pub duration: Duration,
}

impl VideoResult {
    pub fn reduction_percent(&self) -> f64 {
        if self.input_bytes == 0 { return 0.0; }
        let delta = self.input_bytes as f64 - self.output_bytes as f64;
        (delta / self.input_bytes as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(input: u64, output: u64) -> VideoResult {
        VideoResult {
            input_path: PathBuf::from("a.mp4"),
            output_path: PathBuf::from("b.mp4"),
            input_bytes: input,
            output_bytes: output,
            format_in: VideoFormat::Mp4,
            format_out: VideoFormat::Mp4,
            duration: Duration::from_millis(100),
        }
    }

    #[test]
    fn reduction_positive() {
        let r = sample(10_000_000, 4_000_000);
        assert!((r.reduction_percent() - 60.0).abs() < 0.001);
    }

    #[test]
    fn reduction_negative() {
        let r = sample(1000, 1200);
        assert!(r.reduction_percent() < 0.0);
    }

    #[test]
    fn reduction_zero_on_empty() {
        assert_eq!(sample(0, 0).reduction_percent(), 0.0);
    }
}
