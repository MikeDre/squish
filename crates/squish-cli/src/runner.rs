use anyhow::Result;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use rayon::prelude::*;
use squish_audio::AudioCodec;
use squish_audio::AudioError;
use squish_audio::{self, AudioOptions, AudioResult};
use squish_code::{self, CodeError, CodeOptions, CodeResult};
use squish_core::{squish_file, Format, SquishError, SquishOptions, SquishResult};
use squish_video::VideoCodec;
use squish_video::{self, VideoError, VideoFormat, VideoOptions, VideoResult};
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub struct RunConfig {
    pub opts: SquishOptions,
    pub video_opts: VideoOptions,
    pub audio_opts: AudioOptions,
    pub code_opts: CodeOptions,
    pub verbose: bool,
    pub quiet: bool,
    pub dry_run: bool,
    pub json: bool,
    pub overwrite: bool,
    pub kinds: KindFilter,
    /// When true (a preset is active), skip the "--format specifies X but no X
    /// files" batch-presence check — a preset applies to whatever kinds are
    /// present and must not error on absent ones.
    pub skip_format_kind_check: bool,
}

pub struct RunReport {
    pub results: Vec<SquishResult>,
    pub video_results: Vec<VideoResult>,
    pub audio_results: Vec<AudioResult>,
    pub code_results: Vec<CodeResult>,
    pub errors: Vec<(PathBuf, String)>,
    pub skipped_unknown: Vec<PathBuf>,
    /// Files where encoding produced no improvement (and no format/resize/
    /// codec conversion was requested, so growth wasn't expected/allowed):
    /// the encode was discarded and the output left byte-identical to the
    /// input. See `enforce_never_grow`.
    pub already_optimal_images: Vec<SquishResult>,
    pub already_optimal_video: Vec<VideoResult>,
    pub already_optimal_audio: Vec<AudioResult>,
    pub already_optimal_code: Vec<CodeResult>,
    pub total_wall: Duration,
}

impl RunReport {
    pub fn input_bytes(&self) -> u64 {
        let img: u64 = self.results.iter().map(|r| r.input_bytes).sum();
        let vid: u64 = self.video_results.iter().map(|r| r.input_bytes).sum();
        let aud: u64 = self.audio_results.iter().map(|r| r.input_bytes).sum();
        let cod: u64 = self.code_results.iter().map(|r| r.input_bytes).sum();
        img + vid + aud + cod
    }
    pub fn output_bytes(&self) -> u64 {
        let img: u64 = self.results.iter().map(|r| r.output_bytes).sum();
        let vid: u64 = self.video_results.iter().map(|r| r.output_bytes).sum();
        let aud: u64 = self.audio_results.iter().map(|r| r.output_bytes).sum();
        let cod: u64 = self.code_results.iter().map(|r| r.output_bytes).sum();
        img + vid + aud + cod
    }
    pub fn total_files(&self) -> usize {
        self.results.len()
            + self.video_results.len()
            + self.audio_results.len()
            + self.code_results.len()
    }
    pub fn total_already_optimal(&self) -> usize {
        self.already_optimal_images.len()
            + self.already_optimal_video.len()
            + self.already_optimal_audio.len()
            + self.already_optimal_code.len()
    }
    pub fn exit_code(&self) -> u8 {
        if self.errors.is_empty() {
            0
        } else {
            1
        }
    }
}

enum FileKind {
    Image,
    Video,
    Audio,
    Code,
    Unknown,
}

/// Outcome of the never-grow check (see `enforce_never_grow`) for one file.
enum NeverGrow {
    /// Output was smaller than input — the common case, nothing to do.
    Smaller,
    /// Output wasn't smaller, but a format/resize/codec conversion was
    /// requested, so growth is allowed (e.g. a tiny PNG icon converted to
    /// AVIF can legitimately grow). Caller prints a verbose-mode note.
    GrewButAllowed,
    /// Output wasn't smaller and no conversion was requested: the encode
    /// was discarded and `output_path` now holds a byte-identical copy of
    /// the original input.
    Skipped,
}

/// Central "never grow" enforcement (IMPLEMENTATION-BRIEFS.md Brief 12):
/// squish must never write an output larger than its input unless the user
/// asked for a transformation that legitimately changes representation
/// (format conversion, resize, codec change).
///
/// `original_bytes` must be `Some` whenever `overwrite` is true: in that
/// mode every encoder (image, video, audio, code) writes over — or renames
/// onto — `input_path` before returning, so by the time this runs the
/// original bytes are already gone from disk and can only be restored from
/// an in-memory copy taken *before* the encode call. When `overwrite` is
/// false, `input_path` is guaranteed untouched on disk and is read directly
/// instead — no in-memory cache needed for the common case.
fn enforce_never_grow(
    input_path: &Path,
    output_path: &Path,
    input_bytes: u64,
    output_bytes: u64,
    legitimate_transform: bool,
    overwrite: bool,
    original_bytes: Option<&[u8]>,
) -> std::io::Result<NeverGrow> {
    if output_bytes < input_bytes {
        return Ok(NeverGrow::Smaller);
    }
    if legitimate_transform {
        return Ok(NeverGrow::GrewButAllowed);
    }
    if overwrite {
        let bytes = original_bytes
            .expect("original bytes must be cached before encoding when overwrite is set");
        std::fs::write(output_path, bytes)?;
    } else if input_path != output_path {
        std::fs::copy(input_path, output_path)?;
    }
    Ok(NeverGrow::Skipped)
}

