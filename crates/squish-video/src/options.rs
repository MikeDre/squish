#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    H265,
    AV1,
    Vp9,
    Copy,
}

impl VideoCodec {
    pub fn ffmpeg_encoder(&self) -> &'static str {
        match self {
            VideoCodec::H264 => "libx264",
            VideoCodec::H265 => "libx265",
            VideoCodec::AV1 => "libsvtav1",
            VideoCodec::Vp9 => "libvpx-vp9",
            VideoCodec::Copy => "copy",
        }
    }

    pub fn parse(s: &str) -> Option<VideoCodec> {
        match s.to_ascii_lowercase().as_str() {
            "h264" | "x264" | "avc" => Some(VideoCodec::H264),
            "h265" | "x265" | "hevc" => Some(VideoCodec::H265),
            "av1" | "svtav1" => Some(VideoCodec::AV1),
            "vp9" | "libvpx-vp9" => Some(VideoCodec::Vp9),
            "copy" => Some(VideoCodec::Copy),
            _ => None,
        }
    }

    fn crf_max(&self) -> u8 {
        match self {
            VideoCodec::H264 | VideoCodec::H265 => 51,
            VideoCodec::AV1 => 63,
            VideoCodec::Vp9 => 63,
            VideoCodec::Copy => 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VideoOptions {
    pub quality: Option<u8>,
    pub codec: Option<VideoCodec>,
    pub fast: bool,
    pub force_overwrite: bool,
}

impl Default for VideoOptions {
    fn default() -> Self {
        VideoOptions {
            quality: None,
            codec: None,
            fast: false,
            force_overwrite: false,
        }
    }
}

impl VideoOptions {
    pub fn effective_codec(&self) -> VideoCodec {
        if self.fast { return VideoCodec::Copy; }
        self.codec.unwrap_or(VideoCodec::H265)
    }

    /// Like `effective_codec`, but falls back to a container-appropriate default
    /// when no explicit codec is set. WebM only allows VP8/VP9/AV1; use VP9.
    pub fn effective_codec_for_ext(&self, ext: &str) -> VideoCodec {
        if self.fast { return VideoCodec::Copy; }
        if let Some(c) = self.codec { return c; }
        match ext.to_ascii_lowercase().as_str() {
            "webm" => VideoCodec::Vp9,
            _ => VideoCodec::H265,
        }
    }

    pub fn effective_crf(&self) -> Option<u8> {
        let codec = self.effective_codec();
        if codec == VideoCodec::Copy { return None; }
        let quality = self.quality.unwrap_or(default_video_quality());
        Some(quality_to_crf(quality, codec))
    }

    pub fn effective_crf_for_codec(&self, codec: VideoCodec) -> Option<u8> {
        if codec == VideoCodec::Copy { return None; }
        let quality = self.quality.unwrap_or(default_video_quality());
        Some(quality_to_crf(quality, codec))
    }
}

pub fn default_video_quality() -> u8 { 80 }

pub fn quality_to_crf(quality: u8, codec: VideoCodec) -> u8 {
    let crf_max = codec.crf_max() as f64;
    let q = quality.min(100) as f64;
    let crf = crf_max - (q / 100.0 * crf_max);
    crf.round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options() {
        let o = VideoOptions::default();
        assert!(o.quality.is_none());
        assert!(o.codec.is_none());
        assert!(!o.fast);
        assert!(!o.force_overwrite);
    }

    #[test]
    fn effective_codec_defaults_to_h265() {
        assert_eq!(VideoOptions::default().effective_codec(), VideoCodec::H265);
    }

    #[test]
    fn effective_codec_uses_override() {
        let o = VideoOptions { codec: Some(VideoCodec::AV1), ..Default::default() };
        assert_eq!(o.effective_codec(), VideoCodec::AV1);
    }

    #[test]
    fn fast_mode_forces_copy() {
        let o = VideoOptions { fast: true, codec: Some(VideoCodec::H264), ..Default::default() };
        assert_eq!(o.effective_codec(), VideoCodec::Copy);
    }

    #[test]
    fn quality_to_crf_h265() {
        assert_eq!(quality_to_crf(100, VideoCodec::H265), 0);
        assert_eq!(quality_to_crf(0, VideoCodec::H265), 51);
        assert_eq!(quality_to_crf(80, VideoCodec::H265), 10);
    }

    #[test]
    fn quality_to_crf_av1() {
        assert_eq!(quality_to_crf(100, VideoCodec::AV1), 0);
        assert_eq!(quality_to_crf(0, VideoCodec::AV1), 63);
    }

    #[test]
    fn effective_crf_none_for_copy() {
        let o = VideoOptions { fast: true, ..Default::default() };
        assert_eq!(o.effective_crf(), None);
    }

    #[test]
    fn effective_crf_uses_default_quality() {
        let o = VideoOptions::default();
        assert_eq!(o.effective_crf().unwrap(), 10);
    }

    #[test]
    fn codec_parse() {
        assert_eq!(VideoCodec::parse("h264"), Some(VideoCodec::H264));
        assert_eq!(VideoCodec::parse("H265"), Some(VideoCodec::H265));
        assert_eq!(VideoCodec::parse("hevc"), Some(VideoCodec::H265));
        assert_eq!(VideoCodec::parse("av1"), Some(VideoCodec::AV1));
        assert_eq!(VideoCodec::parse("copy"), Some(VideoCodec::Copy));
        assert_eq!(VideoCodec::parse("vp9"), Some(VideoCodec::Vp9));
        assert_eq!(VideoCodec::parse("libvpx-vp9"), Some(VideoCodec::Vp9));
    }

    #[test]
    fn codec_ffmpeg_encoder() {
        assert_eq!(VideoCodec::H264.ffmpeg_encoder(), "libx264");
        assert_eq!(VideoCodec::H265.ffmpeg_encoder(), "libx265");
        assert_eq!(VideoCodec::AV1.ffmpeg_encoder(), "libsvtav1");
        assert_eq!(VideoCodec::Vp9.ffmpeg_encoder(), "libvpx-vp9");
        assert_eq!(VideoCodec::Copy.ffmpeg_encoder(), "copy");
    }
}
