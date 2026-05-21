//! ffmpeg/ffprobe binary detection, command building, and execution.

use crate::AudioError;
use crate::options::AudioCodec;
use std::path::Path;
use std::process::Command;

/// Check that ffprobe is available on PATH.
pub fn check_ffprobe() -> Result<(), AudioError> {
    match Command::new("ffprobe").arg("-version").output() {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(AudioError::MissingDependency {
            name: "ffprobe".into(),
            install_hint: "ffprobe ships with ffmpeg; brew install ffmpeg or apt install ffmpeg"
                .into(),
        }),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeKind {
    AudioOnly,
    HasVideo,
    Unknown,
}

/// Use ffprobe to detect whether `path` contains a video stream. Used to
/// disambiguate ambiguous containers (mp4/m4a/webm/mkv).
///
/// Returns `HasVideo` if at least one non-attached-picture video stream exists,
/// `AudioOnly` if only audio + (optionally) attached pictures, `Unknown` if
/// the probe yielded no usable answer.
pub fn ffprobe_kind(path: &Path) -> Result<ProbeKind, AudioError> {
    // Request `attached_pic` as its own column so we don't have to parse
    // ffprobe's bulk disposition list (which varies in shape between versions
    // and even drops entirely on some builds when the result is "no flags set").
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type:stream_disposition=attached_pic",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AudioError::MissingDependency {
                    name: "ffprobe".into(),
                    install_hint:
                        "ffprobe ships with ffmpeg; brew install ffmpeg or apt install ffmpeg"
                            .into(),
                }
            } else {
                AudioError::Io(e)
            }
        })?;

    if !output.status.success() {
        return Ok(ProbeKind::Unknown);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut has_real_video = false;
    let mut has_audio = false;

    // Each line is `<codec_type>,<attached_pic_flag>` where the flag is "0" or "1".
    for line in stdout.lines() {
        let mut parts = line.split(',');
        let kind = parts.next().unwrap_or("").trim();
        let attached_pic = parts.next().unwrap_or("").trim() == "1";
        match kind {
            "video" if !attached_pic => has_real_video = true,
            "audio" => has_audio = true,
            _ => {}
        }
    }

    Ok(match (has_real_video, has_audio) {
        (true, _) => ProbeKind::HasVideo,
        (false, true) => ProbeKind::AudioOnly,
        _ => ProbeKind::Unknown,
    })
}

/// Detect the first audio stream's codec via ffprobe. Returns `None` for
/// codecs we don't model (e.g. PCM in WAV, AC-3, etc.).
pub fn ffprobe_audio_codec(path: &Path) -> Result<Option<AudioCodec>, AudioError> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AudioError::MissingDependency {
                    name: "ffprobe".into(),
                    install_hint:
                        "ffprobe ships with ffmpeg; brew install ffmpeg or apt install ffmpeg"
                            .into(),
                }
            } else {
                AudioError::Io(e)
            }
        })?;

    if !output.status.success() {
        return Ok(None);
    }

    let codec_name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(map_codec_name(&codec_name))
}

fn map_codec_name(name: &str) -> Option<AudioCodec> {
    match name {
        "mp3" | "mp3float" => Some(AudioCodec::Mp3),
        "aac" => Some(AudioCodec::Aac),
        "opus" => Some(AudioCodec::Opus),
        "vorbis" => Some(AudioCodec::Vorbis),
        "flac" => Some(AudioCodec::Flac),
        "alac" => Some(AudioCodec::Alac),
        _ => None,
    }
}

use crate::options::{
    default_audio_quality, quality_to_aac_bitrate, quality_to_flac_level, quality_to_mp3_v,
    quality_to_opus_bitrate, quality_to_vorbis_q, AudioOptions,
};