fn classify_file(path: &Path) -> FileKind {
    if peek_image_format(path).unwrap_or(None).is_some() {
        return FileKind::Image;
    }
    if let Some(audio_fmt) = squish_audio::detect_audio_format(path) {
        if audio_fmt.is_ambiguous() {
            match squish_audio::ffmpeg::ffprobe_kind(path) {
                Ok(squish_audio::ProbeKind::AudioOnly) => return FileKind::Audio,
                Ok(squish_audio::ProbeKind::HasVideo) => return FileKind::Video,
                _ => {}
            }
        }
        return FileKind::Audio;
    }
    if squish_video::detect_video_format(path).is_some() {
        return FileKind::Video;
    }
    if squish_code::detect_code_format(path).is_some() {
        return FileKind::Code;
    }
    FileKind::Unknown
}

/// Which file kinds a run may touch (`--kinds`). Default: all.
#[derive(Debug, Clone, Copy)]
pub struct KindFilter {
    pub image: bool,
    pub video: bool,
    pub audio: bool,
    pub code: bool,
}

impl Default for KindFilter {
    fn default() -> Self {
        Self {
            image: true,
            video: true,
            audio: true,
            code: true,
        }
    }
}

impl KindFilter {
    /// Unknown is always allowed so filtered runs still report
    /// unrecognized files in the summary.
    fn allows(&self, kind: &FileKind) -> bool {
        match kind {
            FileKind::Image => self.image,
            FileKind::Video => self.video,
            FileKind::Audio => self.audio,
            FileKind::Code => self.code,
            FileKind::Unknown => true,
        }
    }
}

