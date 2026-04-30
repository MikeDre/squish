//! ffmpeg/ffprobe binary detection, command building, and execution.

use crate::error::AudioError;
use std::path::Path;
use std::process::Command;

/// Check that ffmpeg is available on PATH.
pub fn check_ffmpeg() -> Result<(), AudioError> {
    match Command::new("ffmpeg").arg("-version").output() {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(AudioError::MissingDependency {
            name: "ffmpeg".into(),
            install_hint: "brew install ffmpeg (macOS) or apt install ffmpeg (Linux)".into(),
        }),
    }
}

/// Check that ffprobe is available on PATH.
pub fn check_ffprobe() -> Result<(), AudioError> {
    match Command::new("ffprobe").arg("-version").output() {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(AudioError::MissingDependency {
            name: "ffprobe".into(),
            install_hint: "ffprobe ships with ffmpeg; brew install ffmpeg or apt install ffmpeg"
                .into(),
        }),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeKind {
    AudioOnly,
    HasVideo,
    Unknown,
}

/// Use ffprobe to detect whether `path` contains a video stream. Used to
/// disambiguate ambiguous containers (mp4/m4a/webm/mkv).
///
/// Returns `HasVideo` if at least one non-attached-picture video stream exists,
/// `AudioOnly` if only audio + (optionally) attached pictures, `Unknown` if
/// the probe yielded no usable answer.
pub fn ffprobe_kind(path: &Path) -> Result<ProbeKind, AudioError> {
    // Request `attached_pic` as its own column so we don't have to parse
    // ffprobe's bulk disposition list (which varies in shape between versions
    // and even drops entirely on some builds when the result is "no flags set").
    let output = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-show_entries", "stream=codec_type:stream_disposition=attached_pic",
            "-of", "csv=p=0",
        ])
        .arg(path)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AudioError::MissingDependency {
                    name: "ffprobe".into(),
                    install_hint:
                        "ffprobe ships with ffmpeg; brew install ffmpeg or apt install ffmpeg"
                            .into(),
                }
            } else {
                AudioError::Io(e)
            }
        })?;

    if !output.status.success() {
        return Ok(ProbeKind::Unknown);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut has_real_video = false;
    let mut has_audio = false;

    // Each line is `<codec_type>,<attached_pic_flag>` where the flag is "0" or "1".
    for line in stdout.lines() {
        let mut parts = line.split(',');
        let kind = parts.next().unwrap_or("").trim();
        let attached_pic = parts.next().unwrap_or("").trim() == "1";
        match kind {
            "video" if !attached_pic => has_real_video = true,
            "audio" => has_audio = true,
            _ => {}
        }
    }

    Ok(match (has_real_video, has_audio) {
        (true, _) => ProbeKind::HasVideo,
        (false, true) => ProbeKind::AudioOnly,
        _ => ProbeKind::Unknown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_ffmpeg_returns_ok_when_available() {
        if Command::new("ffmpeg").arg("-version").output().is_ok() {
            assert!(check_ffmpeg().is_ok());
        }
    }

    #[test]
    fn check_ffprobe_returns_ok_when_available() {
        if Command::new("ffprobe").arg("-version").output().is_ok() {
            assert!(check_ffprobe().is_ok());
        }
    }

    use std::path::PathBuf;
    use std::process::Command as ProcCommand;

    #[test]
    fn ffprobe_kind_audio_only_for_generated_wav() {
        if !ProcCommand::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false) {
            eprintln!("skipping: ffmpeg not present");
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("sine.wav");
        let status = ProcCommand::new("ffmpeg")
            .args(["-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=0.2", "-ac", "1"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(status.status.success(), "ffmpeg failed to generate fixture");

        match ffprobe_kind(&path).unwrap() {
            ProbeKind::AudioOnly => {}
            other => panic!("expected AudioOnly, got {other:?}"),
        }
    }

    #[test]
    fn ffprobe_kind_returns_unknown_for_missing_path() {
        // Probe a non-existent file; ffprobe writes to stderr and exits nonzero,
        // and we map that to ProbeKind::Unknown (not MissingDependency).
        let result = ffprobe_kind(&PathBuf::from("/definitely/does/not/exist.mp3"));
        assert!(matches!(result, Ok(ProbeKind::Unknown)));
    }

    #[test]
    fn ffprobe_kind_treats_attached_picture_as_audio_only() {
        if !ProcCommand::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false) {
            eprintln!("skipping: ffmpeg not present");
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let cover = tmp.path().join("cover.png");
        let audio = tmp.path().join("with_art.mp3");

        // Generate a single-frame PNG as the cover image.
        let cover_status = ProcCommand::new("ffmpeg")
            .args(["-y", "-f", "lavfi", "-i", "color=c=blue:s=64x64:d=1", "-frames:v", "1"])
            .arg(&cover)
            .output()
            .unwrap();
        assert!(cover_status.status.success(), "cover generation failed");

        // Wrap a sine wave + the cover as an MP3 with attached_pic disposition.
        let mp3_status = ProcCommand::new("ffmpeg")
            .args([
                "-y",
                "-f", "lavfi", "-i", "sine=frequency=440:duration=0.5",
                "-i",
            ])
            .arg(&cover)
            .args([
                "-map", "0", "-map", "1",
                "-c:v", "copy", "-c:a", "libmp3lame",
                "-id3v2_version", "3",
                "-disposition:v:0", "attached_pic",
            ])
            .arg(&audio)
            .output()
            .unwrap();
        assert!(mp3_status.status.success(), "mp3 mux failed: {}", String::from_utf8_lossy(&mp3_status.stderr));

        match ffprobe_kind(&audio).unwrap() {
            ProbeKind::AudioOnly => {}
            other => panic!("expected AudioOnly for attached-picture MP3, got {other:?}"),
        }
    }
}
