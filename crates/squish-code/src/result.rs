use crate::format::CodeFormat;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct CodeResult {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub source_map_path: Option<PathBuf>,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub format: CodeFormat,
    pub duration: Duration,
}

impl CodeResult {
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

    fn sample(input: u64, output: u64) -> CodeResult {
        CodeResult {
            input_path: PathBuf::from("a.js"),
            output_path: PathBuf::from("a.min.js"),
            source_map_path: None,
            input_bytes: input,
            output_bytes: output,
            format: CodeFormat::Js,
            duration: Duration::from_millis(5),
        }
    }

    #[test]
    fn reduction_positive() {
        let r = sample(1000, 400);
        assert!((r.reduction_percent() - 60.0).abs() < 0.001);
    }

    #[test]
    fn reduction_negative() {
        let r = sample(100, 120);
        assert!(r.reduction_percent() < 0.0);
    }

    #[test]
    fn reduction_zero_on_empty_input() {
        assert_eq!(sample(0, 0).reduction_percent(), 0.0);
    }
}
