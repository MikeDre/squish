//! `squish doctor`: report what this install can do — built-in formats plus
//! the external tools (ffmpeg/ffprobe/gifsicle), with versions and install
//! hints. Always exits 0.

use std::io::Write;
use std::process::Command;

/// Result of probing one external tool.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ToolStatus {
    /// Present; `Some(version)` if a version was parsed, else `None`.
    Present(Option<String>),
    Missing,
}

/// One external tool: identity, what it powers, install hint, probe result.
struct ToolReport {
    name: &'static str,
    powers: &'static str,
    install: &'static str,
    status: ToolStatus,
}

/// Extract a version token from a tool's first `-version` line: the first
/// whitespace-separated token that starts with an ASCII digit and contains a
/// '.'. Returns None if there's no such token.
fn parse_version(first_line: &str) -> Option<String> {
    first_line
        .split_whitespace()
        .find(|tok| tok.starts_with(|c: char| c.is_ascii_digit()) && tok.contains('.'))
        .map(|s| s.to_string())
}

/// Render the full report text from already-probed tool reports. Pure: no I/O.
fn render_report(tools: &[ToolReport]) -> String {
    let mut out = String::new();
    out.push_str("squish doctor — what this install can do\n\n");

    out.push_str("Built in (always available):\n");
    out.push_str("  ✓ Images   PNG · JPEG · WebP · AVIF · SVG · TIFF · HEIC\n");
    out.push_str("  ✓ Code     JS · TS · CSS · HTML · JSON\n\n");

    out.push_str("External tools:\n");
    for t in tools {
        match &t.status {
            ToolStatus::Present(Some(v)) => {
                out.push_str(&format!("  ✓ {:<9} {:<8} → {}\n", t.name, v, t.powers));
            }
            ToolStatus::Present(None) => {
                out.push_str(&format!(
                    "  ✓ {:<9} {:<8} → {}\n",
                    t.name, "(present)", t.powers
                ));
            }
            ToolStatus::Missing => {
                out.push_str(&format!(
                    "  ✗ {:<9} {:<8} → {}\n",
                    t.name, "missing", t.powers
                ));
                out.push_str(&format!("       install: {}\n", t.install));
            }
        }
    }

    let missing: Vec<&ToolReport> = tools
        .iter()
        .filter(|t| t.status == ToolStatus::Missing)
        .collect();
    out.push('\n');
    if missing.is_empty() {
        out.push_str("All external tools installed — every format is supported.\n");
    } else {
        for t in &missing {
            out.push_str(&format!(
                "{} is unavailable until {} is installed.\n",
                t.powers, t.name
            ));
        }
    }
    out
}

/// Probe one tool by running `<name> <version_arg>`. Returns `Missing` if the
/// command can't be spawned or exits non-zero; otherwise `Present` with the
/// version parsed from the first stdout line (or `None` if unparseable).
fn probe_tool(name: &str, version_arg: &str) -> ToolStatus {
    match Command::new(name).arg(version_arg).output() {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let first = stdout.lines().next().unwrap_or("");
            ToolStatus::Present(parse_version(first))
        }
        _ => ToolStatus::Missing,
    }
}

/// Whether the installed ffmpeg's `-filters` output lists `libvmaf` — a
/// compile-time filter, not a standalone binary, so this can't use
/// `probe_tool`'s "run `<name> <version_arg>`" shape.
fn probe_libvmaf() -> ToolStatus {
    match Command::new("ffmpeg").arg("-filters").output() {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.contains("libvmaf") {
                ToolStatus::Present(None)
            } else {
                ToolStatus::Missing
            }
        }
        _ => ToolStatus::Missing,
    }
}