/// Parse a `--kinds` value like "image,video,audio" into a `KindFilter`.
/// An empty value, or any unrecognized or empty comma-separated part, is an
/// error — consistent with config-file strictness (typos fail loudly).
pub fn parse_kinds(s: &str) -> Result<KindFilter, String> {
    if s.trim().is_empty() {
        return Err("--kinds value must not be empty".to_string());
    }
    let mut f = KindFilter {
        image: false,
        video: false,
        audio: false,
        code: false,
    };
    for part in s.split(',') {
        match part.trim() {
            "image" => f.image = true,
            "video" => f.video = true,
            "audio" => f.audio = true,
            "code" => f.code = true,
            other => {
                return Err(format!(
                    "unknown kind in --kinds: \"{other}\" (expected: image, video, audio, code)"
                ))
            }
        }
    }
    Ok(f)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodecAppliesTo {
    Video,
    Audio,
    Neither,
}

fn classify_codec_string(s: &str) -> CodecAppliesTo {
    let v = VideoCodec::parse(s).is_some();
    let a = AudioCodec::parse(s).is_some();
    match (v, a) {
        (true, _) => CodecAppliesTo::Video,
        (false, true) => CodecAppliesTo::Audio,
        (false, false) => CodecAppliesTo::Neither,
    }
}

/// Validate the user-supplied --codec against the batch contents. Returns
/// `Ok((maybe_video_codec, maybe_audio_codec))`.
pub fn validate_codec_string(
    codec: Option<&str>,
    has_video: bool,
    has_audio: bool,
) -> anyhow::Result<(Option<VideoCodec>, Option<AudioCodec>)> {
    let Some(s) = codec else {
        return Ok((None, None));
    };
    match classify_codec_string(s) {
        CodecAppliesTo::Video => {
            if !has_video {
                anyhow::bail!("--codec {s} is a video codec, but no video files in batch");
            }
            Ok((VideoCodec::parse(s), None))
        }
        CodecAppliesTo::Audio => {
            if !has_audio {
                anyhow::bail!("--codec {s} is an audio codec, but no audio files in batch");
            }
            Ok((None, AudioCodec::parse(s)))
        }
        CodecAppliesTo::Neither => {
            anyhow::bail!("--codec {s} is not a recognized video or audio codec");
        }
    }
}

fn is_lossless_input(path: &Path) -> bool {
    matches!(
        squish_audio::detect_audio_format(path),
        Some(squish_audio::AudioFormat::Flac)
            | Some(squish_audio::AudioFormat::Wav)
            | Some(squish_audio::AudioFormat::Aiff)
    )
}

fn prompt_lossy_codec() -> AudioCodec {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let _ = write!(
        stdout,
        "Found lossless audio. Convert to: [Opus] / AAC / MP3 > "
    );
    let _ = stdout.flush();

    let mut line = String::new();
    if stdin.lock().read_line(&mut line).is_err() {
        return AudioCodec::Opus;
    }
    match line.trim().to_ascii_lowercase().as_str() {
        "" | "opus" | "o" => AudioCodec::Opus,
        "aac" | "a" => AudioCodec::Aac,
        "mp3" | "m" => AudioCodec::Mp3,
        _ => AudioCodec::Opus,
    }
}

/// Choose the codec for lossless inputs when no explicit `--codec` was given.
/// TTY → ask; non-TTY → silent Opus default.
pub fn choose_lossless_codec(audio_files: &[PathBuf], audio_codec_set: bool) -> Option<AudioCodec> {
    if audio_codec_set {
        return None;
    }
    if !audio_files.iter().any(|p| is_lossless_input(p)) {
        return None;
    }
    if std::io::stdin().is_terminal() {
        Some(prompt_lossy_codec())
    } else {
        Some(AudioCodec::Opus)
    }
}

/// Validate `--source-map`: returns Err if set but no JS/TS/CSS files exist.
pub fn validate_source_map(source_map: bool, code_files: &[PathBuf]) -> anyhow::Result<()> {
    if !source_map {
        return Ok(());
    }
    let any_supports_map = code_files.iter().any(|p| {
        squish_code::detect_code_format(p)
            .map(|f| f.supports_source_map())
            .unwrap_or(false)
    });
    if !any_supports_map {
        anyhow::bail!(
            "--source-map requires at least one .js/.ts/.css/.tsx/.jsx/.mjs/.cjs/.mts/.cts file in the batch"
        );
    }
    Ok(())
}

pub fn run(paths: &[PathBuf], cfg: &RunConfig) -> Result<RunReport> {
    let start = Instant::now();

    let mut image_files = Vec::new();
    let mut video_files = Vec::new();
    let mut audio_files = Vec::new();
    let mut code_files = Vec::new();
    let mut skipped_unknown = Vec::new();

    for path in paths {
        let kind = classify_file(path);
        if !cfg.kinds.allows(&kind) {
            continue;
        }
        match kind {
            FileKind::Image => image_files.push(path.clone()),
            FileKind::Video => video_files.push(path.clone()),
            FileKind::Audio => audio_files.push(path.clone()),
            FileKind::Code => code_files.push(path.clone()),
            FileKind::Unknown => skipped_unknown.push(path.clone()),
        }
    }

    if let Err(msg) = validate_target_size_applicable(
        cfg.opts.target_size.is_some(),
        !image_files.is_empty() || !video_files.is_empty() || !audio_files.is_empty(),
        !code_files.is_empty(),
    ) {
        return Err(anyhow::anyhow!(msg));
    }

    // Skipped under a preset: a preset may request a format (e.g. webp) for a
    // kind not present in this batch (e.g. a code-only run); that must not error.
    if !cfg.skip_format_kind_check {
        if let Err(msg) = validate_format_kinds_present(
            cfg.opts.output_format.is_some(),
            cfg.video_opts.output_format.is_some(),
            cfg.audio_opts.output_format.is_some(),
            !image_files.is_empty(),
            !video_files.is_empty(),
            !audio_files.is_empty(),
        ) {
            return Err(anyhow::anyhow!(msg));
        }
    }

    if cfg.dry_run {
        if cfg.json {
            let report = crate::json_report::build_dry_run(
                &image_files,
                &video_files,
                &audio_files,
                &code_files,
                &skipped_unknown,
            );
            crate::json_report::print(&report);
        } else {
            for p in &image_files {
                println!("{}", dry_run_action(cfg.overwrite, "image", p));
            }
            for p in &video_files {
                println!("{}", dry_run_action(cfg.overwrite, "video", p));
            }
            for p in &audio_files {
                println!("{}", dry_run_action(cfg.overwrite, "audio", p));
            }
            for p in &code_files {
                println!("{}", dry_run_action(cfg.overwrite, "code", p));
            }
            for p in &skipped_unknown {
                println!("would skip (unrecognized): {}", p.display());
            }
        }
        return Ok(RunReport {
            results: Vec::new(),
            video_results: Vec::new(),
            audio_results: Vec::new(),
            code_results: Vec::new(),
            errors: Vec::new(),
            skipped_unknown,
            already_optimal_images: Vec::new(),
            already_optimal_video: Vec::new(),
            already_optimal_audio: Vec::new(),
            already_optimal_code: Vec::new(),
            total_wall: start.elapsed(),
        });
    }

    // Validate --source-map applicability against the batch.
    validate_source_map(cfg.code_opts.source_map, &code_files)?;

    // Lossless audio prompt (existing behavior).
    let mut audio_opts = cfg.audio_opts.clone();
    if audio_opts.codec.is_none() {
        if let Some(c) = choose_lossless_codec(&audio_files, false) {
            audio_opts.codec = Some(c);
        }
    }

    let total =
        (image_files.len() + video_files.len() + audio_files.len() + code_files.len()) as u64;
    let processed = AtomicU64::new(0);
    let progress = build_progress_bar(total, cfg);

    // Images in parallel.
    let image_pairs: Vec<(PathBuf, Result<SquishResult, SquishError>, bool)> = image_files
        .par_iter()
        .map(|path| {
            let original_bytes = if cfg.overwrite {
                std::fs::read(path).ok()
            } else {
                None
            };
            let mut res = squish_file(path, &cfg.opts);
            let mut downgraded = false;
            if let Ok(r) = &res {
                let legitimate = cfg.opts.output_format.is_some()
                    || cfg.opts.needs_resize()
                    || cfg.opts.target_size.is_some()
                    || r.format_in != r.format_out;
                match enforce_never_grow(
                    &r.input_path,
                    &r.output_path,
                    r.input_bytes,
                    r.output_bytes,
                    legitimate,
                    cfg.overwrite,
                    original_bytes.as_deref(),
                ) {
                    Ok(NeverGrow::Smaller) => {}
                    Ok(NeverGrow::GrewButAllowed) => {
                        if !cfg.quiet && cfg.verbose {
                            eprintln!(
                                "  note: output grew ({} → {} bytes) — conversion requested",
                                r.input_bytes, r.output_bytes
                            );
                        }
                    }
                    Ok(NeverGrow::Skipped) => downgraded = true,
                    Err(e) => res = Err(e.into()),
                }
            }
            if downgraded {
                if let Ok(r) = &mut res {
                    r.output_bytes = r.input_bytes;
                }
            }
            let n = processed.fetch_add(1, Ordering::SeqCst) + 1;
            if !cfg.quiet && cfg.verbose {
                match &res {
                    Ok(r) => {
                        eprintln!(
                            "[{n}/{total}] {} → {} ({:.1}% saved)",
                            path.display(),
                            r.output_path.display(),
                            r.reduction_percent()
                        );
                        for w in &r.warnings {
                            eprintln!("  WARNING: {w}");
                        }
                    }
                    Err(e) => eprintln!("[{n}/{total}] {}: ERROR {e}", path.display()),
                }
            }
            if !cfg.quiet && !cfg.verbose {
                if let Ok(r) = &res {
                    for w in &r.warnings {
                        eprintln!("WARNING: {w}");
                    }
                }
            }
            if let Some(pb) = &progress {
                pb.set_message(display_filename(path));
                pb.inc(1);
            }
            (path.clone(), res, downgraded)
        })
        .collect();

    // Videos sequentially.
    let mut video_pairs: Vec<(PathBuf, Result<VideoResult, VideoError>, bool)> = Vec::new();
    for path in &video_files {
        let original_bytes = if cfg.overwrite {
            std::fs::read(path).ok()
        } else {
            None
        };
        let mut res = squish_video::squish_video(path, &cfg.video_opts);
        let mut downgraded = false;
        if let Ok(r) = &res {
            let legitimate = cfg.video_opts.output_format.is_some()
                || cfg.video_opts.codec.is_some()
                || cfg.video_opts.target_size.is_some()
                || r.format_in != r.format_out;
            match enforce_never_grow(
                &r.input_path,
                &r.output_path,
                r.input_bytes,
                r.output_bytes,
                legitimate,
                cfg.overwrite,
                original_bytes.as_deref(),
            ) {
                Ok(NeverGrow::Smaller) => {}
                Ok(NeverGrow::GrewButAllowed) => {
                    if !cfg.quiet && cfg.verbose {
                        eprintln!(
                            "  note: output grew ({} → {} bytes) — conversion requested",
                            r.input_bytes, r.output_bytes
                        );
                    }
                }
                Ok(NeverGrow::Skipped) => downgraded = true,
                Err(e) => res = Err(e.into()),
            }
        }
        if downgraded {
            if let Ok(r) = &mut res {
                r.output_bytes = r.input_bytes;
            }
        }
        let n = processed.fetch_add(1, Ordering::SeqCst) + 1;
        if !cfg.quiet && cfg.verbose {
            match &res {
                Ok(r) => eprintln!(
                    "[{n}/{total}] {} → {} ({:.1}% saved)",
                    path.display(),
                    r.output_path.display(),
                    r.reduction_percent()
                ),
                Err(e) => eprintln!("[{n}/{total}] {}: ERROR {e}", path.display()),
            }
        }
        if !cfg.quiet {
            if let Ok(r) = &res {
                if let Some(note) =
                    fast_override_note(cfg.video_opts.fast, r.format_in, r.format_out)
                {
                    eprintln!("  note: {note}");
                }
            }
        }
        if let Some(pb) = &progress {
            pb.set_message(display_filename(path));
            pb.inc(1);
        }
        video_pairs.push((path.clone(), res, downgraded));
    }

    // Audio sequentially.
    let mut audio_pairs: Vec<(PathBuf, Result<AudioResult, AudioError>, bool)> = Vec::new();
    for path in &audio_files {
        let original_bytes = if cfg.overwrite {
            std::fs::read(path).ok()
        } else {
            None
        };
        let mut res = squish_audio::squish_audio(path, &audio_opts);
        let mut downgraded = false;
        if let Ok(r) = &res {
            let legitimate = audio_opts.output_format.is_some()
                || audio_opts.codec.is_some()
                || audio_opts.bitrate_kbps.is_some()
                || audio_opts.target_size.is_some()
                || r.format_in != r.format_out;
            match enforce_never_grow(
                &r.input_path,
                &r.output_path,
                r.input_bytes,
                r.output_bytes,
                legitimate,
                cfg.overwrite,
                original_bytes.as_deref(),
            ) {
                Ok(NeverGrow::Smaller) => {}
                Ok(NeverGrow::GrewButAllowed) => {
                    if !cfg.quiet && cfg.verbose {
                        eprintln!(
                            "  note: output grew ({} → {} bytes) — conversion requested",
                            r.input_bytes, r.output_bytes
                        );
                    }
                }
                Ok(NeverGrow::Skipped) => downgraded = true,
                Err(e) => res = Err(e.into()),
            }
        }
        if downgraded {
            if let Ok(r) = &mut res {
                r.output_bytes = r.input_bytes;
            }
        }
        let n = processed.fetch_add(1, Ordering::SeqCst) + 1;
        if !cfg.quiet && cfg.verbose {
            match &res {
                Ok(r) => eprintln!(
                    "[{n}/{total}] {} → {} ({:.1}% saved)",
                    path.display(),
                    r.output_path.display(),
                    r.reduction_percent()
                ),
                Err(e) => eprintln!("[{n}/{total}] {}: ERROR {e}", path.display()),
            }
        }
        if let Some(pb) = &progress {
            pb.set_message(display_filename(path));
            pb.inc(1);
        }
        audio_pairs.push((path.clone(), res, downgraded));
    }

    // Code in parallel.
    let code_pairs: Vec<(PathBuf, Result<CodeResult, CodeError>, bool)> = code_files
        .par_iter()
        .map(|path| {
            let original_bytes = if cfg.overwrite {
                std::fs::read(path).ok()
            } else {
                None
            };
            let mut res = squish_code::squish_code(path, &cfg.code_opts);
            let mut downgraded = false;
            if let Ok(r) = &res {
                // Code never has a legitimate size-changing "conversion" —
                // minification only ever changes formatting, never format.
                match enforce_never_grow(
                    &r.input_path,
                    &r.output_path,
                    r.input_bytes,
                    r.output_bytes,
                    false,
                    cfg.overwrite,
                    original_bytes.as_deref(),
                ) {
                    Ok(NeverGrow::Smaller) => {}
                    Ok(NeverGrow::GrewButAllowed) => unreachable!(
                        "code never reports a legitimate transform, so growth is never allowed"
                    ),
                    Ok(NeverGrow::Skipped) => downgraded = true,
                    Err(e) => res = Err(e.into()),
                }
            }
            if downgraded {
                if let Ok(r) = &mut res {
                    r.output_bytes = r.input_bytes;
                }
            }
            let n = processed.fetch_add(1, Ordering::SeqCst) + 1;
            if !cfg.quiet && cfg.verbose {
                match &res {
                    Ok(r) => eprintln!(
                        "[{n}/{total}] {} → {} ({:.1}% saved)",
                        path.display(),
                        r.output_path.display(),
                        r.reduction_percent()
                    ),
                    Err(e) => eprintln!("[{n}/{total}] {}: ERROR {e}", path.display()),
                }
            }
            if let Some(pb) = &progress {
                pb.set_message(display_filename(path));
                pb.inc(1);
            }
            (path.clone(), res, downgraded)
        })
        .collect();

    if let Some(pb) = progress {
        pb.finish_and_clear();
    }

    let mut results = Vec::new();
    let mut video_results = Vec::new();
    let mut audio_results = Vec::new();
    let mut code_results = Vec::new();
    let mut already_optimal_images = Vec::new();
    let mut already_optimal_video = Vec::new();
    let mut already_optimal_audio = Vec::new();
    let mut already_optimal_code = Vec::new();
    let mut errors: Vec<(PathBuf, String)> = Vec::new();

    for (p, r, downgraded) in image_pairs {
        match r {
            Ok(r) if downgraded => already_optimal_images.push(r),
            Ok(r) => results.push(r),
            Err(e) => errors.push((p, format!("{e}"))),
        }
    }
    for (p, r, downgraded) in video_pairs {
        match r {
            Ok(r) if downgraded => already_optimal_video.push(r),
            Ok(r) => video_results.push(r),
            Err(e) => errors.push((p, format!("{e}"))),
        }
    }
    for (p, r, downgraded) in audio_pairs {
        match r {
            Ok(r) if downgraded => already_optimal_audio.push(r),
            Ok(r) => audio_results.push(r),
            Err(e) => errors.push((p, format!("{e}"))),
        }
    }
    for (p, r, downgraded) in code_pairs {
        match r {
            Ok(r) if downgraded => already_optimal_code.push(r),
            Ok(r) => code_results.push(r),
            Err(e) => errors.push((p, format!("{e}"))),
        }
    }

    let report = RunReport {
        results,
        video_results,
        audio_results,
        code_results,
        errors,
        skipped_unknown,
        already_optimal_images,
        already_optimal_video,
        already_optimal_audio,
        already_optimal_code,
        total_wall: start.elapsed(),
    };

    if cfg.json {
        crate::json_report::print(&crate::json_report::build(&report));
    } else if !cfg.quiet {
        print_summary(&report);
    }

    Ok(report)
}

fn build_progress_bar(total: u64, cfg: &RunConfig) -> Option<ProgressBar> {
    if cfg.quiet || cfg.verbose || cfg.json || total == 0 {
        return None;
    }
    let pb = ProgressBar::with_draw_target(Some(total), ProgressDrawTarget::stderr());
    // `with_draw_target` uses stderr; when it's not a TTY indicatif defaults to
    // a hidden draw target, so no extra TTY check is needed here.
    let style =
        ProgressStyle::with_template("{spinner} [{bar:30.cyan/blue}] {pos}/{len} {wide_msg:.dim}")
            .unwrap()
            .progress_chars("=> ");
    pb.set_style(style);
    pb.enable_steady_tick(Duration::from_millis(100));
    Some(pb)
}

fn display_filename(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

/// A user-facing note when `--fast` was silently overridden because the input
/// format is transcode-only (output container differs from the input).
fn fast_override_note(
    fast: bool,
    format_in: VideoFormat,
    format_out: VideoFormat,
) -> Option<String> {
    if fast && format_in != format_out {
        Some(format!(
            "--fast ignored for .{} input; re-encoded to .{}",
            format_in.extension(),
            format_out.extension()
        ))
    } else {
        None
    }
}

/// `--target-size` only makes sense for media: minified code has no quality
/// dial. Reject a batch that is code-only; in mixed batches the code files
/// simply minify as usual.
fn validate_target_size_applicable(
    target_requested: bool,
    has_media: bool,
    has_code: bool,
) -> Result<(), String> {
    if target_requested && !has_media && has_code {
        return Err(
            "--target-size does not apply to code files (only images, video, audio)".into(),
        );
    }
    Ok(())
}

/// After per-input classification, ensure every kind that was requested via
/// `--format` matches at least one input. `--format webm` parses as both
/// video and audio, so the intersection of `requested` and `present` must be
/// non-empty (not "all requested kinds must be present").
fn validate_format_kinds_present(
    img_requested: bool,
    vid_requested: bool,
    aud_requested: bool,
    img_present: bool,
    vid_present: bool,
    aud_present: bool,
) -> Result<(), String> {
    let any_requested = img_requested || vid_requested || aud_requested;
    if !any_requested {
        return Ok(());
    }
    let any_matches = (img_requested && img_present)
        || (vid_requested && vid_present)
        || (aud_requested && aud_present);
    if any_matches {
        return Ok(());
    }
    let requested_kinds: Vec<&str> = [
        ("image", img_requested),
        ("video", vid_requested),
        ("audio", aud_requested),
    ]
    .iter()
    .filter(|(_, r)| *r)
    .map(|(n, _)| *n)
    .collect();
    Err(format!(
        "--format specifies a {} format, but no {} files were provided",
        requested_kinds.join("/"),
        requested_kinds.join("/")
    ))
}

/// Dry-run line for a planned file action, reflecting overwrite mode.
fn dry_run_action(overwrite: bool, kind: &str, path: &std::path::Path) -> String {
    if overwrite {
        format!("would overwrite in place ({kind}): {}", path.display())
    } else {
        format!("would squish ({kind}): {}", path.display())
    }
}

fn peek_image_format(path: &Path) -> std::io::Result<Option<Format>> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut head = [0u8; 32];
    let n = f.read(&mut head)?;
    Ok(squish_core::detect_format(path, &head[..n]))
}

fn print_summary(r: &RunReport) {
    let in_mb = r.input_bytes() as f64 / 1_048_576.0;
    let out_mb = r.output_bytes() as f64 / 1_048_576.0;
    let saved = if r.input_bytes() > 0 {
        (1.0 - r.output_bytes() as f64 / r.input_bytes() as f64) * 100.0
    } else {
        0.0
    };

    let count_detail = format_count_detail(
        r.results.len(),
        r.video_results.len(),
        r.audio_results.len(),
        r.code_results.len(),
    );

    println!(
        "Squished {count_detail} · {:.1} MB → {:.1} MB ({:+.1}%) · {}",
        in_mb,
        out_mb,
        -saved,
        humantime::format_duration(trim_sub_ms(r.total_wall))
    );
    if !r.skipped_unknown.is_empty() {
        let names: Vec<String> = r
            .skipped_unknown
            .iter()
            .take(5)
            .map(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?")
                    .to_string()
            })
            .collect();
        let extra = r.skipped_unknown.len().saturating_sub(5);
        let list = if extra > 0 {
            format!("{}, and {extra} more", names.join(", "))
        } else {
            names.join(", ")
        };
        println!("Skipped {} (unrecognized: {list})", r.skipped_unknown.len());
    }
    let already_optimal = r.total_already_optimal();
    if already_optimal > 0 {
        let paths: Vec<&PathBuf> = r
            .already_optimal_images
            .iter()
            .map(|x| &x.input_path)
            .chain(r.already_optimal_video.iter().map(|x| &x.input_path))
            .chain(r.already_optimal_audio.iter().map(|x| &x.input_path))
            .chain(r.already_optimal_code.iter().map(|x| &x.input_path))
            .collect();
        let names: Vec<String> = paths
            .iter()
            .take(5)
            .map(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?")
                    .to_string()
            })
            .collect();
        let extra = paths.len().saturating_sub(5);
        let list = if extra > 0 {
            format!("{}, and {extra} more", names.join(", "))
        } else {
            names.join(", ")
        };
        println!("Skipped {already_optimal} (already optimal: {list})");
    }
    if !r.errors.is_empty() {
        eprintln!("\nErrors ({}):", r.errors.len());
        for (p, e) in &r.errors {
            eprintln!("  {}: {e}", p.display());
        }
    }
}