/// Build the codec-specific portion of an ffmpeg argv. Returns the full argv
/// after `ffmpeg -y -i <input>` and before `<output>`.
pub fn build_codec_args(
    opts: &AudioOptions,
    codec: AudioCodec,
    has_attached_picture: bool,
) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();

    // Suppress non-attached-picture video streams. When art is present and
    // we want to keep it, we'll add `-c:v copy` instead.
    if has_attached_picture && !opts.strip_tags {
        args.push("-c:v".into());
        args.push("copy".into());
    } else {
        args.push("-vn".into());
    }

    // Audio codec
    args.push("-c:a".into());
    args.push(codec.ffmpeg_encoder().into());

    // Quality / bitrate args
    let quality = opts.quality.unwrap_or(default_audio_quality());
    match codec {
        AudioCodec::Mp3 => {
            if let Some(kbps) = opts.bitrate_kbps {
                args.push("-b:a".into());
                args.push(format!("{kbps}k"));
            } else {
                args.push("-q:a".into());
                args.push(quality_to_mp3_v(quality).to_string());
            }
        }
        AudioCodec::Aac => {
            let kbps = opts
                .bitrate_kbps
                .unwrap_or_else(|| quality_to_aac_bitrate(quality));
            args.push("-b:a".into());
            args.push(format!("{kbps}k"));
        }
        AudioCodec::Opus => {
            let kbps = opts
                .bitrate_kbps
                .unwrap_or_else(|| quality_to_opus_bitrate(quality));
            args.push("-b:a".into());
            args.push(format!("{kbps}k"));
            args.push("-vbr".into());
            args.push("on".into());
        }
        AudioCodec::Vorbis => {
            if let Some(kbps) = opts.bitrate_kbps {
                args.push("-b:a".into());
                args.push(format!("{kbps}k"));
            } else {
                args.push("-q:a".into());
                args.push(quality_to_vorbis_q(quality).to_string());
            }
        }
        AudioCodec::Flac => {
            args.push("-compression_level".into());
            args.push(quality_to_flac_level(quality).to_string());
        }
        AudioCodec::Alac => {
            // ALAC has no quality knob.
        }
        AudioCodec::Copy => {
            // No extra args; -c:a copy already emitted above.
        }
    }

    if opts.strip_tags {
        args.push("-map_metadata".into());
        args.push("-1".into());
    }

    args
}

/// Build and run an ffmpeg command to compress `input` to `output`.
pub fn run_ffmpeg(
    input: &Path,
    output: &Path,
    opts: &AudioOptions,
    codec: AudioCodec,
    has_attached_picture: bool,
) -> Result<(), AudioError> {
    let args: Vec<std::ffi::OsString> = build_codec_args(opts, codec, has_attached_picture)
        .into_iter()
        .map(std::ffi::OsString::from)
        .collect();
    squish_media::run_ffmpeg(input, output, &args)
}

