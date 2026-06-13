mod cli;
mod config;
mod finder_action;
mod format_request;
mod runner;
mod stats;
mod target_size;
mod walker;
mod watch;

use anyhow::Result;
use clap::Parser;
use squish_audio::{AudioFormat, AudioOptions};
use squish_code::CodeOptions;
use squish_core::SquishOptions;
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
    let mut args = cli::Args::parse();

    if args.stats {
        let now = chrono::Local::now();
        let records = stats::default_data_file()
            .and_then(|p| stats::load_records(&p).ok())
            .unwrap_or_default();
        print!("{}", stats::render_report(&records, now));
        return Ok(0);
    }

    let file_cfg = if args.no_config {
        config::FileConfig::default()
    } else {
        load_file_config()?
    };
    apply_file_config(&mut args, &file_cfg);
    let args = args;

    let kinds = match args.kinds.as_deref() {
        None => runner::KindFilter::default(),
        Some(s) => runner::parse_kinds(s).map_err(|e| anyhow::anyhow!(e))?,
    };

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

    let requested_format = if let Some(f) = &args.format {
        let req = format_request::RequestedFormat::parse(f)
            .ok_or_else(|| anyhow::anyhow!("unknown --format value: {f}"))?;
        // Audio sanity: reject wav/aiff up front (no PCM codec yet) only
        // when audio is the SOLE matching kind — otherwise non-audio inputs
        // can still use the requested format and audio inputs (if any) will
        // be caught later per-file.
        if let Err(msg) = req.validate_audio_output() {
            if req.image.is_none() && req.video.is_none() {
                anyhow::bail!("{msg}");
            }
        }
        Some(req)
    } else {
        None
    };

    let target_size = match args.target_size.as_deref() {
        None => None,
        Some(s) => Some(target_size::parse_target_size(s).ok_or_else(|| {
            anyhow::anyhow!(
                "--target-size must be a positive size like 500k, 1.5M, or 800000 (got: {s})"
            )
        })?),
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

    has_video &= kinds.video;
    has_audio &= kinds.audio;

    let (mut video_codec_override, mut audio_codec_override) =
        runner::validate_codec_string(args.codec.as_deref(), has_video, has_audio)?;

    // Config-supplied codecs are ambient per-kind defaults: they fill in only
    // when --codec didn't, and are not validated against the batch contents
    // (a project config's video codec must not error on an image-only run).
    if video_codec_override.is_none() {
        if let Some(s) = &file_cfg.video.codec {
            video_codec_override = Some(
                squish_video::VideoCodec::parse(s)
                    .ok_or_else(|| anyhow::anyhow!("squish.toml: unknown video codec: {s}"))?,
            );
        }
    }
    if audio_codec_override.is_none() {
        if let Some(s) = &file_cfg.audio.codec {
            audio_codec_override = Some(
                squish_audio::AudioCodec::parse(s)
                    .ok_or_else(|| anyhow::anyhow!("squish.toml: unknown audio codec: {s}"))?,
            );
        }
    }

    // If --format implies an audio codec (and is compatible with any
    // explicit --codec), apply it so the audio pipeline uses the right codec
    // and the lossless-input prompt sees codec=Some and skips.
    if let Some(req) = &requested_format {
        if let Some(audio_target) = req.audio {
            if !matches!(audio_target, AudioFormat::Wav | AudioFormat::Aiff) {
                let resolved =
                    format_request::resolve_audio_codec(audio_target, audio_codec_override)
                        .map_err(|e| anyhow::anyhow!(e))?;
                audio_codec_override = Some(resolved);
            }
        }
    }

    let opts = SquishOptions {
        quality: args.quality,
        lossless: args.lossless,
        output_format: requested_format.as_ref().and_then(|r| r.image),
        force_overwrite: args.force,
        max_width: args.max_width,
        max_height: args.max_height,
        suffix: args.suffix.clone(),
        overwrite: args.overwrite,
        target_size,
    };

    let video_opts = VideoOptions {
        quality: args.quality,
        codec: video_codec_override,
        fast: args.fast,
        force_overwrite: args.force,
        suffix: args.suffix.clone(),
        overwrite: args.overwrite,
        output_format: requested_format.as_ref().and_then(|r| r.video),
        target_size,
    };

    let audio_opts = AudioOptions {
        quality: args.quality,
        bitrate_kbps,
        codec: audio_codec_override,
        strip_tags: args.strip_tags,
        force_overwrite: args.force,
        suffix: args.suffix.clone(),
        overwrite: args.overwrite,
        output_format: requested_format.as_ref().and_then(|r| r.audio),
        target_size,
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
        kinds,
    };
    if args.watch {
        watch::run_watch(&args.paths, &cfg, args.recursive, args.no_stats)?;
        return Ok(0);
    }

    let report = runner::run(&worklist, &cfg)?;
    stats::append_batch(&report, args.dry_run, args.no_stats);
    Ok(report.exit_code())
}

