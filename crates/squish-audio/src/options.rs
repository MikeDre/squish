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

#[derive(Debug, Clone)]
pub struct AudioOptions {
    pub quality: Option<u8>,
    pub bitrate_kbps: Option<u32>,
    pub codec: Option<AudioCodec>,
    pub strip_tags: bool,
    pub force_overwrite: bool,
    pub suffix: Option<String>,
}

impl Default for AudioOptions {
    fn default() -> Self {
        AudioOptions {
            quality: None,
            bitrate_kbps: None,
            codec: None,
            strip_tags: false,
            force_overwrite: false,
            suffix: None,
        }
    }
}

pub fn default_audio_quality() -> u8 { 80 }

/// MP3 LAME `-q:a` value (0-9, lower = higher quality).
pub fn quality_to_mp3_v(q: u8) -> u8 {
    let q = q.min(100) as u32;
    let v = 9 - (q * 9 / 100);
    v.min(9) as u8
}

/// Vorbis `-q:a` value (0-10, higher = better).
pub fn quality_to_vorbis_q(q: u8) -> u8 {
    let q = q.min(100) as u32;
    (q * 10 / 100).min(10) as u8
}

/// FLAC compression level (0-12, higher = more effort).
pub fn quality_to_flac_level(q: u8) -> u8 {
    let q = q.min(100) as u32;
    (q * 12 / 100).min(12) as u8
}

/// AAC bitrate ladder lookup keyed by quality bucket.
pub fn quality_to_aac_bitrate(q: u8) -> u32 {
    match q.min(100) {
        0..=20 => 64,
        21..=40 => 96,
        41..=60 => 128,
        61..=80 => 192,
        81..=95 => 256,
        _ => 320,
    }
}

/// Opus bitrate ladder lookup keyed by quality bucket.
pub fn quality_to_opus_bitrate(q: u8) -> u32 {
    match q.min(100) {
        0..=20 => 32,
        21..=40 => 48,
        41..=60 => 64,
        61..=80 => 96,
        81..=95 => 128,
        _ => 160,
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

    #[test]
    fn default_options() {
        let o = AudioOptions::default();
        assert!(o.quality.is_none());
        assert!(o.bitrate_kbps.is_none());
        assert!(o.codec.is_none());
        assert!(!o.strip_tags);
        assert!(!o.force_overwrite);
        assert!(o.suffix.is_none());
    }

    #[test]
    fn mp3_v_mapping_boundaries() {
        assert_eq!(quality_to_mp3_v(0), 9);
        assert_eq!(quality_to_mp3_v(100), 0);
        assert_eq!(quality_to_mp3_v(80), 2);
        assert_eq!(quality_to_mp3_v(50), 5);
    }

    #[test]
    fn vorbis_q_mapping_boundaries() {
        assert_eq!(quality_to_vorbis_q(0), 0);
        assert_eq!(quality_to_vorbis_q(100), 10);
        assert_eq!(quality_to_vorbis_q(80), 8);
    }

    #[test]
    fn flac_level_mapping_boundaries() {
        assert_eq!(quality_to_flac_level(0), 0);
        assert_eq!(quality_to_flac_level(100), 12);
        assert_eq!(quality_to_flac_level(80), 9);
    }

    #[test]
    fn aac_bitrate_ladder() {
        assert_eq!(quality_to_aac_bitrate(0), 64);
        assert_eq!(quality_to_aac_bitrate(80), 192);
        assert_eq!(quality_to_aac_bitrate(100), 320);
    }

    #[test]
    fn opus_bitrate_ladder() {
        assert_eq!(quality_to_opus_bitrate(0), 32);
        assert_eq!(quality_to_opus_bitrate(80), 96);
        assert_eq!(quality_to_opus_bitrate(100), 160);
    }
}
