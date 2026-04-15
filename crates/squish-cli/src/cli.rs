use clap::Parser;
use std::path::PathBuf;

/// Compress images losslessly or with sensible quality defaults.
#[derive(Parser, Debug)]
#[command(name = "squish", version, about)]
pub struct Args {
    /// Files or directories to compress.
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,

    /// Quality override, 0-100 (format-dependent default when omitted).
    #[arg(short = 'q', long, value_parser = clap::value_parser!(u8).range(0..=100))]
    pub quality: Option<u8>,

    /// Lossless compression (overrides --quality).
    #[arg(long)]
    pub lossless: bool,

    /// Output format: png, jpg/jpeg, webp, avif, svg, gif, heic, tiff.
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
}
