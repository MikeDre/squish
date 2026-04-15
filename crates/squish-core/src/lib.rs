//! Core image compression library for squish.
//!
//! Public surface:
//! - [`squish_file`] — compress a single file
//! - [`detect_format`] — identify format from extension + magic bytes
//! - Types: [`SquishOptions`], [`SquishResult`], [`SquishError`], [`Format`]

pub fn placeholder() -> &'static str {
    "squish-core"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_returns_name() {
        assert_eq!(placeholder(), "squish-core");
    }
}
