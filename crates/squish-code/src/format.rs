use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodeFormat {
    Js,
    Ts,
    Css,
    Html,
    Json,
}

impl CodeFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            CodeFormat::Js => "js",
            CodeFormat::Ts => "ts",
            CodeFormat::Css => "css",
            CodeFormat::Html => "html",
            CodeFormat::Json => "json",
        }
    }

    pub fn parse(s: &str) -> Option<CodeFormat> {
        match s.to_ascii_lowercase().as_str() {
            "js" | "mjs" | "cjs" | "jsx" => Some(CodeFormat::Js),
            "ts" | "mts" | "cts" | "tsx" => Some(CodeFormat::Ts),
            "css" => Some(CodeFormat::Css),
            "html" | "htm" => Some(CodeFormat::Html),
            "json" => Some(CodeFormat::Json),
            _ => None,
        }
    }

    pub fn supports_source_map(&self) -> bool {
        matches!(self, CodeFormat::Js | CodeFormat::Ts | CodeFormat::Css)
    }

    /// Whether the file should be parsed in JSX-aware mode (only relevant for JS/TS).
    pub fn is_jsx_extension(ext: &str) -> bool {
        matches!(ext.to_ascii_lowercase().as_str(), "jsx" | "tsx")
    }
}

pub fn detect_code_format(path: &Path) -> Option<CodeFormat> {
    path.extension()
        .and_then(|e| e.to_str())
        .and_then(CodeFormat::parse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parse_js_aliases() {
        assert_eq!(CodeFormat::parse("js"), Some(CodeFormat::Js));
        assert_eq!(CodeFormat::parse("JS"), Some(CodeFormat::Js));
        assert_eq!(CodeFormat::parse("mjs"), Some(CodeFormat::Js));
        assert_eq!(CodeFormat::parse("cjs"), Some(CodeFormat::Js));
        assert_eq!(CodeFormat::parse("jsx"), Some(CodeFormat::Js));
    }

    #[test]
    fn parse_ts_aliases() {
        assert_eq!(CodeFormat::parse("ts"), Some(CodeFormat::Ts));
        assert_eq!(CodeFormat::parse("mts"), Some(CodeFormat::Ts));
        assert_eq!(CodeFormat::parse("cts"), Some(CodeFormat::Ts));
        assert_eq!(CodeFormat::parse("tsx"), Some(CodeFormat::Ts));
    }

    #[test]
    fn parse_css_html_json() {
        assert_eq!(CodeFormat::parse("css"), Some(CodeFormat::Css));
        assert_eq!(CodeFormat::parse("html"), Some(CodeFormat::Html));
        assert_eq!(CodeFormat::parse("htm"), Some(CodeFormat::Html));
        assert_eq!(CodeFormat::parse("json"), Some(CodeFormat::Json));
    }

    #[test]
    fn parse_rejects_unknown() {
        assert_eq!(CodeFormat::parse("png"), None);
        assert_eq!(CodeFormat::parse(""), None);
        assert_eq!(CodeFormat::parse("yaml"), None);
    }

    #[test]
    fn extension_canonical() {
        assert_eq!(CodeFormat::Js.extension(), "js");
        assert_eq!(CodeFormat::Ts.extension(), "ts");
        assert_eq!(CodeFormat::Css.extension(), "css");
        assert_eq!(CodeFormat::Html.extension(), "html");
        assert_eq!(CodeFormat::Json.extension(), "json");
    }

    #[test]
    fn supports_source_map_truth_table() {
        assert!(CodeFormat::Js.supports_source_map());
        assert!(CodeFormat::Ts.supports_source_map());
        assert!(CodeFormat::Css.supports_source_map());
        assert!(!CodeFormat::Html.supports_source_map());
        assert!(!CodeFormat::Json.supports_source_map());
    }

    #[test]
    fn detect_from_path() {
        assert_eq!(
            detect_code_format(&PathBuf::from("app.js")),
            Some(CodeFormat::Js)
        );
        assert_eq!(
            detect_code_format(&PathBuf::from("Component.tsx")),
            Some(CodeFormat::Ts)
        );
        assert_eq!(detect_code_format(&PathBuf::from("noext")), None);
        assert_eq!(
            detect_code_format(&PathBuf::from("data.JSON")),
            Some(CodeFormat::Json)
        );
    }

    #[test]
    fn is_jsx_extension_only_for_jsx_tsx() {
        assert!(CodeFormat::is_jsx_extension("jsx"));
        assert!(CodeFormat::is_jsx_extension("tsx"));
        assert!(CodeFormat::is_jsx_extension("TSX"));
        assert!(!CodeFormat::is_jsx_extension("js"));
        assert!(!CodeFormat::is_jsx_extension("ts"));
    }
}
