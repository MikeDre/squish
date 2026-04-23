use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoFormat {
    Mp4,
    Webm,
    Mov,
    Avi,
    Mkv,
    Flv,
}

impl VideoFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            VideoFormat::Mp4 => "mp4",
            VideoFormat::Webm => "webm",
            VideoFormat::Mov => "mov",
            VideoFormat::Avi => "avi",
            VideoFormat::Mkv => "mkv",
            VideoFormat::Flv => "flv",
        }
    }

    pub fn parse(s: &str) -> Option<VideoFormat> {
        match s.to_ascii_lowercase().as_str() {
            "mp4" | "m4v" => Some(VideoFormat::Mp4),
            "webm" => Some(VideoFormat::Webm),
            "mov" => Some(VideoFormat::Mov),
            "avi" => Some(VideoFormat::Avi),
            "mkv" => Some(VideoFormat::Mkv),
            "flv" => Some(VideoFormat::Flv),
            _ => None,
        }
    }
}

/// Detect video format from path extension and magic bytes fallback.
pub fn detect_video_format(path: &Path) -> Option<VideoFormat> {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if let Some(fmt) = VideoFormat::parse(ext) {
            return Some(fmt);
        }
    }
    let head = read_head(path)?;
    detect_video_by_magic(&head)
}

/// Detect video format from path extension and provided bytes (no filesystem access).
pub fn detect_video_from_bytes(path: &Path, head: &[u8]) -> Option<VideoFormat> {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if let Some(fmt) = VideoFormat::parse(ext) {
            return Some(fmt);
        }
    }
    detect_video_by_magic(head)
}

fn read_head(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = vec![0u8; 32];
    let n = f.read(&mut head).ok()?;
    head.truncate(n);
    Some(head)
}

fn detect_video_by_magic(head: &[u8]) -> Option<VideoFormat> {
    if head.len() >= 8 && &head[4..8] == b"ftyp" {
        if head.len() >= 12 {
            let brand = &head[8..12];
            if brand == b"qt  " {
                return Some(VideoFormat::Mov);
            }
        }
        return Some(VideoFormat::Mp4);
    }
    if head.len() >= 4 && head[0..4] == [0x1A, 0x45, 0xDF, 0xA3] {
        return Some(VideoFormat::Mkv);
    }
    if head.len() >= 12 && head.starts_with(b"RIFF") && &head[8..12] == b"AVI " {
        return Some(VideoFormat::Avi);
    }
    if head.len() >= 3 && head.starts_with(b"FLV") {
        return Some(VideoFormat::Flv);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parse_accepts_extensions() {
        assert_eq!(VideoFormat::parse("mp4"), Some(VideoFormat::Mp4));
        assert_eq!(VideoFormat::parse("MP4"), Some(VideoFormat::Mp4));
        assert_eq!(VideoFormat::parse("m4v"), Some(VideoFormat::Mp4));
        assert_eq!(VideoFormat::parse("webm"), Some(VideoFormat::Webm));
        assert_eq!(VideoFormat::parse("mov"), Some(VideoFormat::Mov));
        assert_eq!(VideoFormat::parse("avi"), Some(VideoFormat::Avi));
        assert_eq!(VideoFormat::parse("mkv"), Some(VideoFormat::Mkv));
        assert_eq!(VideoFormat::parse("flv"), Some(VideoFormat::Flv));
        assert_eq!(VideoFormat::parse("png"), None);
    }

    #[test]
    fn extension_is_canonical() {
        assert_eq!(VideoFormat::Mp4.extension(), "mp4");
        assert_eq!(VideoFormat::Mkv.extension(), "mkv");
    }

    #[test]
    fn detect_from_extension() {
        assert_eq!(detect_video_from_bytes(&PathBuf::from("x.mp4"), &[]), Some(VideoFormat::Mp4));
        assert_eq!(detect_video_from_bytes(&PathBuf::from("x.webm"), &[]), Some(VideoFormat::Webm));
    }

    #[test]
    fn detect_mp4_from_ftyp_magic() {
        let mut head = [0u8; 12];
        head[4..8].copy_from_slice(b"ftyp");
        head[8..12].copy_from_slice(b"isom");
        assert_eq!(detect_video_from_bytes(&PathBuf::from("x.xyz"), &head), Some(VideoFormat::Mp4));
    }

    #[test]
    fn detect_mov_from_ftyp_qt_magic() {
        let mut head = [0u8; 12];
        head[4..8].copy_from_slice(b"ftyp");
        head[8..12].copy_from_slice(b"qt  ");
        assert_eq!(detect_video_from_bytes(&PathBuf::from("x.xyz"), &head), Some(VideoFormat::Mov));
    }

    #[test]
    fn detect_mkv_from_ebml_magic() {
        let head = [0x1A, 0x45, 0xDF, 0xA3, 0x00, 0x00];
        assert_eq!(detect_video_from_bytes(&PathBuf::from("x.xyz"), &head), Some(VideoFormat::Mkv));
    }

    #[test]
    fn detect_avi_from_riff_magic() {
        let mut head = [0u8; 12];
        head[0..4].copy_from_slice(b"RIFF");
        head[8..12].copy_from_slice(b"AVI ");
        assert_eq!(detect_video_from_bytes(&PathBuf::from("x.xyz"), &head), Some(VideoFormat::Avi));
    }

    #[test]
    fn detect_flv_from_magic() {
        let head = b"FLV\x01\x00";
        assert_eq!(detect_video_from_bytes(&PathBuf::from("x.xyz"), head), Some(VideoFormat::Flv));
    }

    #[test]
    fn detect_returns_none_for_unknown() {
        assert_eq!(detect_video_from_bytes(&PathBuf::from("x.xyz"), b"random bytes"), None);
    }
}
