//! Audio compression library for squish (ffmpeg-backed).

pub mod ffmpeg;
pub mod format;
pub mod options;
pub mod result;

pub use squish_media::MediaError as AudioError;
pub use ffmpeg::ProbeKind;
pub use format::{detect_audio_format, detect_audio_from_bytes, AudioFormat};
pub use options::{AudioCodec, AudioOptions};
pub use result::AudioResult;

use squish_core::{derive_output_path_with_suffix, in_place_target, in_place_temp_path};
use std::path::Path;
use std::time::Instant;

/// Compress a single audio file. Shells out to system ffmpeg + ffprobe.
///
/// On error, any partial output file is cleaned up.
pub fn squish_audio(input: &Path, opts: &AudioOptions) -> Result<AudioResult, AudioError> {
    squish_media::check_ffmpeg()?;
    ffmpeg::check_ffprobe()?;

    let start = Instant::now();
    let input_bytes = std::fs::metadata(input)?.len();

    let format_in = detect_audio_format(input).ok_or_else(|| AudioError::UnsupportedFormat {
        path: input.to_path_buf(),
        reason: "could not identify audio format from extension or magic bytes".into(),
    })?;

    // For ambiguous containers, refuse if it's actually a video.
    if format_in.is_ambiguous() {
        if let Ok(ProbeKind::HasVideo) = ffmpeg::ffprobe_kind(input) {
            return Err(AudioError::NotAudio {
                path: input.to_path_buf(),
            });
        }
    }

    // Resolve output codec.
    let input_codec = ffmpeg::ffprobe_audio_codec(input)?;
    let codec = resolve_output_codec(opts, input_codec);

    // Reject incompatible options.
    if codec.is_lossless() && opts.bitrate_kbps.is_some() {
        return Err(AudioError::InvalidOption {
            reason: format!("--bitrate is not valid for lossless codec {codec:?}"),
        });
    }

    // Resolve output extension.
    let input_ext = input
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let output_ext = resolve_output_extension(input_codec, codec, &input_ext);
    let format_out = AudioFormat::parse(&output_ext).unwrap_or(format_in);

    let (encode_path, rename_to) = if opts.overwrite {
        match in_place_target(input, &output_ext) {
            Some(target) => {
                let tmp = in_place_temp_path(&target);
                (tmp, Some(target))
            }
            None => {
                return Err(AudioError::InPlaceFormatChange {
                    path: input.to_path_buf(),
                    from: input_ext.clone(),
                    to: output_ext.clone(),
                });
            }
        }
    } else {
        let suffix = opts.suffix.as_deref().unwrap_or("squished");
        (
            derive_output_path_with_suffix(input, &output_ext, opts.force_overwrite, suffix),
            None,
        )
    };

    let has_art = ffmpeg::ffprobe_has_attached_picture(input).unwrap_or(false);

    ffmpeg::run_ffmpeg(input, &encode_path, opts, codec, has_art)?;

    let output_path = match rename_to {
        Some(target) => {
            std::fs::rename(&encode_path, &target)?;
            target
        }
        None => encode_path,
    };

    let output_bytes = std::fs::metadata(&output_path)?.len();

    Ok(AudioResult {
        input_path: input.to_path_buf(),
        output_path,
        input_bytes,
        output_bytes,
        format_in,
        format_out,
        codec_used: codec,
        duration: start.elapsed(),
    })
}

/// Pure: choose output codec given options + (probed) input codec.
pub fn resolve_output_codec(opts: &AudioOptions, input_codec: Option<AudioCodec>) -> AudioCodec {
    if let Some(c) = opts.codec {
        return c;
    }
    match input_codec {
        Some(c) if !c.is_lossless() => c,
        Some(_) | None => AudioCodec::Opus,
    }
}

