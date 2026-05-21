//! Shared ffmpeg plumbing for squish-video and squish-audio.

pub mod error;
pub mod ffmpeg;

pub use error::MediaError;
pub use ffmpeg::check_ffmpeg;
