use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Value of `--quality`: a fixed 0–100 number, or `auto` (perceptual
/// visually-lossless search, image formats only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityArg {
    Fixed(u8),
    Auto,
}

/// clap value parser for `--quality`: accepts `auto` (any case) or 0..=100.
fn parse_quality(s: &str) -> Result<QualityArg, String> {
    if s.eq_ignore_ascii_case("auto") {
        return Ok(QualityArg::Auto);
    }
    match s.parse::<u8>() {
        Ok(n) if n <= 100 => Ok(QualityArg::Fixed(n)),
        _ => Err(format!("expected a number 0-100 or \"auto\", got \"{s}\"")),
    }
}

/// Compress images losslessly or with sensible quality defaults.
#[derive(Parser, Debug)]
#[command(name = "squish", version, about, subcommand_negates_reqs = true)]
pub struct Args {
    /// Subcommands (currently only finder-action).
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Files or directories to compress.
    #[arg(required_unless_present = "stats")]
    pub paths: Vec<PathBuf>,

    /// Quality: 0-100, or `auto` for the lowest visually-lossless quality
    /// (image formats only). Auto conflicts with --target-size.
    #[arg(short = 'q', long, value_parser = parse_quality)]
    pub quality: Option<QualityArg>,

    /// Lossless compression (overrides --quality).
    #[arg(long)]
    pub lossless: bool,

    /// Output format. Image: png, jpg/jpeg, webp, avif, svg, gif, heic, tiff.
    /// Video: mp4, webm, mov, avi, mkv, flv. Audio: mp3, m4a, opus, ogg, flac, webm.
    /// Applied per input kind; other kinds use their defaults.
    #[arg(short = 'f', long)]
    pub format: Option<String>,

    /// Recurse into directories.
    #[arg(short = 'r', long)]
    pub recursive: bool,

    /// Overwrite existing _squished files instead of appending _2, _3, ...
    #[arg(long)]
    pub force: bool,

    /// Show what would happen; don't write anything.
    #[arg(long = "dry-run")]
    pub dry_run: bool,

    /// Parallelism (default: number of CPUs).
    #[arg(short = 'j', long)]
    pub jobs: Option<usize>,

    /// Per-file output.
    #[arg(short = 'v', long)]
    pub verbose: bool,

    /// Errors only (no short flag to avoid conflict with --quality).
    #[arg(long)]
    pub quiet: bool,

    /// Custom output suffix (default: "squished", e.g. dog_squished.png).
    #[arg(long)]
    pub suffix: Option<String>,

    /// Replace each input file in place with its squished version.
    /// Skips any file whose squish would change the extension.
    #[arg(short = 'o', long, conflicts_with = "suffix")]
    pub overwrite: bool,

    /// Maximum output width in pixels (scales down proportionally).
    #[arg(long)]
    pub max_width: Option<u32>,

    /// Maximum output height in pixels (scales down proportionally).
    #[arg(long)]
    pub max_height: Option<u32>,

    /// Codec: video=h264|h265|av1|vp9, audio=mp3|aac|opus|vorbis|flac|alac (default: kind-specific).
    #[arg(long)]
    pub codec: Option<String>,

    /// Video fast mode: optimise without re-encoding.
    #[arg(long)]
    pub fast: bool,

    /// Audio bitrate, e.g. 128k, 192k. Overrides --quality for lossy audio.
    #[arg(long)]
    pub bitrate: Option<String>,

    /// Target output size per file, e.g. 500k, 1.5M, 2g (decimal units).
    /// Images search for the best quality that fits; video/audio compute a
    /// bitrate from the input's duration. Not applicable to code files.
    #[arg(
        long = "target-size",
        conflicts_with_all = ["quality", "lossless", "bitrate", "fast"]
    )]
    pub target_size: Option<String>,

    /// Strip audio metadata (ID3 tags, album art). Default: preserved.
    #[arg(long = "strip-tags")]
    pub strip_tags: bool,

    /// Code: skip mangling and dead-code elimination (whitespace-only).
    #[arg(long)]
    pub safe: bool,

    /// Code: emit a .map file alongside minified output (JS/TS/CSS only).
    #[arg(long = "source-map")]
    pub source_map: bool,

    /// Print usage report (files squished, bytes saved this month and all-time) and exit.
    #[arg(long)]
    pub stats: bool,

    /// Don't record this run in the local usage ledger.
    /// Also respected via SQUISH_NO_STATS env var.
    #[arg(long = "no-stats")]
    pub no_stats: bool,

    /// Ignore squish.toml config files for this run.
    #[arg(long = "no-config")]
    pub no_config: bool,

    /// Keep running: watch the given paths and squish files as they appear
    /// or change. Stop with Ctrl-C.
    #[arg(long, conflicts_with_all = ["dry_run", "stats"])]
    pub watch: bool,

    /// Restrict the run to these file kinds, comma-separated.
    /// Kinds: image, video, audio, code. Default: all kinds.
    #[arg(long, value_name = "KINDS")]
    pub kinds: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Manage the macOS Finder Quick Action ("Right-click → Squish").
    #[command(subcommand, name = "finder-action")]
    FinderAction(FinderActionCmd),
    /// Interactively set squish defaults (writes the config file).
    Config {
        /// Write ./squish.toml in the current directory instead of the global config.
        #[arg(long)]
        local: bool,
    },
    /// Report which formats and external tools are available on this machine.
    Doctor,
}

#[derive(Subcommand, Debug)]
pub enum FinderActionCmd {
    /// Install the Quick Action into ~/Library/Services.
    Install,
    /// Remove the Quick Action.
    Uninstall,
}
