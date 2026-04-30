use crate::format::AudioFormat;
use crate::options::AudioCodec;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct AudioResult {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub format_in: AudioFormat,
    pub format_out: AudioFormat,
    pub codec_used: AudioCodec,
    pub duration: Duration,
}

impl AudioResult {
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

    fn sample(input: u64, output: u64) -> AudioResult {
        AudioResult {
            input_path: PathBuf::from("a.mp3"),
            output_path: PathBuf::from("b.mp3"),
            input_bytes: input,
            output_bytes: output,
            format_in: AudioFormat::Mp3,
            format_out: AudioFormat::Mp3,
            codec_used: AudioCodec::Mp3,
            duration: Duration::from_millis(100),
        }
    }

    #[test]
    fn reduction_positive() {
        let r = sample(1_000_000, 500_000);
        assert!((r.reduction_percent() - 50.0).abs() < 0.001);
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
