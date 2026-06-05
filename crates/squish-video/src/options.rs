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

    /// Inclusive CRF range mapped onto the 0-100 quality dial.
    /// `(min_crf @ q=100, max_crf @ q=0)`. The bounds correspond to
    /// "visually lossless" and "noticeably degraded" within each codec's
    /// effective quality envelope, not the codec's full theoretical range.
    fn crf_range(&self) -> (u8, u8) {
        match self {
            VideoCodec::H264 => (18, 32),
            VideoCodec::H265 => (22, 38),
            VideoCodec::AV1 => (25, 50),
            VideoCodec::Vp9 => (25, 50),
            VideoCodec::Copy => (0, 0),
        }
    }
}

/// Default codec for a container when no explicit codec is chosen.
/// WebM only allows VP8/VP9/AV1; everything else defaults to H.265.
fn container_default_codec(ext: &str) -> VideoCodec {
    match ext.to_ascii_lowercase().as_str() {
        "webm" => VideoCodec::Vp9,
        _ => VideoCodec::H265,
    }
}

#[derive(Debug, Clone, Default)]
pub struct VideoOptions {
    pub quality: Option<u8>,
    pub codec: Option<VideoCodec>,
    pub fast: bool,
    pub force_overwrite: bool,
    /// Custom output suffix. `None` means "squished".
    pub suffix: Option<String>,

    /// Replace the input file in place instead of writing a `_squished` copy.
    pub overwrite: bool,

    /// Explicit output container override (set via CLI `--format`). When
    /// `None`, the pipeline falls back to `format_in.output_format()` (which
    /// handles the DV → MP4 transcode-only rule).
    pub output_format: Option<crate::format::VideoFormat>,
}

impl VideoOptions {
    pub fn effective_codec(&self) -> VideoCodec {
        if self.fast {
            return VideoCodec::Copy;
        }
        self.codec.unwrap_or(VideoCodec::H265)
    }

    /// Like `effective_codec`, but falls back to a container-appropriate default
    /// when no explicit codec is set. WebM only allows VP8/VP9/AV1; use VP9.
    pub fn effective_codec_for_ext(&self, ext: &str) -> VideoCodec {
        if self.fast {
            return VideoCodec::Copy;
        }
        if let Some(c) = self.codec {
            return c;
        }
        container_default_codec(ext)
    }

    /// Like `effective_codec_for_ext`, but when the input must be transcoded
    /// (`force_reencode`), a `Copy` selection is invalid — the source stream
    /// cannot be muxed into the target container — so fall back to the
    /// container default codec instead.
    pub fn effective_codec_for_ext_reencode(&self, ext: &str, force_reencode: bool) -> VideoCodec {
        let codec = self.effective_codec_for_ext(ext);
        if force_reencode && codec == VideoCodec::Copy {
            return container_default_codec(ext);
        }
        codec
    }

    pub fn effective_crf(&self) -> Option<u8> {
        let codec = self.effective_codec();
        if codec == VideoCodec::Copy {
            return None;
        }
        let quality = self.quality.unwrap_or(default_video_quality());
        Some(quality_to_crf(quality, codec))
    }

    pub fn effective_crf_for_codec(&self, codec: VideoCodec) -> Option<u8> {
        if codec == VideoCodec::Copy {
            return None;
        }
        let quality = self.quality.unwrap_or(default_video_quality());
        Some(quality_to_crf(quality, codec))
    }
}

pub fn default_video_quality() -> u8 {
    80
}

pub fn quality_to_crf(quality: u8, codec: VideoCodec) -> u8 {
    let (min_crf, max_crf) = codec.crf_range();
    if codec == VideoCodec::Copy {
        return 0;
    }
    let q = quality.min(100) as f64 / 100.0;
    let span = (max_crf as f64) - (min_crf as f64);
    ((max_crf as f64) - q * span).round() as u8
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
        let o = VideoOptions {
            codec: Some(VideoCodec::AV1),
            ..Default::default()
        };
        assert_eq!(o.effective_codec(), VideoCodec::AV1);
    }

    #[test]
    fn fast_mode_forces_copy() {
        let o = VideoOptions {
            fast: true,
            codec: Some(VideoCodec::H264),
            ..Default::default()
        };
        assert_eq!(o.effective_codec(), VideoCodec::Copy);
    }

    #[test]
    fn quality_to_crf_h265_endpoints_and_default() {
        assert_eq!(quality_to_crf(100, VideoCodec::H265), 22);
        assert_eq!(quality_to_crf(0, VideoCodec::H265), 38);
        // q=80 sits 80% of the way from max(38) to min(22) → 38 - 0.8*16 = 25.2 → 25
        assert_eq!(quality_to_crf(80, VideoCodec::H265), 25);
    }

    #[test]
    fn quality_to_crf_h264_endpoints() {
        assert_eq!(quality_to_crf(100, VideoCodec::H264), 18);
        assert_eq!(quality_to_crf(0, VideoCodec::H264), 32);
        // q=80 → 32 - 0.8*14 = 20.8 → 21
        assert_eq!(quality_to_crf(80, VideoCodec::H264), 21);
    }

    #[test]
    fn quality_to_crf_av1_endpoints() {
        assert_eq!(quality_to_crf(100, VideoCodec::AV1), 25);
        assert_eq!(quality_to_crf(0, VideoCodec::AV1), 50);
        // q=80 → 50 - 0.8*25 = 30
        assert_eq!(quality_to_crf(80, VideoCodec::AV1), 30);
    }

    #[test]
    fn quality_to_crf_vp9_endpoints() {
        assert_eq!(quality_to_crf(100, VideoCodec::Vp9), 25);
        assert_eq!(quality_to_crf(0, VideoCodec::Vp9), 50);
    }

    #[test]
    fn effective_crf_none_for_copy() {
        let o = VideoOptions {
            fast: true,
            ..Default::default()
        };
        assert_eq!(o.effective_crf(), None);
    }

    #[test]
    fn effective_crf_uses_default_quality() {
        // Default quality 80 + default codec H.265 → CRF 25
        let o = VideoOptions::default();
        assert_eq!(o.effective_crf().unwrap(), 25);
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

    #[test]
    fn reencode_overrides_fast_copy_to_default() {
        // --fast normally forces Copy; for a forced re-encode that's invalid.
        let o = VideoOptions { fast: true, ..Default::default() };
        assert_eq!(o.effective_codec_for_ext(""), VideoCodec::Copy);
        assert_eq!(o.effective_codec_for_ext_reencode("mp4", true), VideoCodec::H265);
        assert_eq!(o.effective_codec_for_ext_reencode("webm", true), VideoCodec::Vp9);
    }

    #[test]
    fn reencode_overrides_explicit_copy() {
        let o = VideoOptions { codec: Some(VideoCodec::Copy), ..Default::default() };
        assert_eq!(o.effective_codec_for_ext_reencode("mp4", true), VideoCodec::H265);
    }

    #[test]
    fn reencode_false_preserves_normal_selection() {
        let o = VideoOptions { fast: true, ..Default::default() };
        assert_eq!(o.effective_codec_for_ext_reencode("mp4", false), VideoCodec::Copy);
    }

    #[test]
    fn reencode_does_not_override_real_codec() {
        let o = VideoOptions { codec: Some(VideoCodec::AV1), ..Default::default() };
        assert_eq!(o.effective_codec_for_ext_reencode("mp4", true), VideoCodec::AV1);
    }
}