/// Pure: choose output extension given input/output codec and input extension.
pub fn resolve_output_extension(
    input_codec: Option<AudioCodec>,
    output_codec: AudioCodec,
    input_ext: &str,
) -> String {
    if output_codec == AudioCodec::Copy {
        return input_ext.to_string();
    }
    if Some(output_codec) == input_codec {
        // Same codec: preserve input extension exactly.
        if !input_ext.is_empty() {
            return input_ext.to_string();
        }
    }
    // Special: Opus output may live in a `.ogg` container if input was `.ogg`.
    if output_codec == AudioCodec::Opus && input_ext == "ogg" {
        return "ogg".into();
    }
    output_codec.canonical_extension().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn unknown_format_returns_unsupported() {
        let tmp = TempDir::new().unwrap();
        let input = tmp.path().join("mystery.xyz");
        fs::write(&input, b"random bytes").unwrap();

        let err = squish_audio(&input, &AudioOptions::default()).unwrap_err();
        match err {
            AudioError::UnsupportedFormat { reason, .. } => {
                assert!(reason.contains("could not identify audio format"));
            }
            AudioError::MissingDependency { .. } => {}
            other => panic!("wrong error variant: {other:?}"),
        }
    }

    #[test]
    fn missing_file_returns_io_error() {
        let err =
            squish_audio(Path::new("/nonexistent/song.mp3"), &AudioOptions::default()).unwrap_err();
        assert!(matches!(
            err,
            AudioError::Io(_) | AudioError::MissingDependency { .. }
        ));
    }

    #[test]
    fn resolve_codec_uses_explicit_override() {
        let opts = AudioOptions {
            codec: Some(AudioCodec::Mp3),
            ..Default::default()
        };
        assert_eq!(
            resolve_output_codec(&opts, Some(AudioCodec::Flac)),
            AudioCodec::Mp3
        );
    }

    #[test]
    fn resolve_codec_uses_input_for_lossy() {
        let opts = AudioOptions::default();
        assert_eq!(
            resolve_output_codec(&opts, Some(AudioCodec::Mp3)),
            AudioCodec::Mp3
        );
    }

    #[test]
    fn resolve_codec_defaults_opus_for_lossless() {
        let opts = AudioOptions::default();
        assert_eq!(
            resolve_output_codec(&opts, Some(AudioCodec::Flac)),
            AudioCodec::Opus
        );
    }

    #[test]
    fn resolve_ext_same_codec_preserves_input_ext() {
        assert_eq!(
            resolve_output_extension(Some(AudioCodec::Mp3), AudioCodec::Mp3, "mp3"),
            "mp3"
        );
        assert_eq!(
            resolve_output_extension(Some(AudioCodec::Opus), AudioCodec::Opus, "opus"),
            "opus"
        );
    }

    #[test]
    fn resolve_ext_codec_change_uses_canonical() {
        assert_eq!(
            resolve_output_extension(Some(AudioCodec::Flac), AudioCodec::Mp3, "flac"),
            "mp3"
        );
        assert_eq!(
            resolve_output_extension(Some(AudioCodec::Flac), AudioCodec::Aac, "flac"),
            "m4a"
        );
    }

    #[test]
    fn resolve_ext_opus_ogg_special_case() {
        assert_eq!(
            resolve_output_extension(Some(AudioCodec::Vorbis), AudioCodec::Opus, "ogg"),
            "ogg"
        );
    }

    #[test]
    fn resolve_ext_copy_preserves_input_ext() {
        assert_eq!(
            resolve_output_extension(Some(AudioCodec::Mp3), AudioCodec::Copy, "mp3"),
            "mp3"
        );
        assert_eq!(
            resolve_output_extension(None, AudioCodec::Copy, "wav"),
            "wav"
        );
    }

    #[test]
    fn resolve_ext_falls_back_to_canonical_when_empty_input_ext() {
        // Same-codec match but empty input_ext: must fall through to canonical.
        assert_eq!(
            resolve_output_extension(Some(AudioCodec::Opus), AudioCodec::Opus, ""),
            "opus"
        );
    }

    #[test]
    fn resolve_ext_uses_canonical_when_input_codec_unknown() {
        // input_codec=None with explicit output codec: same-codec branch can't
        // match, so we get the canonical extension.
        assert_eq!(
            resolve_output_extension(None, AudioCodec::Opus, "wav"),
            "opus"
        );
    }
}