fn trim_sub_ms(d: Duration) -> Duration {
    Duration::from_millis(d.as_millis() as u64)
}

fn format_count_detail(images: usize, videos: usize, audio: usize, code: usize) -> String {
    let total = images + videos + audio + code;
    let mut breakdown: Vec<String> = Vec::new();
    if images > 0 {
        breakdown.push(format!("{images} images"));
    }
    if videos > 0 {
        breakdown.push(format!("{videos} videos"));
    }
    if audio > 0 {
        breakdown.push(format!("{audio} audio"));
    }
    if code > 0 {
        breakdown.push(format!("{code} code"));
    }
    if breakdown.len() <= 1 {
        format!("{total} files")
    } else {
        format!("{total} files ({})", breakdown.join(", "))
    }
}

#[cfg(test)]
mod codec_validation_tests {
    use super::*;

    #[test]
    fn none_codec_passes() {
        let (v, a) = validate_codec_string(None, true, true).unwrap();
        assert!(v.is_none() && a.is_none());
    }

    #[test]
    fn video_codec_with_video_files() {
        let (v, a) = validate_codec_string(Some("h265"), true, false).unwrap();
        assert_eq!(v, Some(VideoCodec::H265));
        assert!(a.is_none());
    }

    #[test]
    fn audio_codec_with_audio_files() {
        let (v, a) = validate_codec_string(Some("opus"), false, true).unwrap();
        assert!(v.is_none());
        assert_eq!(a, Some(AudioCodec::Opus));
    }

