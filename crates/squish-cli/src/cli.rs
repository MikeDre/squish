use clap::Parser;
use std::path::PathBuf;

/// Compress images losslessly or with sensible quality defaults.
#[derive(Parser, Debug)]
#[command(name = "squish", version, about)]
pub struct Args {
    /// Files or directories to compress.
    #[arg(required_unless_present = "stats")]
    pub paths: Vec<PathBuf>,

    /// Quality override, 0-100 (format-dependent default when omitted).
    #[arg(short = 'q', long, value_parser = clap::value_parser!(u8).range(0..=100))]
    pub quality: Option<u8>,

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
}
