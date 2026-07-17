//! Destination presets: expand `--preset web` into overridable defaults.

use crate::cli::{Args, Preset, QualityArg};
use crate::config::FileConfig;

/// Apply a preset's defaults to `args`/`file_cfg`, filling only values the CLI
/// left unset (so explicit flags win). Runs before `apply_file_config`, so
/// preset-set values also beat config. The video codec is routed through
/// `file_cfg.video.codec` (the ambient, non-batch-validated codec path) so a
/// preset never errors on a batch that has no video.
pub fn apply_preset(args: &mut Args, file_cfg: &mut FileConfig, preset: Preset) {
    match preset {
        Preset::Web => {
            if args.max_width.is_none() {
                args.max_width = Some(1920);
            }
            if args.max_height.is_none() {
                args.max_height = Some(1920);
            }
            if args.format.is_none() {
                args.format = Some("webp".to_string());
            }
            // Video → H.264 via the ambient config-codec path (read directly
            // from file_cfg later, not validated against the batch). Overwrites
            // any config-supplied video codec (preset > config), but only when
            // the CLI gave no --codec (CLI > preset).
            if args.codec.is_none() {
                file_cfg.video.codec = Some("h264".to_string());
            }
            // Rate control is all-or-nothing: supply `quality auto` only when
            // the CLI gave no explicit rate flag (mirrors apply_file_config).
            let cli_rate = args.quality.is_some()
                || args.lossless
                || args.bitrate.is_some()
                || args.fast
                || args.target_size.is_some();
            if !cli_rate {
                args.quality = Some(QualityArg::Auto);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn args_from(argv: &[&str]) -> Args {
        Args::parse_from(argv)
    }

    #[test]
    fn web_sets_defaults_on_clean_args() {
        let mut args = args_from(&["squish", "input.png"]);
        let mut cfg = FileConfig::default();
        apply_preset(&mut args, &mut cfg, Preset::Web);
        assert_eq!(args.max_width, Some(1920));
        assert_eq!(args.max_height, Some(1920));
        assert_eq!(args.format.as_deref(), Some("webp"));
        assert_eq!(args.quality, Some(QualityArg::Auto));
        assert_eq!(cfg.video.codec.as_deref(), Some("h264"));
    }

    #[test]
    fn explicit_cli_flags_win_over_preset() {
        let mut args = args_from(&[
            "squish",
            "input.png",
            "--quality",
            "90",
            "--max-width",
            "800",
        ]);
        let mut cfg = FileConfig::default();
        apply_preset(&mut args, &mut cfg, Preset::Web);
        assert_eq!(args.quality, Some(QualityArg::Fixed(90)));
        assert_eq!(args.max_width, Some(800));
        assert_eq!(args.format.as_deref(), Some("webp"));
    }

    #[test]
    fn explicit_max_height_wins_over_preset() {
        let mut args = args_from(&["squish", "input.png", "--max-height", "400"]);
        let mut cfg = FileConfig::default();
        apply_preset(&mut args, &mut cfg, Preset::Web);
        assert_eq!(args.max_height, Some(400));
        assert_eq!(args.max_width, Some(1920));
    }

    #[test]
    fn explicit_codec_suppresses_preset_video_codec() {
        let mut args = args_from(&["squish", "clip.mp4", "--codec", "av1"]);
        let mut cfg = FileConfig::default();
        apply_preset(&mut args, &mut cfg, Preset::Web);
        assert_eq!(cfg.video.codec, None);
        assert_eq!(args.codec.as_deref(), Some("av1"));
    }

    #[test]
    fn web_video_codec_overrides_config_codec() {
        // Precedence is CLI > preset > config: with no CLI --codec, the preset's
        // h264 must override a codec the config file supplied.
        let mut args = args_from(&["squish", "clip.mp4"]);
        let mut cfg = FileConfig::default();
        cfg.video.codec = Some("av1".to_string());
        apply_preset(&mut args, &mut cfg, Preset::Web);
        assert_eq!(cfg.video.codec.as_deref(), Some("h264"));
    }

    #[test]
    fn cli_rate_flag_suppresses_preset_quality_auto() {
        let mut args = args_from(&["squish", "input.png", "--target-size", "1M"]);
        let mut cfg = FileConfig::default();
        apply_preset(&mut args, &mut cfg, Preset::Web);
        assert_eq!(args.quality, None);
        assert_eq!(args.format.as_deref(), Some("webp"));
    }
}
