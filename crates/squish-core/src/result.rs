use crate::format::Format;
use std::path::PathBuf;
use std::time::Duration;

/// Outcome of compressing a single file.
#[derive(Debug, Clone)]
pub struct SquishResult {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub format_in: Format,
    pub format_out: Format,
    pub duration: Duration,
}

impl SquishResult {
    /// Size reduction percentage, signed (negative = output grew).
    /// Returns 0.0 if input was empty.
    pub fn reduction_percent(&self) -> f64 {
        if self.input_bytes == 0 {
            return 0.0;
        }
        let delta = self.input_bytes as f64 - self.output_bytes as f64;
        (delta / self.input_bytes as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(input: u64, output: u64) -> SquishResult {
        SquishResult {
            input_path: PathBuf::from("a"),
            output_path: PathBuf::from("b"),
            input_bytes: input,
            output_bytes: output,
            format_in: Format::Png,
            format_out: Format::Png,
            duration: Duration::from_millis(10),
        }
    }

    #[test]
    fn reduction_positive_when_smaller() {
        let r = sample(1000, 400);
        assert!((r.reduction_percent() - 60.0).abs() < 0.001);
    }

    #[test]
    fn reduction_negative_when_grew() {
        let r = sample(1000, 1200);
        assert!(r.reduction_percent() < 0.0);
    }

    #[test]
    fn reduction_zero_on_empty_input() {
        let r = sample(0, 0);
        assert_eq!(r.reduction_percent(), 0.0);
    }
}
