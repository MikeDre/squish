//! Core image compression library for squish.

pub mod error;
pub mod format;
pub mod options;
pub mod result;

pub use error::SquishError;
pub use format::{detect_format, Format};
pub use options::SquishOptions;
pub use result::SquishResult;
