use anyhow::Result;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use rayon::prelude::*;
use squish_core::{squish_file, Format, SquishError, SquishOptions, SquishResult};
use squish_video::{self, VideoOptions, VideoResult, VideoError};
use squish_video::VideoCodec;
use squish_audio::{self, AudioOptions, AudioResult};
use squish_audio::AudioCodec;
#[allow(unused_imports)]
use squish_audio::AudioError;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub struct RunConfig {
    pub opts: SquishOptions,
    pub video_opts: VideoOptions,
    #[allow(dead_code)]
    pub audio_opts: AudioOptions,
    pub verbose: bool,
    pub quiet: bool,
    pub dry_run: bool,
}

pub struct RunReport {
    pub results: Vec<SquishResult>,
    pub video_results: Vec<VideoResult>,
    pub audio_results: Vec<AudioResult>,
    pub errors: Vec<(PathBuf, String)>,
    pub skipped_unknown: Vec<PathBuf>,
    pub total_wall: Duration,
}

impl RunReport {
    pub fn input_bytes(&self) -> u64 {
        let img: u64 = self.results.iter().map(|r| r.input_bytes).sum();
        let vid: u64 = self.video_results.iter().map(|r| r.input_bytes).sum();
        let aud: u64 = self.audio_results.iter().map(|r| r.input_bytes).sum();
        img + vid + aud
    }
    pub fn output_bytes(&self) -> u64 {
        let img: u64 = self.results.iter().map(|r| r.output_bytes).sum();
        let vid: u64 = self.video_results.iter().map(|r| r.output_bytes).sum();
        let aud: u64 = self.audio_results.iter().map(|r| r.output_bytes).sum();
        img + vid + aud
    }
    #[allow(dead_code)]
    pub fn total_files(&self) -> usize {
        self.results.len() + self.video_results.len() + self.audio_results.len()
    }
    pub fn exit_code(&self) -> u8 {
        if self.errors.is_empty() { 0 } else { 1 }
    }
}

