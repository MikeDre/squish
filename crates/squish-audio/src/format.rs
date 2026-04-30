use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioFormat {
    Mp3,
    M4a,
    Wav,
    Flac,
    Ogg,
    Opus,
    Webm,
    Aiff,
}

impl AudioFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "mp3",
            AudioFormat::M4a => "m4a",
            AudioFormat::Wav => "wav",
            AudioFormat::Flac => "flac",
            AudioFormat::Ogg => "ogg",
            AudioFormat::Opus => "opus",
            AudioFormat::Webm => "webm",
            AudioFormat::Aiff => "aiff",
        }
    }

    pub fn parse(s: &str) -> Option<AudioFormat> {
        match s.to_ascii_lowercase().as_str() {
            "mp3" => Some(AudioFormat::Mp3),
            "m4a" | "mp4" | "m4b" => Some(AudioFormat::M4a),
            "wav" | "wave" => Some(AudioFormat::Wav),
            "flac" => Some(AudioFormat::Flac),
            "ogg" | "oga" => Some(AudioFormat::Ogg),
            "opus" => Some(AudioFormat::Opus),
            "webm" => Some(AudioFormat::Webm),
            "aiff" | "aif" | "aifc" => Some(AudioFormat::Aiff),
            _ => None,
        }
    }

    /// True if the container could hold either audio-only or video streams,
    /// requiring an ffprobe call to disambiguate.
    pub fn is_ambiguous(&self) -> bool {
        matches!(self, AudioFormat::M4a | AudioFormat::Webm)
    }
}

/// Detect audio format from path extension and magic bytes fallback.
pub fn detect_audio_format(path: &Path) -> Option<AudioFormat> {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if let Some(fmt) = AudioFormat::parse(ext) {
            return Some(fmt);
        }
    }
    let head = read_head(path)?;
    detect_audio_by_magic(&head)
}

/// Detect audio format from path extension and provided bytes (no filesystem access).
pub fn detect_audio_from_bytes(path: &Path, head: &[u8]) -> Option<AudioFormat> {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if let Some(fmt) = AudioFormat::parse(ext) {
            return Some(fmt);
        }
    }
    detect_audio_by_magic(head)
}

fn read_head(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = vec![0u8; 16];
    let n = f.read(&mut head).ok()?;
    head.truncate(n);
    Some(head)
}

fn detect_audio_by_magic(head: &[u8]) -> Option<AudioFormat> {
    // ID3v2 tag (MP3 with header) or MPEG sync word (raw MP3)
    if head.len() >= 3 && &head[0..3] == b"ID3" {
        return Some(AudioFormat::Mp3);
    }
    if head.len() >= 2 && head[0] == 0xFF && (head[1] & 0xE0) == 0xE0 {
        return Some(AudioFormat::Mp3);
    }
    // FLAC
    if head.len() >= 4 && &head[0..4] == b"fLaC" {
        return Some(AudioFormat::Flac);
    }
    // RIFF WAVE
    if head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WAVE" {
        return Some(AudioFormat::Wav);
    }
    // Ogg (could be Vorbis or Opus; we report Ogg here, ffprobe distinguishes)
    if head.len() >= 4 && &head[0..4] == b"OggS" {
        return Some(AudioFormat::Ogg);
    }
    // AIFF (FORM ... AIFF)
    if head.len() >= 12 && &head[0..4] == b"FORM" && &head[8..12] == b"AIFF" {
        return Some(AudioFormat::Aiff);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parse_accepts_extensions() {
        assert_eq!(AudioFormat::parse("mp3"), Some(AudioFormat::Mp3));
        assert_eq!(AudioFormat::parse("MP3"), Some(AudioFormat::Mp3));
        assert_eq!(AudioFormat::parse("m4a"), Some(AudioFormat::M4a));
        assert_eq!(AudioFormat::parse("m4b"), Some(AudioFormat::M4a));
        assert_eq!(AudioFormat::parse("wav"), Some(AudioFormat::Wav));
        assert_eq!(AudioFormat::parse("flac"), Some(AudioFormat::Flac));
        assert_eq!(AudioFormat::parse("ogg"), Some(AudioFormat::Ogg));
        assert_eq!(AudioFormat::parse("opus"), Some(AudioFormat::Opus));
        assert_eq!(AudioFormat::parse("webm"), Some(AudioFormat::Webm));
        assert_eq!(AudioFormat::parse("aiff"), Some(AudioFormat::Aiff));
        assert_eq!(AudioFormat::parse("aif"), Some(AudioFormat::Aiff));
        assert_eq!(AudioFormat::parse("png"), None);
    }

    #[test]
    fn extension_is_canonical() {
        assert_eq!(AudioFormat::Mp3.extension(), "mp3");
        assert_eq!(AudioFormat::Flac.extension(), "flac");
        assert_eq!(AudioFormat::Opus.extension(), "opus");
    }

    #[test]
    fn is_ambiguous_only_for_overlapping_containers() {
        assert!(AudioFormat::M4a.is_ambiguous());
        assert!(AudioFormat::Webm.is_ambiguous());
        assert!(!AudioFormat::Mp3.is_ambiguous());
        assert!(!AudioFormat::Flac.is_ambiguous());
        assert!(!AudioFormat::Ogg.is_ambiguous());
    }

    #[test]
    fn detect_mp3_id3_magic() {
        let head = b"ID3\x04\x00";
        assert_eq!(
            detect_audio_from_bytes(&PathBuf::from("x.xyz"), head),
            Some(AudioFormat::Mp3)
        );
    }

    #[test]
    fn detect_mp3_sync_magic() {
        let head = [0xFF, 0xFB, 0x90, 0x00];
        assert_eq!(
            detect_audio_from_bytes(&PathBuf::from("x.xyz"), &head),
            Some(AudioFormat::Mp3)
        );
    }

    #[test]
    fn detect_flac_magic() {
        let head = b"fLaC\x00\x00";
        assert_eq!(
            detect_audio_from_bytes(&PathBuf::from("x.xyz"), head),
            Some(AudioFormat::Flac)
        );
    }

    #[test]
    fn detect_wav_magic() {
        let mut head = [0u8; 12];
        head[0..4].copy_from_slice(b"RIFF");
        head[8..12].copy_from_slice(b"WAVE");
        assert_eq!(
            detect_audio_from_bytes(&PathBuf::from("x.xyz"), &head),
            Some(AudioFormat::Wav)
        );
    }

    #[test]
    fn detect_ogg_magic() {
        let head = b"OggS\x00\x02";
        assert_eq!(
            detect_audio_from_bytes(&PathBuf::from("x.xyz"), head),
            Some(AudioFormat::Ogg)
        );
    }

    #[test]
    fn detect_aiff_magic() {
        let mut head = [0u8; 12];
        head[0..4].copy_from_slice(b"FORM");
        head[8..12].copy_from_slice(b"AIFF");
        assert_eq!(
            detect_audio_from_bytes(&PathBuf::from("x.xyz"), &head),
            Some(AudioFormat::Aiff)
        );
    }

    #[test]
    fn detect_returns_none_for_unknown() {
        assert_eq!(
            detect_audio_from_bytes(&PathBuf::from("x.xyz"), b"random"),
            None
        );
    }

    #[test]
    fn detect_extension_takes_priority() {
        // .mp3 extension wins even with non-MP3 bytes
        assert_eq!(
            detect_audio_from_bytes(&PathBuf::from("x.mp3"), b"random"),
            Some(AudioFormat::Mp3)
        );
    }
}