/// Load and merge config files: the global config (overridable via the
/// SQUISH_GLOBAL_CONFIG env var, mainly for tests) under the nearest
/// project-level squish.toml found from the current directory upward.
fn load_file_config() -> Result<config::FileConfig> {
    let mut cfg = config::FileConfig::default();

    let global_path = std::env::var_os("SQUISH_GLOBAL_CONFIG")
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::config_dir().map(|d| d.join("squish/config.toml")));
    if let Some(p) = global_path {
        if p.is_file() {
            let text = std::fs::read_to_string(&p)?;
            cfg =
                config::parse_config(&text).map_err(|e| anyhow::anyhow!("{}: {e}", p.display()))?;
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        if let Some(p) = config::find_project_config(&cwd) {
            let text = std::fs::read_to_string(&p)?;
            let project =
                config::parse_config(&text).map_err(|e| anyhow::anyhow!("{}: {e}", p.display()))?;
            cfg = config::merge(cfg, project);
        }
    }

    Ok(cfg)
}

/// Fill unset CLI args from the merged config. CLI flags always win. Rate
/// control is all-or-nothing: passing any of --quality/--lossless/--bitrate/
/// --fast/--target-size on the CLI disables every config rate-control key, so
/// a config target-size can't sneak past an explicit --quality (clap's
/// conflict rules only see real CLI flags).
fn apply_file_config(args: &mut cli::Args, cfg: &config::FileConfig) {
    let cli_rate_control = args.quality.is_some()
        || args.lossless
        || args.bitrate.is_some()
        || args.fast
        || args.target_size.is_some();
    if !cli_rate_control {
        args.quality = cfg.quality;
        args.lossless = cfg.lossless.unwrap_or(false);
        args.target_size = cfg.target_size.clone();
        args.bitrate = cfg.audio.bitrate.clone();
        args.fast = cfg.video.fast.unwrap_or(false);
    }

    if args.format.is_none() {
        args.format = cfg.format.clone();
    }
    if !args.recursive {
        args.recursive = cfg.recursive.unwrap_or(false);
    }
    // --overwrite conflicts with --suffix on the CLI; respect that here too.
    if args.suffix.is_none() && !args.overwrite {
        args.suffix = cfg.suffix.clone();
    }
    if args.jobs.is_none() {
        args.jobs = cfg.jobs;
    }
    if args.max_width.is_none() {
        args.max_width = cfg.max_width;
    }
    if args.max_height.is_none() {
        args.max_height = cfg.max_height;
    }
    if !args.strip_tags {
        args.strip_tags = cfg.audio.strip_tags.unwrap_or(false);
    }
    if !args.safe {
        args.safe = cfg.code.safe.unwrap_or(false);
    }
    if !args.source_map {
        args.source_map = cfg.code.source_map.unwrap_or(false);
    }
}
