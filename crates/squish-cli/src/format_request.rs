//! Resolve `--format <value>` against all three media kinds, and validate
//! audio `--format` + `--codec` compatibility.

use squish_audio::{AudioCodec, AudioFormat};
use squish_core::Format;
use squish_video::VideoFormat;

/// A `--format` value, decomposed into whichever per-kind enum(s) accepted
/// it. At least one of the three `Option`s is `Some` whenever `parse`
/// returns `Some`.
#[derive(Debug, Clone, PartialEq)]
pub struct RequestedFormat {
    pub raw: String,
    pub image: Option<Format>,
    pub video: Option<VideoFormat>,
    pub audio: Option<AudioFormat>,
}

impl RequestedFormat {
    /// Parse the raw `--format` value against all three media kinds. Returns
    /// `None` only if no kind recognised the value. `dv` is excluded from the
    /// video result because it is transcode-only input, not a valid output.
    pub fn parse(value: &str) -> Option<RequestedFormat> {
        let image = Format::parse(value);
        let video = VideoFormat::parse(value).filter(|v| !matches!(v, VideoFormat::Dv));
        let audio = AudioFormat::parse(value);
        if image.is_none() && video.is_none() && audio.is_none() {
            return None;
        }
        Some(RequestedFormat {
            raw: value.to_string(),
            image,
            video,
            audio,
        })
    }

    /// Reject audio formats the audio pipeline cannot encode today (`wav`,
    /// `aiff` — both require a PCM codec which `AudioCodec` does not yet
    /// expose). Call after a successful `parse`, before any encoding starts.
    pub fn validate_audio_output(&self) -> Result<(), String> {
        match self.audio {
            Some(AudioFormat::Wav) => Err(
                "audio --format wav requires a PCM codec which squish does not yet support"
                    .into(),
            ),
            Some(AudioFormat::Aiff) => Err(
                "audio --format aiff requires a PCM codec which squish does not yet support"
                    .into(),
            ),
            _ => Ok(()),
        }
    }
}

/// Default codec for a given audio target format, used when `--codec` is
/// unset. Returns `None` for formats with no supported default (wav/aiff —
/// caller should validate first via `validate_audio_output`).
pub fn default_codec_for_audio_format(fmt: AudioFormat) -> Option<AudioCodec> {
    match fmt {
        AudioFormat::Mp3 => Some(AudioCodec::Mp3),
        AudioFormat::M4a => Some(AudioCodec::Aac),
        AudioFormat::Opus => Some(AudioCodec::Opus),
        AudioFormat::Ogg => Some(AudioCodec::Vorbis),
        AudioFormat::Flac => Some(AudioCodec::Flac),
        AudioFormat::Webm => Some(AudioCodec::Opus),
        AudioFormat::Wav | AudioFormat::Aiff => None,
    }
}