    #[test]
    fn video_codec_with_audio_only_batch_errors() {
        let err = validate_codec_string(Some("h265"), false, true).unwrap_err();
        assert!(format!("{err}").contains("video codec"));
    }

    #[test]
    fn audio_codec_with_video_only_batch_errors() {
        let err = validate_codec_string(Some("opus"), true, false).unwrap_err();
        assert!(format!("{err}").contains("audio codec"));
    }

    #[test]
    fn unrecognized_codec_errors() {
        let err = validate_codec_string(Some("nope"), true, true).unwrap_err();
        assert!(format!("{err}").contains("not a recognized"));
    }

    #[test]
    fn no_audio_files_returns_none() {
        assert!(choose_lossless_codec(&[], false).is_none());
    }

    #[test]
    fn explicit_codec_skips_prompt() {
        let files = vec![PathBuf::from("song.flac")];
        assert!(choose_lossless_codec(&files, true).is_none());
    }

    #[test]
    fn no_lossless_inputs_returns_none() {
        let files = vec![PathBuf::from("song.mp3")];
        assert!(choose_lossless_codec(&files, false).is_none());
    }
}

#[cfg(test)]
mod summary_tests {
    use super::format_count_detail;

    #[test]
    fn images_only() {
        assert_eq!(format_count_detail(3, 0, 0, 0), "3 files");
    }

