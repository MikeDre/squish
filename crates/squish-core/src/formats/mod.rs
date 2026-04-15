//! Per-format compression implementations.
//!
//! Each format module exposes `pub fn compress(input: &[u8], opts: &SquishOptions)
//! -> Result<Vec<u8>, SquishError>`. No file I/O inside — callers handle that.

// Modules added task-by-task:
// pub mod png;
// pub mod jpeg;
// pub mod webp;
// pub mod avif;
// pub mod svg;
// pub mod gif;
// pub mod heic;
// pub mod tiff;
