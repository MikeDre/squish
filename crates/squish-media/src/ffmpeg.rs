//! ffmpeg binary detection and invocation.

use crate::error::MediaError;
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

/// Check that ffmpeg is available on PATH.
pub fn check_ffmpeg() -> Result<(), MediaError> {
    match Command::new("ffmpeg").arg("-version").output() {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(MediaError::MissingDependency {
            name: "ffmpeg".into(),
            install_hint: "brew install ffmpeg (macOS) or apt install ffmpeg (Linux)".into(),
        }),
    }
}

/// Run `ffmpeg -y -i <input> <args...> <output>`.
///
/// - On binary-not-found, returns `MediaError::MissingDependency`.
/// - On non-zero exit, removes the partial `output` file (best-effort) and
///   returns `MediaError::FfmpegFailed { path: input, stderr }`.
/// - On other I/O failure during spawn, returns `MediaError::Io`.
pub fn run_ffmpeg(
    input: &Path,
    output: &Path,
    args: &[OsString],
) -> Result<(), MediaError> {
    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.arg("-y");
    cmd.arg("-i").arg(input);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.arg(output);

    let result = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            MediaError::MissingDependency {
                name: "ffmpeg".into(),
                install_hint: "brew install ffmpeg (macOS) or apt install ffmpeg (Linux)".into(),
            }
        } else {
            MediaError::Io(e)
        }
    })?;

    if !result.status.success() {
        let _ = std::fs::remove_file(output);
        let stderr = String::from_utf8_lossy(&result.stderr).to_string();
        return Err(MediaError::FfmpegFailed {
            path: input.to_path_buf(),
            stderr,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn ffmpeg_available() -> bool {
        Command::new("ffmpeg")
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn check_ffmpeg_returns_ok_when_available() {
        if Command::new("ffmpeg").arg("-version").output().is_ok() {
            assert!(check_ffmpeg().is_ok());
        }
    }

    #[test]
    fn run_ffmpeg_fails_for_garbage_input_and_cleans_partial_output() {
        if !ffmpeg_available() {
            eprintln!("skipping: ffmpeg not present");
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let input = tmp.path().join("garbage.bin");
        std::fs::write(&input, b"this is not a media file at all").unwrap();
        let output = tmp.path().join("out.mp3");

        let args: Vec<OsString> = vec![OsString::from("-c:a"), OsString::from("copy")];
        let result = run_ffmpeg(&input, &output, &args);

        match result {
            Err(MediaError::FfmpegFailed { path, stderr }) => {
                assert_eq!(path, input);
                assert!(!stderr.is_empty(), "stderr should be populated on failure");
            }
            other => panic!("expected FfmpegFailed, got {other:?}"),
        }
        assert!(
            !output.exists(),
            "partial output file must be removed on failure"
        );
    }

    #[test]
    fn run_ffmpeg_returns_missing_dependency_when_binary_absent() {
        // Simulate a missing binary by pointing PATH at an empty dir.
        let tmp = tempfile::TempDir::new().unwrap();
        let input = tmp.path().join("any.bin");
        std::fs::write(&input, b"x").unwrap();
        let output = tmp.path().join("out.mp3");

        let original_path = std::env::var_os("PATH");
        std::env::set_var("PATH", tmp.path());

        let args: Vec<OsString> = vec![];
        let result = run_ffmpeg(&input, &output, &args);

        // Restore PATH before any assertion that could panic.
        match original_path {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }

        assert!(
            matches!(result, Err(MediaError::MissingDependency { ref name, .. }) if name == "ffmpeg"),
            "expected MissingDependency, got {result:?}"
        );
    }

    #[test]
    fn run_ffmpeg_succeeds_for_valid_passthrough() {
        if !ffmpeg_available() {
            eprintln!("skipping: ffmpeg not present");
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let input = tmp.path().join("sine.wav");
        // Generate a 0.2s sine wave WAV via ffmpeg lavfi.
        let gen = Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=0.2",
                "-ac",
                "1",
            ])
            .arg(&input)
            .output()
            .unwrap();
        assert!(gen.status.success(), "fixture generation failed");

        let output = tmp.path().join("out.wav");
        let args: Vec<OsString> =
            vec![OsString::from("-c:a"), OsString::from("copy")];
        run_ffmpeg(&input, &output, &args).expect("run_ffmpeg should succeed");
        assert!(output.exists(), "output file should exist on success");
    }
}
