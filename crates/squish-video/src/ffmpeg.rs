//! Codec-specific argv construction. Delegates execution to squish-media.

use crate::options::{VideoCodec, VideoOptions};
use crate::VideoError;
use std::ffi::OsString;
use std::path::Path;

/// Build the codec-specific portion of an ffmpeg argv. Returns the full argv
/// after `ffmpeg -y -i <input>` and before `<output>`.
pub fn build_codec_args(out_ext: &str, opts: &VideoOptions, force_reencode: bool) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();
    let codec = opts.effective_codec_for_ext_reencode(out_ext, force_reencode);

    if codec == VideoCodec::Copy {
        args.push("-c".into());
        args.push("copy".into());
    } else {
        args.push("-c:v".into());
        args.push(codec.ffmpeg_encoder().into());

        if let Some(crf) = opts.effective_crf_for_codec(codec) {
            args.push("-crf".into());
            args.push(crf.to_string().into());
        }

        match codec {
            VideoCodec::H264 | VideoCodec::H265 => {
                args.push("-preset".into());
                args.push("medium".into());
            }
            VideoCodec::AV1 => {
                args.push("-preset".into());
                args.push("6".into());
            }
            VideoCodec::Vp9 => {
                args.push("-b:v".into());
                args.push("0".into());
            }
            VideoCodec::Copy => unreachable!(),
        }

        // ffmpeg defaults H.265 in MP4/MOV to the `hev1` codec tag, which
        // QuickTime/Safari/iOS refuse to decode. Force `hvc1` so the output
        // plays everywhere H.265 is supported.
        if codec == VideoCodec::H265 && matches!(out_ext, "mp4" | "m4v" | "mov") {
            args.push("-tag:v".into());
            args.push("hvc1".into());
        }

        args.push("-c:a".into());
        args.push("copy".into());
        args.push("-c:s".into());
        args.push("copy".into());
    }

    args.push("-map_metadata".into());
    args.push("-1".into());

    args
}

/// Build and run an ffmpeg command to compress `input` to `output`.
pub fn run_ffmpeg(
    input: &Path,
    output: &Path,
    opts: &VideoOptions,
    force_reencode: bool,
) -> Result<(), VideoError> {
    let out_ext = output
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let args = build_codec_args(out_ext, opts, force_reencode);
    squish_media::run_ffmpeg(input, output, &args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{VideoCodec, VideoOptions};

    fn args(out_ext: &str, opts: VideoOptions) -> Vec<String> {
        build_codec_args(out_ext, &opts, false)
            .into_iter()
            .map(|o| o.into_string().unwrap())
            .collect()
    }

    #[test]
    fn force_reencode_replaces_fast_copy() {
        let opts = VideoOptions { fast: true, ..Default::default() };
        let a: Vec<String> = build_codec_args("mp4", &opts, true)
            .into_iter()
            .map(|o| o.into_string().unwrap())
            .collect();
        assert!(a.contains(&"-c:v".to_string()));
        assert!(a.contains(&"libx265".to_string()));
        assert!(!a.windows(2).any(|w| w == ["-c", "copy"]));
    }

    #[test]
    fn h265_mp4_emits_hvc1_tag() {
        let opts = VideoOptions {
            codec: Some(VideoCodec::H265),
            ..Default::default()
        };
        let a = args("mp4", opts);
        let pos = a.iter().position(|s| s == "-tag:v").expect("expected -tag:v");
        assert_eq!(a[pos + 1], "hvc1");
    }

    #[test]
    fn h264_does_not_emit_hvc1() {
        let opts = VideoOptions {
            codec: Some(VideoCodec::H264),
            ..Default::default()
        };
        let a = args("mp4", opts);
        assert!(!a.contains(&"-tag:v".to_string()));
    }

    #[test]
    fn copy_codec_only_emits_c_copy() {
        let opts = VideoOptions {
            codec: Some(VideoCodec::Copy),
            ..Default::default()
        };
        let a = args("mp4", opts);
        assert!(a.contains(&"-c".to_string()));
        assert!(a.contains(&"copy".to_string()));
        assert!(!a.contains(&"-c:v".to_string()));
    }

    #[test]
    fn vp9_emits_bv_zero() {
        let opts = VideoOptions {
            codec: Some(VideoCodec::Vp9),
            ..Default::default()
        };
        let a = args("webm", opts);
        let pos = a.iter().position(|s| s == "-b:v").expect("expected -b:v");
        assert_eq!(a[pos + 1], "0");
    }
}