    #[test]
    fn videos_only() {
        assert_eq!(format_count_detail(0, 2, 0, 0), "2 files");
    }

    #[test]
    fn audio_only() {
        assert_eq!(format_count_detail(0, 0, 5, 0), "5 files");
    }

    #[test]
    fn code_only() {
        assert_eq!(format_count_detail(0, 0, 0, 4), "4 files");
    }

    #[test]
    fn images_and_videos() {
        assert_eq!(
            format_count_detail(3, 2, 0, 0),
            "5 files (3 images, 2 videos)"
        );
    }

    #[test]
    fn all_three_legacy_kinds() {
        assert_eq!(
            format_count_detail(3, 2, 5, 0),
            "10 files (3 images, 2 videos, 5 audio)"
        );
    }

    #[test]
    fn all_four_kinds() {
        assert_eq!(
            format_count_detail(3, 2, 5, 4),
            "14 files (3 images, 2 videos, 5 audio, 4 code)"
        );
    }

    #[test]
    fn code_with_images() {
        assert_eq!(
            format_count_detail(3, 0, 0, 4),
            "7 files (3 images, 4 code)"
        );
    }

    #[test]
    fn empty() {
        assert_eq!(format_count_detail(0, 0, 0, 0), "0 files");
    }
}

#[cfg(test)]
mod source_map_validation_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn source_map_off_passes() {
        assert!(validate_source_map(false, &[]).is_ok());
    }

    #[test]
    fn source_map_on_with_js_passes() {
        let files = vec![PathBuf::from("a.js")];
        assert!(validate_source_map(true, &files).is_ok());
    }

    #[test]
    fn source_map_on_with_css_passes() {
        let files = vec![PathBuf::from("style.css")];
        assert!(validate_source_map(true, &files).is_ok());
    }

    #[test]
    fn source_map_on_with_only_html_errors() {
        let files = vec![PathBuf::from("page.html")];
        let err = validate_source_map(true, &files).unwrap_err();
        assert!(format!("{err}").contains("source-map"));
    }

    #[test]
    fn source_map_on_with_only_json_errors() {
        let files = vec![PathBuf::from("data.json")];
        let err = validate_source_map(true, &files).unwrap_err();
        assert!(format!("{err}").contains("source-map"));
    }

    #[test]
    fn source_map_on_with_mixed_html_and_js_passes() {
        let files = vec![PathBuf::from("page.html"), PathBuf::from("a.js")];
        assert!(validate_source_map(true, &files).is_ok());
    }
}

