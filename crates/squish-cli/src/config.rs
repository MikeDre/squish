//! `squish.toml` configuration files.
//!
//! Defaults are loaded from two places and merged, lowest precedence first:
//! 1. the global config at `<platform config dir>/squish/config.toml`
//! 2. the nearest `squish.toml` found walking up from the current directory
//!
//! CLI flags always win over config values. Keys mirror the CLI flag names
//! (kebab-case); kind-specific options live in `[video]`, `[audio]`, and
//! `[code]` tables.

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct FileConfig {
    pub quality: Option<u8>,
    pub lossless: Option<bool>,
    pub format: Option<String>,
    pub recursive: Option<bool>,
    pub suffix: Option<String>,
    pub overwrite: Option<bool>,
    pub jobs: Option<usize>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub target_size: Option<String>,
    #[serde(default)]
    pub video: VideoConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub code: CodeConfig,
}

#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct VideoConfig {
    pub codec: Option<String>,
    pub fast: Option<bool>,
}

#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct AudioConfig {
    pub codec: Option<String>,
    pub bitrate: Option<String>,
    pub strip_tags: Option<bool>,
}

#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct CodeConfig {
    pub safe: Option<bool>,
    pub source_map: Option<bool>,
}

/// Parse a config file's contents. Unknown keys are an error so typos
/// (`qualty = 80`) fail loudly instead of being silently ignored.
pub fn parse_config(toml_text: &str) -> Result<FileConfig, String> {
    let config: FileConfig = toml::from_str(toml_text).map_err(|e| e.to_string())?;
    if let Some(q) = config.quality {
        if q > 100 {
            return Err(format!("quality must be 0-100, got {q}"));
        }
    }
    Ok(config)
}

/// Walk up from `start` looking for a `squish.toml`. Returns the first hit.
pub fn find_project_config(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let candidate = d.join("squish.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = d.parent();
    }
    None
}

/// Resolve the global config path: the `SQUISH_GLOBAL_CONFIG` env override if
/// set (used by tests), else `<platform config dir>/squish/config.toml`.
pub fn global_config_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("SQUISH_GLOBAL_CONFIG") {
        return Some(PathBuf::from(p));
    }
    dirs::config_dir().map(|d| d.join("squish/config.toml"))
}

/// Merge two configs; fields set in `over` win over `base`.
pub fn merge(base: FileConfig, over: FileConfig) -> FileConfig {
    FileConfig {
        quality: over.quality.or(base.quality),
        lossless: over.lossless.or(base.lossless),
        format: over.format.or(base.format),
        recursive: over.recursive.or(base.recursive),
        suffix: over.suffix.or(base.suffix),
        overwrite: over.overwrite.or(base.overwrite),
        jobs: over.jobs.or(base.jobs),
        max_width: over.max_width.or(base.max_width),
        max_height: over.max_height.or(base.max_height),
        target_size: over.target_size.or(base.target_size),
        video: VideoConfig {
            codec: over.video.codec.or(base.video.codec),
            fast: over.video.fast.or(base.video.fast),
        },
        audio: AudioConfig {
            codec: over.audio.codec.or(base.audio.codec),
            bitrate: over.audio.bitrate.or(base.audio.bitrate),
            strip_tags: over.audio.strip_tags.or(base.audio.strip_tags),
        },
        code: CodeConfig {
            safe: over.code.safe.or(base.code.safe),
            source_map: over.code.source_map.or(base.code.source_map),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const FULL: &str = r#"
quality = 75
lossless = false
format = "webp"
recursive = true
overwrite = true
suffix = "tiny"
jobs = 4
max-width = 2000
max-height = 1500
target-size = "500k"

[video]
codec = "h264"
fast = false

[audio]
codec = "opus"
bitrate = "128k"
strip-tags = true

[code]
safe = true
source-map = false
"#;

    #[test]
    fn parses_full_config() {
        let c = parse_config(FULL).unwrap();
        assert_eq!(c.quality, Some(75));
        assert_eq!(c.format.as_deref(), Some("webp"));
        assert_eq!(c.recursive, Some(true));
        assert_eq!(c.suffix.as_deref(), Some("tiny"));
        assert_eq!(c.jobs, Some(4));
        assert_eq!(c.max_width, Some(2000));
        assert_eq!(c.target_size.as_deref(), Some("500k"));
        assert_eq!(c.video.codec.as_deref(), Some("h264"));
        assert_eq!(c.audio.bitrate.as_deref(), Some("128k"));
        assert_eq!(c.audio.strip_tags, Some(true));
        assert_eq!(c.code.safe, Some(true));
        assert_eq!(c.overwrite, Some(true));
    }

    #[test]
    fn empty_config_is_all_none() {
        let c = parse_config("").unwrap();
        assert_eq!(c, FileConfig::default());
    }

    #[test]
    fn unknown_key_is_an_error() {
        let err = parse_config("qualty = 80\n").unwrap_err();
        assert!(
            err.contains("qualty"),
            "error should name the bad key: {err}"
        );
    }

    #[test]
    fn unknown_table_key_is_an_error() {
        let err = parse_config("[video]\nbitrate = \"1M\"\n").unwrap_err();
        assert!(
            err.contains("bitrate"),
            "error should name the bad key: {err}"
        );
    }

    #[test]
    fn quality_out_of_range_is_an_error() {
        assert!(parse_config("quality = 150\n").is_err());
    }

    #[test]
    fn merge_overlay_wins() {
        let base = parse_config("quality = 50\nsuffix = \"a\"\n").unwrap();
        let over = parse_config("quality = 80\n[audio]\ncodec = \"opus\"\n").unwrap();
        let m = merge(base, over);
        assert_eq!(m.quality, Some(80)); // overlay wins
        assert_eq!(m.suffix.as_deref(), Some("a")); // base survives
        assert_eq!(m.audio.codec.as_deref(), Some("opus"));
    }

    #[test]
    fn find_walks_up_to_nearest() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        fs::write(root.join("squish.toml"), "quality = 1\n").unwrap();
        let nested = root.join("a/b/c");
        fs::create_dir_all(&nested).unwrap();

        let found = find_project_config(&nested).unwrap();
        assert_eq!(found, root.join("squish.toml"));

        // A nearer file shadows the outer one.
        fs::write(root.join("a/squish.toml"), "quality = 2\n").unwrap();
        let found = find_project_config(&nested).unwrap();
        assert_eq!(found, root.join("a/squish.toml"));
    }

    #[test]
    fn parses_overwrite_key() {
        let c = parse_config("overwrite = true\n").unwrap();
        assert_eq!(c.overwrite, Some(true));
    }

    #[test]
    fn overwrite_absent_is_none() {
        let c = parse_config("quality = 50\n").unwrap();
        assert_eq!(c.overwrite, None);
    }

    #[test]
    fn find_returns_none_when_absent() {
        let tmp = tempfile::TempDir::new().unwrap();
        assert_eq!(find_project_config(tmp.path()), None);
    }

    #[test]
    fn global_config_path_honors_env_override() {
        // SAFETY: single-threaded test; set then remove the override.
        std::env::set_var("SQUISH_GLOBAL_CONFIG", "/tmp/squish-test-xyz.toml");
        let p = global_config_path();
        std::env::remove_var("SQUISH_GLOBAL_CONFIG");
        assert_eq!(
            p,
            Some(std::path::PathBuf::from("/tmp/squish-test-xyz.toml"))
        );
    }
}
