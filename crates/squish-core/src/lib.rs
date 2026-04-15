//! Core image compression library for squish.

pub mod format;
pub mod options;

pub use format::{detect_format, Format};
pub use options::SquishOptions;