#[cfg(test)]
mod dry_run_tests {
    use super::*;

    #[test]
    fn dry_run_action_reflects_overwrite() {
        use std::path::Path;
        let p = Path::new("/dir/a.mp4");
        assert_eq!(
            dry_run_action(false, "video", p),
            "would squish (video): /dir/a.mp4"
        );
        assert_eq!(
            dry_run_action(true, "video", p),
            "would overwrite in place (video): /dir/a.mp4"
        );
    }
}

#[cfg(test)]
mod fast_override_tests {
    use super::*;

    #[test]
    fn fast_override_note_only_when_remapped_and_fast() {
        use squish_video::VideoFormat;
        // fast + DV→MP4 remap → note present
        let note = fast_override_note(true, VideoFormat::Dv, VideoFormat::Mp4);
        assert!(note.unwrap().contains("dv"));
        // not fast → no note
        assert!(fast_override_note(false, VideoFormat::Dv, VideoFormat::Mp4).is_none());
        // no remap → no note
        assert!(fast_override_note(true, VideoFormat::Mp4, VideoFormat::Mp4).is_none());
    }
}

#[cfg(test)]
mod never_grow_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn smaller_output_is_kept() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("in.bin");
        let output = tmp.path().join("out.bin");
        std::fs::write(&input, b"aaaaaaaaaa").unwrap();
        std::fs::write(&output, b"aaa").unwrap();

        let outcome = enforce_never_grow(&input, &output, 10, 3, false, false, None).unwrap();
        assert!(matches!(outcome, NeverGrow::Smaller));
        // Untouched — the encoder's own (smaller) output stands.
        assert_eq!(std::fs::read(&output).unwrap(), b"aaa");
    }

    #[test]
    fn legitimate_transform_allows_growth() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("in.bin");
        let output = tmp.path().join("out.bin");
        std::fs::write(&input, b"aaa").unwrap();
        std::fs::write(&output, b"aaaaaaaaaa").unwrap();

        let outcome = enforce_never_grow(&input, &output, 3, 10, true, false, None).unwrap();
        assert!(matches!(outcome, NeverGrow::GrewButAllowed));
        // Untouched — growth was allowed, nothing to restore.
        assert_eq!(std::fs::read(&output).unwrap(), b"aaaaaaaaaa");
    }

    #[test]
    fn non_overwrite_growth_restores_input_by_copy() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("in.bin");
        let output = tmp.path().join("out.bin");
        std::fs::write(&input, b"aaa").unwrap();
        std::fs::write(&output, b"aaaaaaaaaa").unwrap(); // encoder's output "grew"

        let outcome = enforce_never_grow(&input, &output, 3, 10, false, false, None).unwrap();
        assert!(matches!(outcome, NeverGrow::Skipped));
        assert_eq!(std::fs::read(&output).unwrap(), b"aaa");
        // The input itself must be untouched.
        assert_eq!(std::fs::read(&input).unwrap(), b"aaa");
    }

    #[test]
    fn overwrite_growth_restores_from_cached_original_bytes() {
        let tmp = TempDir::new().unwrap();
        // In --overwrite mode output_path == input_path, and by the time
        // this runs the encoder has already clobbered it in place — this
        // test simulates exactly that: the path on disk already holds the
        // (larger) encoded bytes, and only the pre-encode cache remembers
        // the real original.
        let path = tmp.path().join("in.bin");
        std::fs::write(&path, b"aaaaaaaaaa").unwrap(); // already-clobbered "grown" content
        let original = b"aaa".to_vec();

        let outcome =
            enforce_never_grow(&path, &path, 3, 10, false, true, Some(&original)).unwrap();
        assert!(matches!(outcome, NeverGrow::Skipped));
        assert_eq!(std::fs::read(&path).unwrap(), b"aaa");
    }

    #[test]
    #[should_panic(expected = "original bytes must be cached")]
    fn overwrite_without_cached_bytes_panics_rather_than_silently_losing_data() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("in.bin");
        std::fs::write(&path, b"aaaaaaaaaa").unwrap();
        let _ = enforce_never_grow(&path, &path, 3, 10, false, true, None);
    }
}

