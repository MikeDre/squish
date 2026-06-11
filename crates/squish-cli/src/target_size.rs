/// Parse a human-readable size budget like "500k", "1.5M", "2g", "800000".
///
/// Units are decimal (k = 1000, M = 1 000 000, G = 1 000 000 000) so a budget
/// like "8M" always fits under a service's "8 MB" limit regardless of whether
/// that limit is decimal or binary. Suffixes are case-insensitive and accept
/// an optional trailing "b" ("500kb" == "500k"). No suffix means bytes.
///
/// Returns `None` for anything that isn't a positive size.
pub fn parse_target_size(s: &str) -> Option<u64> {
    let s = s.trim().to_ascii_lowercase();

    let (number_part, multiplier) =
        if let Some(prefix) = s.strip_suffix("kb").or_else(|| s.strip_suffix('k')) {
            (prefix, 1_000.0)
        } else if let Some(prefix) = s.strip_suffix("mb").or_else(|| s.strip_suffix('m')) {
            (prefix, 1_000_000.0)
        } else if let Some(prefix) = s.strip_suffix("gb").or_else(|| s.strip_suffix('g')) {
            (prefix, 1_000_000_000.0)
        } else if let Some(prefix) = s.strip_suffix('b') {
            (prefix, 1.0)
        } else {
            (s.as_str(), 1.0)
        };

    if number_part.is_empty() || !number_part.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let value: f64 = number_part.parse().ok()?;
    if !value.is_finite() || value <= 0.0 {
        return None;
    }

    let bytes = (value * multiplier).floor();
    if bytes < 1.0 || bytes > u64::MAX as f64 {
        return None;
    }
    Some(bytes as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_bytes() {
        assert_eq!(parse_target_size("800000"), Some(800_000));
    }

    #[test]
    fn kilo_suffix_variants() {
        assert_eq!(parse_target_size("500k"), Some(500_000));
        assert_eq!(parse_target_size("500K"), Some(500_000));
        assert_eq!(parse_target_size("500kb"), Some(500_000));
        assert_eq!(parse_target_size("500KB"), Some(500_000));
    }

    #[test]
    fn mega_suffix() {
        assert_eq!(parse_target_size("2M"), Some(2_000_000));
        assert_eq!(parse_target_size("2mb"), Some(2_000_000));
    }

    #[test]
    fn giga_suffix() {
        assert_eq!(parse_target_size("1g"), Some(1_000_000_000));
    }

    #[test]
    fn fractional_with_suffix() {
        assert_eq!(parse_target_size("1.5M"), Some(1_500_000));
        assert_eq!(parse_target_size("0.5k"), Some(500));
    }

    #[test]
    fn surrounding_whitespace_ok() {
        assert_eq!(parse_target_size("  500k "), Some(500_000));
    }

    #[test]
    fn rejects_zero_empty_and_garbage() {
        assert_eq!(parse_target_size("0"), None);
        assert_eq!(parse_target_size("0k"), None);
        assert_eq!(parse_target_size(""), None);
        assert_eq!(parse_target_size("abc"), None);
        assert_eq!(parse_target_size("k"), None);
        assert_eq!(parse_target_size("12x"), None);
    }

    #[test]
    fn rejects_negative_and_non_finite() {
        assert_eq!(parse_target_size("-5k"), None);
        assert_eq!(parse_target_size("-1"), None);
        assert_eq!(parse_target_size("nan"), None);
        assert_eq!(parse_target_size("inf"), None);
    }

    #[test]
    fn sub_byte_results_reject() {
        assert_eq!(parse_target_size("0.4"), None);
    }
}