/// Detect whether the file has an attached picture (album art) stream.
pub fn ffprobe_has_attached_picture(path: &Path) -> Result<bool, AudioError> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type:stream_disposition=attached_pic",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AudioError::MissingDependency {
                    name: "ffprobe".into(),
                    install_hint: "ffprobe ships with ffmpeg".into(),
                }
            } else {
                AudioError::Io(e)
            }
        })?;
    if !output.status.success() {
        return Ok(false);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let mut parts = line.split(',');
        let kind = parts.next().unwrap_or("").trim();
        let attached_pic = parts.next().unwrap_or("").trim() == "1";
        if kind == "video" && attached_pic {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_ffprobe_returns_ok_when_available() {
        if Command::new("ffprobe").arg("-version").output().is_ok() {
            assert!(check_ffprobe().is_ok());
        }
    }

    use std::path::PathBuf;
    use std::process::Command as ProcCommand;

    #[test]
    fn ffprobe_kind_audio_only_for_generated_wav() {
        if !ProcCommand::new("ffmpeg")
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            eprintln!("skipping: ffmpeg not present");
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("sine.wav");
        let status = ProcCommand::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=0.2",
                "-ac",
                "1",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert!(status.status.success(), "ffmpeg failed to generate fixture");

        match ffprobe_kind(&path).unwrap() {
            ProbeKind::AudioOnly => {}
            other => panic!("expected AudioOnly, got {other:?}"),
        }
    }

    #[test]
    fn ffprobe_kind_returns_unknown_for_missing_path() {
        // Probe a non-existent file; ffprobe writes to stderr and exits nonzero,
        // and we map that to ProbeKind::Unknown (not MissingDependency).
        let result = ffprobe_kind(&PathBuf::from("/definitely/does/not/exist.mp3"));
        assert!(matches!(result, Ok(ProbeKind::Unknown)));
    }

    #[test]
    fn ffprobe_kind_treats_attached_picture_as_audio_only() {
        if !ProcCommand::new("ffmpeg")
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            eprintln!("skipping: ffmpeg not present");
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let cover = tmp.path().join("cover.png");
        let audio = tmp.path().join("with_art.mp3");

        // Generate a single-frame PNG as the cover image.
        let cover_status = ProcCommand::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=64x64:d=1",
                "-frames:v",
                "1",
            ])
            .arg(&cover)
            .output()
            .unwrap();
        assert!(cover_status.status.success(), "cover generation failed");

        // Wrap a sine wave + the cover as an MP3 with attached_pic disposition.
        let mp3_status = ProcCommand::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=0.5",
                "-i",
            ])
            .arg(&cover)
            .args([
                "-map",
                "0",
                "-map",
                "1",
                "-c:v",
                "copy",
                "-c:a",
                "libmp3lame",
                "-id3v2_version",
                "3",
                "-disposition:v:0",
                "attached_pic",
            ])
            .arg(&audio)
            .output()
            .unwrap();
        assert!(
            mp3_status.status.success(),
            "mp3 mux failed: {}",
            String::from_utf8_lossy(&mp3_status.stderr)
        );

        match ffprobe_kind(&audio).unwrap() {
            ProbeKind::AudioOnly => {}
            other => panic!("expected AudioOnly for attached-picture MP3, got {other:?}"),
        }
    }

    #[test]
    fn map_codec_name_known() {
        assert_eq!(map_codec_name("mp3"), Some(AudioCodec::Mp3));
        assert_eq!(map_codec_name("aac"), Some(AudioCodec::Aac));
        assert_eq!(map_codec_name("opus"), Some(AudioCodec::Opus));
        assert_eq!(map_codec_name("vorbis"), Some(AudioCodec::Vorbis));
        assert_eq!(map_codec_name("flac"), Some(AudioCodec::Flac));
        assert_eq!(map_codec_name("alac"), Some(AudioCodec::Alac));
    }

    #[test]
    fn map_codec_name_unknown() {
        assert_eq!(map_codec_name("pcm_s16le"), None);
        assert_eq!(map_codec_name("ac3"), None);
        assert_eq!(map_codec_name(""), None);
    }

    #[test]
    fn ffprobe_audio_codec_round_trips_mp3() {
        if !ProcCommand::new("ffmpeg")
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            eprintln!("skipping: ffmpeg not present");
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("sine.mp3");
        let status = ProcCommand::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=0.2",
                "-c:a",
                "libmp3lame",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert!(status.status.success());

        let codec = ffprobe_audio_codec(&path).unwrap();
        assert_eq!(codec, Some(AudioCodec::Mp3));
    }

    use crate::options::{AudioCodec as AC, AudioOptions};

    fn args(codec: AC, opts: AudioOptions, art: bool) -> Vec<String> {
        build_codec_args(&opts, codec, art)
    }

    #[test]
    fn mp3_uses_q_a_by_default() {
        let a = args(AC::Mp3, AudioOptions::default(), false);
        assert!(a.contains(&"-vn".to_string()));
        assert!(a.contains(&"-c:a".to_string()));
        assert!(a.contains(&"libmp3lame".to_string()));
        assert!(a.contains(&"-q:a".to_string()));
    }

    #[test]
    fn mp3_bitrate_overrides_quality() {
        let opts = AudioOptions {
            bitrate_kbps: Some(192),
            ..Default::default()
        };
        let a = args(AC::Mp3, opts, false);
        assert!(a.contains(&"-b:a".to_string()));
        assert!(a.contains(&"192k".to_string()));
        assert!(!a.contains(&"-q:a".to_string()));
    }

    #[test]
    fn opus_emits_vbr_on() {
        let a = args(AC::Opus, AudioOptions::default(), false);
        assert!(a.contains(&"-vbr".to_string()));
        assert!(a.contains(&"on".to_string()));
        assert!(a.contains(&"libopus".to_string()));
    }

    #[test]
    fn flac_uses_compression_level() {
        let a = args(AC::Flac, AudioOptions::default(), false);
        assert!(a.contains(&"-compression_level".to_string()));
    }

    #[test]
    fn copy_emits_only_c_a_copy() {
        let a = args(AC::Copy, AudioOptions::default(), false);
        assert!(a.contains(&"-c:a".to_string()));
        assert!(a.contains(&"copy".to_string()));
        assert!(!a.contains(&"-q:a".to_string()));
        assert!(!a.contains(&"-b:a".to_string()));
    }

    #[test]
    fn art_preserved_when_present_and_tags_kept() {
        let a = args(AC::Mp3, AudioOptions::default(), true);
        // -c:v copy is added when art present; -vn is NOT added
        let cv_pos = a.iter().position(|s| s == "-c:v").unwrap();
        assert_eq!(a[cv_pos + 1], "copy");
        assert!(!a.contains(&"-vn".to_string()));
    }

    #[test]
    fn art_dropped_when_strip_tags() {
        let opts = AudioOptions {
            strip_tags: true,
            ..Default::default()
        };
        let a = args(AC::Mp3, opts, true);
        assert!(a.contains(&"-vn".to_string()));
        assert!(a.contains(&"-map_metadata".to_string()));
        assert!(a.contains(&"-1".to_string()));
    }

    #[test]
    fn strip_tags_emits_map_metadata_neg_1() {
        let opts = AudioOptions {
            strip_tags: true,
            ..Default::default()
        };
        let a = args(AC::Mp3, opts, false);
        let pos = a.iter().position(|s| s == "-map_metadata").unwrap();
        assert_eq!(a[pos + 1], "-1");
    }

    #[test]
    fn aac_default_uses_bitrate_ladder() {
        let a = args(AC::Aac, AudioOptions::default(), false);
        assert!(a.contains(&"-b:a".to_string()));
        assert!(a.contains(&"192k".to_string()));
        assert!(!a.contains(&"-q:a".to_string()));
    }

    #[test]
    fn opus_bitrate_overrides_ladder() {
        let opts = AudioOptions {
            bitrate_kbps: Some(256),
            ..Default::default()
        };
        let a = args(AC::Opus, opts, false);
        assert!(a.contains(&"256k".to_string()));
    }

    #[test]
    fn ffprobe_has_attached_picture_detects_album_art() {
        if !ProcCommand::new("ffmpeg")
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            eprintln!("skipping: ffmpeg not present");
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let cover = tmp.path().join("cover.png");
        let audio = tmp.path().join("with_art.mp3");

        let cover_status = ProcCommand::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=64x64:d=1",
                "-frames:v",
                "1",
            ])
            .arg(&cover)
            .output()
            .unwrap();
        assert!(cover_status.status.success());

        let mp3_status = ProcCommand::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=0.5",
                "-i",
            ])
            .arg(&cover)
            .args([
                "-map",
                "0",
                "-map",
                "1",
                "-c:v",
                "copy",
                "-c:a",
                "libmp3lame",
                "-id3v2_version",
                "3",
                "-disposition:v:0",
                "attached_pic",
            ])
            .arg(&audio)
            .output()
            .unwrap();
        assert!(mp3_status.status.success(), "mp3 mux failed");

        assert!(ffprobe_has_attached_picture(&audio).unwrap());
    }

    #[test]
    fn ffprobe_has_attached_picture_returns_false_for_plain_audio() {
        if !ProcCommand::new("ffmpeg")
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            eprintln!("skipping: ffmpeg not present");
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("plain.mp3");
        let status = ProcCommand::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=0.2",
                "-c:a",
                "libmp3lame",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert!(status.status.success());

        assert!(!ffprobe_has_attached_picture(&path).unwrap());
    }
}