#[cfg(test)]
mod target_size_validation_tests {
    use super::*;

    #[test]
    fn no_target_size_always_passes() {
        assert!(validate_target_size_applicable(false, false, true).is_ok());
        assert!(validate_target_size_applicable(false, true, false).is_ok());
    }

    #[test]
    fn target_size_with_media_passes() {
        assert!(validate_target_size_applicable(true, true, false).is_ok());
        assert!(validate_target_size_applicable(true, true, true).is_ok());
    }

    #[test]
    fn target_size_code_only_errors() {
        let msg = validate_target_size_applicable(true, false, true).unwrap_err();
        assert!(msg.contains("code"), "msg should mention code: {msg}");
    }

    #[test]
    fn target_size_empty_batch_passes() {
        // Nothing matched at all — the empty-batch handling elsewhere applies.
        assert!(validate_target_size_applicable(true, false, false).is_ok());
    }
}

#[cfg(test)]
mod kind_filter_tests {
    use super::*;

    #[test]
    fn default_filter_allows_everything() {
        let f = KindFilter::default();
        assert!(f.image && f.video && f.audio && f.code);
    }

    #[test]
    fn parse_media_only() {
        let f = parse_kinds("image,video,audio").unwrap();
        assert!(f.image && f.video && f.audio);
        assert!(!f.code);
    }

    #[test]
    fn parse_single_kind_with_spaces() {
        let f = parse_kinds(" image , code ").unwrap();
        assert!(f.image && f.code);
        assert!(!f.video && !f.audio);
    }

    #[test]
    fn parse_unknown_kind_errors() {
        let err = parse_kinds("image,imagery").unwrap_err();
        assert!(err.contains("unknown kind"));
        assert!(err.contains("imagery"));
    }

    #[test]
    fn parse_empty_part_errors() {
        assert!(parse_kinds("image,,video").is_err());
        assert!(parse_kinds("").is_err());
    }

    #[test]
    fn allows_matches_kinds_and_passes_unknown_through() {
        let f = parse_kinds("image").unwrap();
        assert!(f.allows(&FileKind::Image));
        assert!(!f.allows(&FileKind::Video));
        assert!(!f.allows(&FileKind::Audio));
        assert!(!f.allows(&FileKind::Code));
        // Unknown files stay visible so the "Skipped (unrecognized)" report
        // keeps working regardless of the filter.
        assert!(f.allows(&FileKind::Unknown));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_kind_format_check_passes_when_kind_present() {
        // image format requested, image files present → OK.
        let r = validate_format_kinds_present(
            true, false, false, // requested: image-yes, video-no, audio-no
            true, false, false, // present: image-yes
        );
        assert!(r.is_ok());
    }

    #[test]
    fn cross_kind_format_check_fails_when_kind_missing() {
        // video format requested but worklist has only images → error.
        let r = validate_format_kinds_present(
            false, true, false, // requested: video only
            true, false, false, // present: image only
        );
        let msg = r.unwrap_err();
        assert!(msg.contains("video"), "msg should mention video: {msg}");
    }

    #[test]
    fn cross_kind_format_check_passes_when_at_least_one_matches() {
        // webm: both video AND audio requested; only video files present → OK.
        let r = validate_format_kinds_present(
            false, true, true, // requested: video + audio (webm)
            false, true, false, // present: video only
        );
        assert!(r.is_ok());
    }

    #[test]
    fn cross_kind_format_check_passes_when_nothing_requested() {
        // No --format → no validation to do.
        let r = validate_format_kinds_present(false, false, false, true, false, false);
        assert!(r.is_ok());
    }
}