/// Resolve the final audio codec given `--format` and the explicit `--codec`
/// (if any). Returns the codec to use, or an error if the combination is
/// incompatible. `--codec copy` is allowed for formats whose default codec is
/// lossy/lossless-matching; per-file mismatches (e.g. `--codec copy` on a
/// non-matching input) are caught later by the audio pipeline.
pub fn resolve_audio_codec(
    fmt: AudioFormat,
    explicit: Option<AudioCodec>,
) -> Result<AudioCodec, String> {
    let Some(explicit) = explicit else {
        return default_codec_for_audio_format(fmt).ok_or_else(|| {
            format!(
                "--format {} is not supported as audio output",
                fmt.extension()
            )
        });
    };
    let ok = matches!(
        (fmt, explicit),
        (AudioFormat::Mp3, AudioCodec::Mp3 | AudioCodec::Copy)
            | (AudioFormat::M4a, AudioCodec::Aac | AudioCodec::Alac)
            | (AudioFormat::Opus, AudioCodec::Opus | AudioCodec::Copy)
            | (AudioFormat::Ogg, AudioCodec::Vorbis | AudioCodec::Opus)
            | (AudioFormat::Flac, AudioCodec::Flac | AudioCodec::Copy)
            | (AudioFormat::Webm, AudioCodec::Opus)
    );
    if ok {
        Ok(explicit)
    } else {
        Err(format!(
            "--format {} is not compatible with --codec {:?}",
            fmt.extension(),
            explicit
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_image_only() {
        let r = RequestedFormat::parse("png").unwrap();
        assert_eq!(r.image, Some(Format::Png));
        assert!(r.video.is_none());
        assert!(r.audio.is_none());
    }

    #[test]
    fn parse_video_only() {
        let r = RequestedFormat::parse("mov").unwrap();
        assert_eq!(r.video, Some(VideoFormat::Mov));
        assert!(r.image.is_none());
        assert!(r.audio.is_none());
    }

    #[test]
    fn parse_audio_only_mp3_flac_opus() {
        for (val, want) in [
            ("mp3", AudioFormat::Mp3),
            ("flac", AudioFormat::Flac),
            ("opus", AudioFormat::Opus),
        ] {
            let r = RequestedFormat::parse(val).unwrap();
            assert_eq!(r.audio, Some(want), "value {val}");
            assert!(r.image.is_none(), "value {val} parsed as image");
            assert!(r.video.is_none(), "value {val} parsed as video");
        }
    }

    #[test]
    fn parse_webm_resolves_to_both_video_and_audio() {
        let r = RequestedFormat::parse("webm").unwrap();
        assert_eq!(r.video, Some(VideoFormat::Webm));
        assert_eq!(r.audio, Some(AudioFormat::Webm));
        assert!(r.image.is_none());
    }

    #[test]
    fn parse_mp4_resolves_to_video_and_m4a_audio() {
        let r = RequestedFormat::parse("mp4").unwrap();
        assert_eq!(r.video, Some(VideoFormat::Mp4));
        assert_eq!(r.audio, Some(AudioFormat::M4a));
    }

    #[test]
    fn parse_rejects_unknown() {
        assert_eq!(RequestedFormat::parse("xyz"), None);
    }

    #[test]
    fn parse_excludes_dv_from_video_result() {
        // dv is input-only — not a valid `--format` target.
        assert_eq!(RequestedFormat::parse("dv"), None);
        assert_eq!(RequestedFormat::parse("dif"), None);
    }

    #[test]
    fn validate_audio_output_rejects_wav_and_aiff() {
        let wav = RequestedFormat::parse("wav").unwrap();
        assert!(wav.validate_audio_output().is_err());
        let aiff = RequestedFormat::parse("aiff").unwrap();
        assert!(aiff.validate_audio_output().is_err());
    }

    #[test]
    fn validate_audio_output_accepts_supported_formats() {
        for v in ["mp3", "m4a", "opus", "ogg", "flac", "webm"] {
            let r = RequestedFormat::parse(v).unwrap();
            assert!(r.validate_audio_output().is_ok(), "value {v}");
        }
    }

    #[test]
    fn default_codec_table() {
        assert_eq!(default_codec_for_audio_format(AudioFormat::Mp3), Some(AudioCodec::Mp3));
        assert_eq!(default_codec_for_audio_format(AudioFormat::M4a), Some(AudioCodec::Aac));
        assert_eq!(default_codec_for_audio_format(AudioFormat::Opus), Some(AudioCodec::Opus));
        assert_eq!(default_codec_for_audio_format(AudioFormat::Ogg), Some(AudioCodec::Vorbis));
        assert_eq!(default_codec_for_audio_format(AudioFormat::Flac), Some(AudioCodec::Flac));
        assert_eq!(default_codec_for_audio_format(AudioFormat::Webm), Some(AudioCodec::Opus));
        assert_eq!(default_codec_for_audio_format(AudioFormat::Wav), None);
        assert_eq!(default_codec_for_audio_format(AudioFormat::Aiff), None);
    }

    #[test]
    fn resolve_audio_codec_implies_default_when_unset() {
        assert_eq!(resolve_audio_codec(AudioFormat::Opus, None).unwrap(), AudioCodec::Opus);
        assert_eq!(resolve_audio_codec(AudioFormat::Webm, None).unwrap(), AudioCodec::Opus);
    }

    #[test]
    fn resolve_audio_codec_accepts_compatible_explicit() {
        assert_eq!(
            resolve_audio_codec(AudioFormat::M4a, Some(AudioCodec::Alac)).unwrap(),
            AudioCodec::Alac
        );
        assert_eq!(
            resolve_audio_codec(AudioFormat::Ogg, Some(AudioCodec::Opus)).unwrap(),
            AudioCodec::Opus
        );
    }

    #[test]
    fn resolve_audio_codec_rejects_incompatible_explicit() {
        // mp3 container + opus codec is nonsensical.
        assert!(resolve_audio_codec(AudioFormat::Mp3, Some(AudioCodec::Opus)).is_err());
        // webm container only accepts Opus today.
        assert!(resolve_audio_codec(AudioFormat::Webm, Some(AudioCodec::Vorbis)).is_err());
    }
}
