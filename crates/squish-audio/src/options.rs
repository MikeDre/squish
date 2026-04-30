#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    Mp3,
    Aac,
    Opus,
    Vorbis,
    Flac,
    Alac,
    Copy,
}

impl AudioCodec {
    pub fn ffmpeg_encoder(&self) -> &'static str {
        match self {
            AudioCodec::Mp3 => "libmp3lame",
            AudioCodec::Aac => "aac",
            AudioCodec::Opus => "libopus",
            AudioCodec::Vorbis => "libvorbis",
            AudioCodec::Flac => "flac",
            AudioCodec::Alac => "alac",
            AudioCodec::Copy => "copy",
        }
    }

    pub fn parse(s: &str) -> Option<AudioCodec> {
        match s.to_ascii_lowercase().as_str() {
            "mp3" | "libmp3lame" => Some(AudioCodec::Mp3),
            "aac" => Some(AudioCodec::Aac),
            "opus" | "libopus" => Some(AudioCodec::Opus),
            "vorbis" | "libvorbis" => Some(AudioCodec::Vorbis),
            "flac" => Some(AudioCodec::Flac),
            "alac" => Some(AudioCodec::Alac),
            "copy" => Some(AudioCodec::Copy),
            _ => None,
        }
    }

    pub fn is_lossless(&self) -> bool {
        matches!(self, AudioCodec::Flac | AudioCodec::Alac)
    }

    /// Canonical output extension for this codec when there is no input
    /// extension to preserve.
    pub fn canonical_extension(&self) -> &'static str {
        match self {
            AudioCodec::Mp3 => "mp3",
            AudioCodec::Aac => "m4a",
            AudioCodec::Opus => "opus",
            AudioCodec::Vorbis => "ogg",
            AudioCodec::Flac => "flac",
            AudioCodec::Alac => "m4a",
            AudioCodec::Copy => "mp3", // unused at runtime; Copy preserves input ext
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_aliases() {
        assert_eq!(AudioCodec::parse("mp3"), Some(AudioCodec::Mp3));
        assert_eq!(AudioCodec::parse("MP3"), Some(AudioCodec::Mp3));
        assert_eq!(AudioCodec::parse("libmp3lame"), Some(AudioCodec::Mp3));
        assert_eq!(AudioCodec::parse("aac"), Some(AudioCodec::Aac));
        assert_eq!(AudioCodec::parse("opus"), Some(AudioCodec::Opus));
        assert_eq!(AudioCodec::parse("libopus"), Some(AudioCodec::Opus));
        assert_eq!(AudioCodec::parse("vorbis"), Some(AudioCodec::Vorbis));
        assert_eq!(AudioCodec::parse("flac"), Some(AudioCodec::Flac));
        assert_eq!(AudioCodec::parse("alac"), Some(AudioCodec::Alac));
        assert_eq!(AudioCodec::parse("copy"), Some(AudioCodec::Copy));
        assert_eq!(AudioCodec::parse("h265"), None);
        assert_eq!(AudioCodec::parse(""), None);
    }

    #[test]
    fn ffmpeg_encoder_matches() {
        assert_eq!(AudioCodec::Mp3.ffmpeg_encoder(), "libmp3lame");
        assert_eq!(AudioCodec::Aac.ffmpeg_encoder(), "aac");
        assert_eq!(AudioCodec::Opus.ffmpeg_encoder(), "libopus");
        assert_eq!(AudioCodec::Vorbis.ffmpeg_encoder(), "libvorbis");
        assert_eq!(AudioCodec::Flac.ffmpeg_encoder(), "flac");
        assert_eq!(AudioCodec::Alac.ffmpeg_encoder(), "alac");
        assert_eq!(AudioCodec::Copy.ffmpeg_encoder(), "copy");
    }

    #[test]
    fn is_lossless_truth_table() {
        assert!(AudioCodec::Flac.is_lossless());
        assert!(AudioCodec::Alac.is_lossless());
        assert!(!AudioCodec::Mp3.is_lossless());
        assert!(!AudioCodec::Aac.is_lossless());
        assert!(!AudioCodec::Opus.is_lossless());
        assert!(!AudioCodec::Vorbis.is_lossless());
        assert!(!AudioCodec::Copy.is_lossless());
    }

    #[test]
    fn canonical_extensions() {
        assert_eq!(AudioCodec::Mp3.canonical_extension(), "mp3");
        assert_eq!(AudioCodec::Aac.canonical_extension(), "m4a");
        assert_eq!(AudioCodec::Opus.canonical_extension(), "opus");
        assert_eq!(AudioCodec::Vorbis.canonical_extension(), "ogg");
        assert_eq!(AudioCodec::Flac.canonical_extension(), "flac");
        assert_eq!(AudioCodec::Alac.canonical_extension(), "m4a");
    }
}
