//! Interactive `squish config` wizard.

use crate::cli::QualityArg;
use crate::config::{self, FileConfig};
use anyhow::{Context, Result};
use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;

use crate::format_request::RequestedFormat;

/// Outcome of reading one prompt line.
enum Answer {
    Keep,
    Clear,
    Value(String),
}

/// Read a line; classify it. Returns Err on EOF (so a truncated interactive
/// session fails loudly instead of writing partial config).
fn read_answer(input: &mut impl BufRead) -> std::io::Result<Answer> {
    let mut line = String::new();
    let n = input.read_line(&mut line)?;
    if n == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "input ended unexpectedly",
        ));
    }
    let t = line.trim();
    Ok(match t {
        "" => Answer::Keep,
        "-" => Answer::Clear,
        other => Answer::Value(other.to_string()),
    })
}

fn current_hint(v: Option<&str>) -> String {
    match v {
        Some(s) => format!("[current: {s}]"),
        None => "[current: unset]".to_string(),
    }
}

fn bool_hint(v: Option<bool>) -> String {
    match v {
        Some(true) => "[current: y]".to_string(),
        Some(false) => "[current: n]".to_string(),
        None => "[current: n]".to_string(),
    }
}

fn prompt_quality(
    cur: Option<QualityArg>,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> std::io::Result<Option<QualityArg>> {
    let cur_str = match cur {
        Some(QualityArg::Fixed(n)) => Some(n.to_string()),
        Some(QualityArg::Auto) => Some("auto".to_string()),
        None => None,
    };
    loop {
        write!(
            out,
            "quality (0–100) {}: ",
            current_hint(cur_str.as_deref())
        )?;
        out.flush()?;
        match read_answer(input)? {
            Answer::Keep => return Ok(cur),
            Answer::Clear => return Ok(None),
            Answer::Value(v) => match v.parse::<u8>() {
                Ok(q) if q <= 100 => return Ok(Some(QualityArg::Fixed(q))),
                _ => {
                    writeln!(out, "  please enter a whole number from 0 to 100")?;
                }
            },
        }
    }
}

fn prompt_format(
    cur: Option<String>,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> std::io::Result<Option<String>> {
    loop {
        write!(
            out,
            "output format (png/jpeg/webp/avif/svg/gif/heic/tiff) {}: ",
            current_hint(cur.as_deref())
        )?;
        out.flush()?;
        match read_answer(input)? {
            Answer::Keep => return Ok(cur),
            Answer::Clear => return Ok(None),
            Answer::Value(v) => {
                if RequestedFormat::parse(&v).is_some() {
                    return Ok(Some(v));
                }
                writeln!(out, "  unknown format: {v}")?;
            }
        }
    }
}

fn prompt_suffix(
    cur: Option<String>,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> std::io::Result<Option<String>> {
    write!(out, "output suffix {}: ", current_hint(cur.as_deref()))?;
    out.flush()?;
    Ok(match read_answer(input)? {
        Answer::Keep => cur,
        Answer::Clear => None,
        Answer::Value(v) => Some(v),
    })
}

fn prompt_bool(
    label: &str,
    cur: Option<bool>,
    warning: Option<&str>,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> std::io::Result<Option<bool>> {
    if let Some(w) = warning {
        writeln!(out, "{w}")?;
    }
    loop {
        write!(out, "{label} (y/N) {}: ", bool_hint(cur))?;
        out.flush()?;
        match read_answer(input)? {
            Answer::Keep => return Ok(cur),
            Answer::Clear => return Ok(None),
            Answer::Value(v) => match v.to_ascii_lowercase().as_str() {
                "y" | "yes" => return Ok(Some(true)),
                "n" | "no" => return Ok(Some(false)),
                _ => {
                    writeln!(out, "  please answer y or n")?;
                }
            },
        }
    }
}

const OVERWRITE_WARNING: &str =
    "   ⚠  this replaces your original files everywhere squish runs, including\n      Finder right-click. No _squished copy is kept. Leave as N if unsure.";

/// Pure, testable wizard core: render prompts against `existing`, read answers
/// from `input`, write prompt text to `out`, return the updated config.
/// Only the curated fields are touched; all other keys in `existing` survive.
pub fn run_wizard(
    mut existing: FileConfig,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> std::io::Result<FileConfig> {
    existing.quality = prompt_quality(existing.quality, input, out)?;
    existing.format = prompt_format(existing.format.clone(), input, out)?;
    existing.suffix = prompt_suffix(existing.suffix.clone(), input, out)?;
    existing.recursive = prompt_bool(
        "recurse into directories by default?",
        existing.recursive,
        None,
        input,
        out,
    )?;
    existing.audio.strip_tags = prompt_bool(
        "strip audio tags by default?",
        existing.audio.strip_tags,
        None,
        input,
        out,
    )?;
    existing.overwrite = prompt_bool(
        "overwrite originals in place by default?",
        existing.overwrite,
        Some(OVERWRITE_WARNING),
        input,
        out,
    )?;
    Ok(existing)
}

/// Entry point for `squish config [--local]`.
pub fn run(local: bool) -> Result<u8> {
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("squish config requires an interactive terminal");
    }

    let target: PathBuf = if local {
        PathBuf::from("squish.toml")
    } else {
        config::global_config_path().context("cannot determine the global config path")?
    };

    let existing = if target.is_file() {
        let text = std::fs::read_to_string(&target)
            .with_context(|| format!("reading {}", target.display()))?;
        config::parse_config(&text).map_err(|e| anyhow::anyhow!("{}: {e}", target.display()))?
    } else {
        config::FileConfig::default()
    };

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut input = stdin.lock();
    let mut out = stdout.lock();

    writeln!(out, "squish config — editing {}", target.display())?;
    writeln!(
        out,
        "Press Enter to keep the current value, type a new one to change it,\nor \"-\" to clear it. Note: existing comments in the file are not preserved.\n"
    )?;
    out.flush()?;

    let updated = run_wizard(existing, &mut input, &mut out)?;

    let toml_text = toml::to_string_pretty(&updated).context("serializing config")?;
    // For a bare relative path like "squish.toml" (the --local target),
    // Path::parent() returns Some("") rather than None; skip create_dir_all on
    // that empty path since it would error.
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
    }
    std::fs::write(&target, toml_text).with_context(|| format!("writing {}", target.display()))?;
    writeln!(out, "\nWrote {}", target.display())?;
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn run(existing: FileConfig, input: &str) -> (FileConfig, String) {
        let mut inp = Cursor::new(input.as_bytes().to_vec());
        let mut out: Vec<u8> = Vec::new();
        let cfg = run_wizard(existing, &mut inp, &mut out).unwrap();
        (cfg, String::from_utf8(out).unwrap())
    }

    fn base() -> FileConfig {
        FileConfig {
            quality: Some(QualityArg::Fixed(75)),
            format: Some("webp".to_string()),
            suffix: Some("min".to_string()),
            recursive: Some(true),
            ..Default::default()
        }
    }

    #[test]
    fn enter_on_every_prompt_keeps_existing() {
        let (cfg, _) = run(base(), "\n\n\n\n\n\n");
        assert_eq!(cfg.quality, Some(QualityArg::Fixed(75)));
        assert_eq!(cfg.format.as_deref(), Some("webp"));
        assert_eq!(cfg.suffix.as_deref(), Some("min"));
        assert_eq!(cfg.recursive, Some(true));
        assert_eq!(cfg.audio.strip_tags, None);
        assert_eq!(cfg.overwrite, None);
    }

    #[test]
    fn typed_values_update_fields() {
        let (cfg, _) = run(base(), "60\njpeg\nsmall\nn\ny\nn\n");
        assert_eq!(cfg.quality, Some(QualityArg::Fixed(60)));
        assert_eq!(cfg.format.as_deref(), Some("jpeg"));
        assert_eq!(cfg.suffix.as_deref(), Some("small"));
        assert_eq!(cfg.recursive, Some(false));
        assert_eq!(cfg.audio.strip_tags, Some(true));
        assert_eq!(cfg.overwrite, Some(false));
    }

    #[test]
    fn invalid_quality_reprompts_then_accepts() {
        let (cfg, out) = run(base(), "999\nabc\n42\n\n\n\n\n\n");
        assert_eq!(cfg.quality, Some(QualityArg::Fixed(42)));
        assert!(
            out.matches("quality").count() >= 3,
            "should re-prompt quality"
        );
    }

    #[test]
    fn invalid_format_reprompts() {
        let (cfg, _) = run(base(), "\njpegg\navif\n\n\n\n\n");
        assert_eq!(cfg.format.as_deref(), Some("avif"));
    }

    #[test]
    fn dash_clears_a_field() {
        let (cfg, _) = run(base(), "-\n-\n-\n-\n-\n-\n");
        assert_eq!(cfg.quality, None);
        assert_eq!(cfg.format, None);
        assert_eq!(cfg.suffix, None);
        assert_eq!(cfg.recursive, None);
        assert_eq!(cfg.audio.strip_tags, None);
        assert_eq!(cfg.overwrite, None);
    }

    #[test]
    fn overwrite_prompt_shows_warning() {
        let (_, out) = run(base(), "\n\n\n\n\n\n");
        assert!(out.to_lowercase().contains("replaces your original"));
    }

    #[test]
    fn unprompted_keys_are_preserved() {
        let mut c = base();
        c.max_width = Some(2000);
        c.video.codec = Some("h264".to_string());
        let (cfg, _) = run(c, "\n\n\n\n\n\n");
        assert_eq!(cfg.max_width, Some(2000));
        assert_eq!(cfg.video.codec.as_deref(), Some("h264"));
    }

    #[test]
    fn eof_midway_errors() {
        let mut inp = Cursor::new(b"\n\n".to_vec());
        let mut out: Vec<u8> = Vec::new();
        assert!(run_wizard(base(), &mut inp, &mut out).is_err());
    }

    #[test]
    fn whitespace_around_input_is_trimmed() {
        let (cfg, _) = run(base(), "  60  \n\n\n\n\n\n");
        assert_eq!(cfg.quality, Some(QualityArg::Fixed(60)));
    }

    #[test]
    fn uppercase_yes_parses_as_true() {
        // quality/format/suffix kept, recursive = YES, then keep rest.
        let (cfg, _) = run(base(), "\n\n\nYES\n\n\n");
        assert_eq!(cfg.recursive, Some(true));
    }
}