/// Entry point for `squish doctor`: probe the external tools, render, print.
pub fn run() -> anyhow::Result<u8> {
    let ffmpeg_hint = "brew install ffmpeg (macOS) · apt install ffmpeg (Linux)";
    let tools = vec![
        ToolReport {
            name: "ffmpeg",
            powers: "video and audio compression",
            install: ffmpeg_hint,
            status: probe_tool("ffmpeg", "-version"),
        },
        ToolReport {
            name: "ffprobe",
            powers: "audio stream / codec detection",
            install: ffmpeg_hint,
            status: probe_tool("ffprobe", "-version"),
        },
        ToolReport {
            name: "gifsicle",
            powers: "GIF compression",
            install: "brew install gifsicle (macOS) · apt install gifsicle (Linux)",
            status: probe_tool("gifsicle", "--version"),
        },
        ToolReport {
            name: "libvmaf",
            powers: "--quality auto for video (VMAF scoring)",
            install: "reinstall/upgrade ffmpeg with libvmaf support (Homebrew's ffmpeg \
                      formula includes it by default: brew reinstall ffmpeg)",
            status: probe_libvmaf(),
        },
    ];

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    write!(out, "{}", render_report(&tools))?;
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(name: &'static str, powers: &'static str, status: ToolStatus) -> ToolReport {
        ToolReport {
            name,
            powers,
            install: "brew install X (macOS) · apt install X (Linux)",
            status,
        }
    }

    #[test]
    fn parse_version_ffmpeg() {
        assert_eq!(
            parse_version("ffmpeg version 7.1 Copyright (c) 2000-2024"),
            Some("7.1".to_string())
        );
    }

    #[test]
    fn parse_version_gifsicle() {
        assert_eq!(
            parse_version("LCDF Gifsicle 1.94"),
            Some("1.94".to_string())
        );
    }

    #[test]
    fn parse_version_distro_suffix() {
        assert_eq!(
            parse_version("ffmpeg version 4.4.2-0ubuntu0.22.04.1 Copyright"),
            Some("4.4.2-0ubuntu0.22.04.1".to_string())
        );
    }

    #[test]
    fn parse_version_none_when_absent() {
        assert_eq!(parse_version("some tool with no version token"), None);
    }

    #[test]
    fn render_all_present_says_all_good_and_no_hints() {
        let tools = vec![
            report(
                "ffmpeg",
                "video and audio compression",
                ToolStatus::Present(Some("7.1".into())),
            ),
            report(
                "gifsicle",
                "GIF compression",
                ToolStatus::Present(Some("1.94".into())),
            ),
        ];
        let out = render_report(&tools);
        assert!(out.contains("✓ ffmpeg"));
        assert!(out.contains("7.1"));
        assert!(out.contains("All external tools installed"));
        assert!(!out.contains("install:"));
        assert!(out.contains("Images"));
        assert!(out.contains("Code"));
    }

    #[test]
    fn render_missing_gifsicle_shows_cross_hint_and_unavailable_line() {
        let tools = vec![
            report(
                "ffmpeg",
                "video and audio compression",
                ToolStatus::Present(Some("7.1".into())),
            ),
            report("gifsicle", "GIF compression", ToolStatus::Missing),
        ];
        let out = render_report(&tools);
        assert!(out.contains("✓ ffmpeg"));
        assert!(out.contains("✗ gifsicle"));
        assert!(out.contains("missing"));
        assert!(out.contains("install:"));
        assert!(out.contains("GIF compression is unavailable until gifsicle is installed."));
        assert!(!out.contains("All external tools installed"));
    }

    #[test]
    fn render_present_unknown_version() {
        let tools = vec![report(
            "ffmpeg",
            "video and audio compression",
            ToolStatus::Present(None),
        )];
        let out = render_report(&tools);
        assert!(out.contains("✓ ffmpeg"));
        assert!(out.contains("(present)"));
        assert!(!out.contains("missing"));
    }

    #[test]
    fn render_shows_libvmaf_present() {
        let tools = vec![report(
            "libvmaf",
            "--quality auto for video (VMAF scoring)",
            ToolStatus::Present(None),
        )];
        let out = render_report(&tools);
        assert!(out.contains("✓ libvmaf"));
        assert!(out.contains("(present)"));
    }

    #[test]
    fn render_shows_libvmaf_missing_with_hint() {
        let tools = vec![report(
            "libvmaf",
            "--quality auto for video (VMAF scoring)",
            ToolStatus::Missing,
        )];
        let out = render_report(&tools);
        assert!(out.contains("✗ libvmaf"));
        assert!(out.contains(
            "--quality auto for video (VMAF scoring) is unavailable until libvmaf is installed."
        ));
    }
}
