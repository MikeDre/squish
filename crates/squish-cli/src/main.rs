mod cli;
mod config;
mod config_wizard;
mod doctor;
mod finder_action;
mod format_request;
mod json_report;
mod preset;
mod runner;
mod select;
mod stats;
mod target_size;
mod walker;
mod watch;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use squish_audio::{AudioFormat, AudioOptions};
use squish_code::CodeOptions;
use squish_core::{CropSpec, SquishOptions};
use squish_video::VideoOptions;

/// How long a `--select` run waits for the browser to read its result before
/// exiting. `--select` requires a TTY and can never be scripted, and the page
/// polls every 300ms, so this is one poll interval of slack in the common case
/// and a bounded worst case when the tab has been backgrounded.
const SELECT_REPORT_LINGER: std::time::Duration = std::time::Duration::from_millis(1500);

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

    match &args.command {
        Some(cli::Command::FinderAction(cmd)) => return finder_action::run(cmd),
        Some(cli::Command::Config { local }) => return config_wizard::run(*local),
        Some(cli::Command::Doctor) => return doctor::run(),
        Some(cli::Command::Completions { shell }) => {
            clap_complete::generate(
                *shell,
                &mut cli::Args::command(),
                "squish",
                &mut std::io::stdout(),
            );
            return Ok(0);
        }
        Some(cli::Command::Man) => {
            let man = clap_mangen::Man::new(cli::Args::command());
            man.render(&mut std::io::stdout())?;
            return Ok(0);
        }
        None => {}
    }

    if args.stats {
        let now = chrono::Local::now();
        let records = stats::default_data_file()
            .and_then(|p| stats::load_records(&p).ok())
            .unwrap_or_default();
        print!("{}", stats::render_report(&records, now));
        return Ok(0);
    }

    let mut file_cfg = if args.no_config {
        config::FileConfig::default()
    } else {
        load_file_config()?
    };
    if let Some(preset) = args.preset {
        preset::apply_preset(&mut args, &mut file_cfg, preset);
    }
    apply_file_config(&mut args, &file_cfg);
    let args = args;
    let file_cfg = file_cfg;

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

    // Split --quality into the numeric value the encoders use and the auto
    // flag. Auto drives a perceptual search for both images (SSIMULACRA2) and
    // video (VMAF); audio has no perceptual auto and always gets a plain
    // numeric quality (None when auto, so the codec default applies).
    let (image_quality, auto): (Option<u8>, bool) = match args.quality {
        Some(cli::QualityArg::Fixed(n)) => (Some(n), false),
        Some(cli::QualityArg::Auto) => (None, true),
        None => (None, false),
    };
    let av_quality = image_quality;

    let exclude_opts = walker::ExcludeOptions {
        globs: args.exclude.clone(),
        gitignore: args.gitignore,
        no_default_excludes: args.no_default_excludes,
    };
    let worklist = walker::collect_worklist(&args.paths, args.recursive, &exclude_opts);

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

    let mut opts = SquishOptions {
        quality: image_quality,
        lossless: args.lossless,
        output_format: requested_format.as_ref().and_then(|r| r.image),
        force_overwrite: args.force,
        max_width: args.max_width,
        max_height: args.max_height,
        width: args.width,
        height: args.height,
        suffix: args.suffix.clone(),
        overwrite: args.overwrite,
        target_size,
        auto,
        keep_metadata: args.keep_metadata,
        crop: args.crop,
        gravity: args.gravity,
    };

    // Kept alive for the whole run: the page is still open and polling, and this
    // is how it learns what happened.
    let mut reporter: Option<select::Reporter> = None;
    let mut crop_label = String::new();
    let mut select_source: Option<(String, u64)> = None;

    if args.select {
        // `opts` here is the run's settings *before* the crop is chosen, which
        // is exactly what the live estimate must encode with.
        let sel = select::resolve_crop(&worklist, &args, &opts)?;
        let source_dims = sel.source;
        reporter = sel.reporter;
        let source = &worklist[0];
        select_source = Some((
            source
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            std::fs::metadata(source).map(|m| m.len()).unwrap_or(0),
        ));
        match sel.rect {
            Some(r) => {
                let full = r.x == 0 && r.y == 0 && r.w == source_dims.0 && r.h == source_dims.1;
                if full {
                    println!("selection covers the whole image — no crop applied");
                } else {
                    // Echo the resolved rect so an interactive choice can be
                    // replayed non-interactively: --crop 1440x810+240+120
                    println!("crop: {}x{}+{}+{}", r.w, r.h, r.x, r.y);
                    crop_label = format!("{}x{}+{}+{}", r.w, r.h, r.x, r.y);
                    opts.crop = Some(CropSpec::Exact {
                        w: r.w,
                        h: r.h,
                        x: r.x,
                        y: r.y,
                    });
                }
            }
            None => {
                println!("crop cancelled — nothing written");
                return Ok(0);
            }
        }
    }
    let opts = opts;

    let video_opts = VideoOptions {
        quality: av_quality,
        codec: video_codec_override,
        fast: args.fast,
        force_overwrite: args.force,
        suffix: args.suffix.clone(),
        overwrite: args.overwrite,
        output_format: requested_format.as_ref().and_then(|r| r.video),
        target_size,
        quality_auto: auto,
    };

    let audio_opts = AudioOptions {
        quality: av_quality,
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
        json: args.json,
        overwrite: args.overwrite,
        kinds,
        skip_format_kind_check: args.preset.is_some(),
    };
    if args.watch {
        watch::run_watch(
            &args.paths,
            &cfg,
            args.recursive,
            args.no_stats,
            &exclude_opts,
        )?;
        return Ok(0);
    }

    // Captured, not `?`-propagated: an early return here would drop the
    // selector's reporter with the page still showing "working".
    let run = runner::run(&worklist, &cfg);
    if let Some(rep) = &reporter {
        let (name, bytes) = select_source.unwrap_or_default();
        rep.finish(select::report_phase(
            &run,
            &name,
            bytes,
            &crop_label,
            args.dry_run,
        ));
        rep.wait_for_pickup(SELECT_REPORT_LINGER);
    }
    let report = run?;
    stats::append_batch(&report, args.dry_run, args.no_stats);
    Ok(report.exit_code())
}

/// Load and merge config files: the global config (overridable via the
/// SQUISH_GLOBAL_CONFIG env var, mainly for tests) under the nearest
/// project-level squish.toml found from the current directory upward.
fn load_file_config() -> Result<config::FileConfig> {
    let mut cfg = config::FileConfig::default();

    if let Some(p) = config::global_config_path() {
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
    // Config overwrite applies only when the CLI passed neither -o nor
    // --suffix (a CLI --suffix blocks it because args.suffix.is_none() is
    // then false). Must come before the suffix block so that config
    // overwrite=true also suppresses a config suffix, matching the CLI's own
    // conflicts_with constraint between -o and --suffix.
    if !args.overwrite && args.suffix.is_none() {
        args.overwrite = cfg.overwrite.unwrap_or(false);
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
    if args.exclude.is_empty() {
        args.exclude = cfg.exclude.clone().unwrap_or_default();
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
