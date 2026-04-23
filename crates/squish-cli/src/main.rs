mod cli;
mod runner;
mod walker;

use anyhow::{Context, Result};
use clap::Parser;
use squish_core::{Format, SquishOptions};
use squish_video::{VideoCodec, VideoOptions};

fn main() -> std::process::ExitCode {
    match real_main() {
        Ok(exit) => std::process::ExitCode::from(exit),
        Err(e) => {
            eprintln!("ERROR: {e:#}");
            std::process::ExitCode::from(2)
        }
    }
}

fn real_main() -> Result<u8> {
    let args = cli::Args::parse();

    for p in &args.paths {
        if !p.exists() {
            anyhow::bail!("path does not exist: {}", p.display());
        }
    }

    if let Some(n) = args.jobs {
        rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global()
            .ok();
    }

    let output_format = if let Some(f) = &args.format {
        Some(Format::parse(f).context(format!("unknown --format value: {f}"))?)
    } else {
        None
    };

    let opts = SquishOptions {
        quality: args.quality,
        lossless: args.lossless,
        output_format,
        force_overwrite: args.force,
    };

    let video_codec = if let Some(c) = &args.codec {
        Some(VideoCodec::parse(c).context(format!("unknown --codec value: {c}"))?)
    } else {
        None
    };

    let video_opts = VideoOptions {
        quality: args.quality,
        codec: video_codec,
        fast: args.fast,
        force_overwrite: args.force,
    };

    let worklist = walker::collect_worklist(&args.paths, args.recursive);

    let cfg = runner::RunConfig {
        opts,
        video_opts,
        verbose: args.verbose,
        quiet: args.quiet,
        dry_run: args.dry_run,
    };
    let report = runner::run(&worklist, &cfg)?;
    Ok(report.exit_code())
}
