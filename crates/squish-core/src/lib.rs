//! Core image compression library for squish.

pub mod error;
pub mod format;
pub mod naming;
pub mod options;
pub mod result;

pub use error::SquishError;
pub use format::{detect_format, Format};
pub use naming::derive_output_path;
pub use options::SquishOptions;
pub use result::SquishResult;
