#[derive(Debug, Clone, Default)]
pub struct CodeOptions {
    pub safe: bool,
    pub source_map: bool,
    pub force_overwrite: bool,
    pub suffix: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options() {
        let o = CodeOptions::default();
        assert!(!o.safe);
        assert!(!o.source_map);
        assert!(!o.force_overwrite);
        assert!(o.suffix.is_none());
    }
}