enum FileKind {
    Image,
    Video,
    Audio,
    Unknown,
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
    FileKind::Unknown
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum CodecAppliesTo {
    Video,
    Audio,
    Neither,
}

#[allow(dead_code)]
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
#[allow(dead_code)]
pub fn validate_codec_string(
    codec: Option<&str>,
    has_video: bool,
    has_audio: bool,
) -> anyhow::Result<(Option<VideoCodec>, Option<AudioCodec>)> {
    let Some(s) = codec else { return Ok((None, None)); };
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

#[allow(dead_code)]
fn is_lossless_input(path: &Path) -> bool {
    matches!(
        squish_audio::detect_audio_format(path),
        Some(squish_audio::AudioFormat::Flac) | Some(squish_audio::AudioFormat::Wav) | Some(squish_audio::AudioFormat::Aiff)
    )
}

#[allow(dead_code)]
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
#[allow(dead_code)]
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

pub fn run(paths: &[PathBuf], cfg: &RunConfig) -> Result<RunReport> {
    let start = Instant::now();

    let mut image_files = Vec::new();
    let mut video_files = Vec::new();
    let mut skipped_unknown = Vec::new();

    for path in paths {
        match classify_file(path) {
            FileKind::Image => image_files.push(path.clone()),
            FileKind::Video => video_files.push(path.clone()),
            FileKind::Audio => skipped_unknown.push(path.clone()),
            FileKind::Unknown => skipped_unknown.push(path.clone()),
        }
    }

    if cfg.dry_run {
        for p in &image_files {
            println!("would squish (image): {}", p.display());
        }
        for p in &video_files {
            println!("would squish (video): {}", p.display());
        }
        for p in &skipped_unknown {
            println!("would skip (unrecognized): {}", p.display());
        }
        return Ok(RunReport {
            results: Vec::new(),
            video_results: Vec::new(),
            audio_results: Vec::new(),
            errors: Vec::new(),
            skipped_unknown,
            total_wall: start.elapsed(),
        });
    }

    let total = (image_files.len() + video_files.len()) as u64;
    let processed = AtomicU64::new(0);
    let progress = build_progress_bar(total, cfg);

    // Process images in parallel
    let image_pairs: Vec<(PathBuf, Result<SquishResult, SquishError>)> = image_files
        .par_iter()
        .map(|path| {
            let res = squish_file(path, &cfg.opts);
            let n = processed.fetch_add(1, Ordering::SeqCst) + 1;
            if !cfg.quiet && cfg.verbose {
                match &res {
                    Ok(r) => eprintln!(
                        "[{n}/{total}] {} → {} ({:.1}% saved)",
                        path.display(), r.output_path.display(), r.reduction_percent()
                    ),
                    Err(e) => eprintln!("[{n}/{total}] {}: ERROR {e}", path.display()),
                }
            }
            if let Some(pb) = &progress {
                pb.set_message(display_filename(path));
                pb.inc(1);
            }
            (path.clone(), res)
        })
        .collect();

    // Process videos sequentially (ffmpeg uses multiple cores internally)
    let mut video_pairs: Vec<(PathBuf, Result<VideoResult, VideoError>)> = Vec::new();
    for path in &video_files {
        let res = squish_video::squish_video(path, &cfg.video_opts);
        let n = processed.fetch_add(1, Ordering::SeqCst) + 1;
        if !cfg.quiet && cfg.verbose {
            match &res {
                Ok(r) => eprintln!(
                    "[{n}/{total}] {} → {} ({:.1}% saved)",
                    path.display(), r.output_path.display(), r.reduction_percent()
                ),
                Err(e) => eprintln!("[{n}/{total}] {}: ERROR {e}", path.display()),
            }
        }
        if let Some(pb) = &progress {
            pb.set_message(display_filename(path));
            pb.inc(1);
        }
        video_pairs.push((path.clone(), res));
    }

    if let Some(pb) = progress {
        pb.finish_and_clear();
    }

    let mut results = Vec::new();
    let mut video_results = Vec::new();
    let mut errors: Vec<(PathBuf, String)> = Vec::new();

    for (p, r) in image_pairs {
        match r {
            Ok(r) => results.push(r),
            Err(e) => errors.push((p, format!("{e}"))),
        }
    }
    for (p, r) in video_pairs {
        match r {
            Ok(r) => video_results.push(r),
            Err(e) => errors.push((p, format!("{e}"))),
        }
    }

    let report = RunReport {
        results,
        video_results,
        audio_results: Vec::new(),
        errors,
        skipped_unknown,
        total_wall: start.elapsed(),
    };

    if !cfg.quiet {
        print_summary(&report);
    }

    Ok(report)
}

fn build_progress_bar(total: u64, cfg: &RunConfig) -> Option<ProgressBar> {
    if cfg.quiet || cfg.verbose || total == 0 {
        return None;
    }
    let pb = ProgressBar::with_draw_target(Some(total), ProgressDrawTarget::stderr());
    // `with_draw_target` uses stderr; when it's not a TTY indicatif defaults to
    // a hidden draw target, so no extra TTY check is needed here.
    let style = ProgressStyle::with_template(
        "{spinner} [{bar:30.cyan/blue}] {pos}/{len} {wide_msg:.dim}",
    )
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

    let count_detail = match (r.results.len(), r.video_results.len()) {
        (img, 0) => format!("{img} files"),
        (0, vid) => format!("{vid} files"),
        (img, vid) => format!("{} files ({img} images, {vid} videos)", img + vid),
    };

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
            .map(|p| p.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string())
            .collect();
        let extra = r.skipped_unknown.len().saturating_sub(5);
        let list = if extra > 0 {
            format!("{}, and {extra} more", names.join(", "))
        } else {
            names.join(", ")
        };
        println!("Skipped {} (unrecognized: {list})", r.skipped_unknown.len());
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
