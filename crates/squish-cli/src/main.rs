mod cli;
mod runner;
mod stats;
mod walker;

use anyhow::{Context, Result};
use clap::Parser;
use squish_audio::AudioOptions;
use squish_code::CodeOptions;
use squish_core::{Format, SquishOptions};
use squish_video::VideoOptions;

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

    let bitrate_kbps = match args.bitrate.as_deref() {
        None => None,
        Some(s) => {
            let trimmed = s.trim_end_matches('k').trim_end_matches('K');
            Some(trimmed.parse::<u32>().map_err(|_| {
                anyhow::anyhow!("--bitrate must be a number optionally suffixed with k (e.g. 192k)")
            })?)
        }
    };

    let worklist = walker::collect_worklist(&args.paths, args.recursive);

    // Classify worklist to determine which kinds are present (for codec validation).
    let mut has_video = false;
    let mut has_audio = false;
    for p in &worklist {
        if let Some(audio_fmt) = squish_audio::detect_audio_format(p) {
            if audio_fmt.is_ambiguous() {
                match squish_audio::ffmpeg::ffprobe_kind(p) {
                    Ok(squish_audio::ProbeKind::HasVideo) => has_video = true,
                    Ok(squish_audio::ProbeKind::AudioOnly) => has_audio = true,
                    _ => has_audio = true,
                }
            } else {
                has_audio = true;
            }
            continue;
        }
        if squish_video::detect_video_format(p).is_some() {
            has_video = true;
        }
    }

    let (video_codec_override, audio_codec_override) =
        runner::validate_codec_string(args.codec.as_deref(), has_video, has_audio)?;

    let opts = SquishOptions {
        quality: args.quality,
        lossless: args.lossless,
        output_format,
        force_overwrite: args.force,
        max_width: args.max_width,
        max_height: args.max_height,
        suffix: args.suffix.clone(),
        overwrite: args.overwrite,
    };

    let video_opts = VideoOptions {
        quality: args.quality,
        codec: video_codec_override,
        fast: args.fast,
        force_overwrite: args.force,
        suffix: args.suffix.clone(),
        overwrite: args.overwrite,
    };

    let audio_opts = AudioOptions {
        quality: args.quality,
        bitrate_kbps,
        codec: audio_codec_override,
        strip_tags: args.strip_tags,
        force_overwrite: args.force,
        suffix: args.suffix.clone(),
        overwrite: args.overwrite,
    };

    let code_opts = CodeOptions {
        safe: args.safe,
        source_map: args.source_map,
        force_overwrite: args.force,
        suffix: args.suffix.clone(),
        overwrite: args.overwrite,
    };

    let cfg = runner::RunConfig {
        opts,
        video_opts,
        audio_opts,
        code_opts,
        verbose: args.verbose,
        quiet: args.quiet,
        dry_run: args.dry_run,
        overwrite: args.overwrite,
    };
    let report = runner::run(&worklist, &cfg)?;
    Ok(report.exit_code())
}
