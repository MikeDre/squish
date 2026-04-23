//! Video compression library for squish (ffmpeg-backed).

pub mod error;
pub mod ffmpeg;
pub mod format;
pub mod options;
pub mod result;

pub use error::VideoError;
pub use format::{detect_video_format, detect_video_from_bytes, VideoFormat};
pub use options::{VideoCodec, VideoOptions};
pub use result::VideoResult;

use std::path::Path;

/// Compress a single video file. Shells out to system ffmpeg.
pub fn squish_video(
    _input: &Path,
    _opts: &VideoOptions,
) -> Result<VideoResult, VideoError> {
    todo!("Implemented in Task 2")
}
