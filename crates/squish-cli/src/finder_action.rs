//! macOS Finder Quick Action: generate ~/Library/Services/Squish.workflow.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const INFO_PLIST: &str = include_str!("finder_action/Info.plist.tmpl");
const DOCUMENT_WFLOW_TMPL: &str = include_str!("finder_action/document.wflow.tmpl");
const SHELL_SCRIPT_TMPL: &str = include_str!("finder_action/quick-action.zsh.tmpl");

/// The zsh script run by the Quick Action. The absolute path to the squish
/// binary is baked in so the Service works regardless of the user's PATH.
/// Assumes `squish_bin` is a plain filesystem path with no shell-special
/// characters (always true for `current_exe()` on macOS).
fn shell_script(squish_bin: &Path) -> String {
    SHELL_SCRIPT_TMPL.replace("@@SQUISH_BIN@@", &squish_bin.display().to_string())
}

/// Escape text for embedding in a plist <string> element. Ampersand first.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn document_wflow(squish_bin: &Path) -> String {
    DOCUMENT_WFLOW_TMPL.replace("@@COMMAND_STRING@@", &xml_escape(&shell_script(squish_bin)))
}

fn info_plist() -> String {
    INFO_PLIST.to_string()
}

pub const WORKFLOW_DIR_NAME: &str = "Squish.workflow";

/// Where Quick Actions live. `SQUISH_SERVICES_DIR` overrides for tests,
/// mirroring the existing SQUISH_GLOBAL_CONFIG test hook.
pub fn services_dir() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("SQUISH_SERVICES_DIR") {
        return Ok(PathBuf::from(p));
    }
    dirs::home_dir()
        .map(|h| h.join("Library/Services"))
        .context("cannot determine home directory")
}

/// Write (or rewrite) the Squish.workflow bundle. Idempotent.
pub fn install_into(services_dir: &Path, squish_bin: &Path) -> Result<PathBuf> {
    let bundle = services_dir.join(WORKFLOW_DIR_NAME);
    let contents = bundle.join("Contents");
    // The bundle only ever contains these two files; if that changes,
    // reinstall would need to clear stale files from Contents/ first.
    std::fs::create_dir_all(&contents)
        .with_context(|| format!("creating {}", contents.display()))?;
    let info_path = contents.join("Info.plist");
    std::fs::write(&info_path, info_plist())
        .with_context(|| format!("writing {}", info_path.display()))?;
    let wflow_path = contents.join("document.wflow");
    std::fs::write(&wflow_path, document_wflow(squish_bin))
        .with_context(|| format!("writing {}", wflow_path.display()))?;
    Ok(bundle)
}

/// Remove the bundle. Returns false if it wasn't installed.
pub fn uninstall_from(services_dir: &Path) -> Result<bool> {
    let bundle = services_dir.join(WORKFLOW_DIR_NAME);
    match std::fs::remove_dir_all(&bundle) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).with_context(|| format!("removing {}", bundle.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn script_bakes_in_binary_path_and_media_kinds() {
        let s = shell_script(Path::new("/opt/homebrew/bin/squish"));
        assert!(s.contains(r#"bin="/opt/homebrew/bin/squish""#));
        assert!(s.contains("--kinds image,video,audio"));
        assert!(s.contains("display notification"));
        assert!(!s.contains("@@SQUISH_BIN@@"));
    }

    #[test]
    fn xml_escape_handles_amp_first() {
        assert_eq!(xml_escape("2>&1 <x>"), "2&gt;&amp;1 &lt;x&gt;");
    }

    #[test]
    fn wflow_embeds_escaped_script() {
        let doc = document_wflow(Path::new("/usr/local/bin/squish"));
        assert!(doc.contains("2&gt;&amp;1"), "shell redirection must be XML-escaped");
        assert!(doc.contains("/bin/zsh"));
        assert!(doc.contains("com.apple.RunShellScript"));
        assert!(!doc.contains("@@COMMAND_STRING@@"));
    }

    #[test]
    fn info_plist_declares_finder_service_for_items() {
        let plist = info_plist();
        assert!(plist.contains("com.apple.finder"));
        assert!(plist.contains("public.item"));
        assert!(plist.contains("<string>Squish</string>"));
    }

    #[test]
    fn install_writes_bundle_then_uninstall_removes_it() {
        let tmp = tempfile::TempDir::new().unwrap();
        let bundle = install_into(tmp.path(), Path::new("/usr/local/bin/squish")).unwrap();

        assert_eq!(bundle, tmp.path().join("Squish.workflow"));
        let info = std::fs::read_to_string(bundle.join("Contents/Info.plist")).unwrap();
        assert!(info.contains("com.apple.finder"));
        let doc = std::fs::read_to_string(bundle.join("Contents/document.wflow")).unwrap();
        assert!(doc.contains("/usr/local/bin/squish"));

        assert!(uninstall_from(tmp.path()).unwrap());
        assert!(!bundle.exists());
        // Second uninstall: nothing there, reports false, no error.
        assert!(!uninstall_from(tmp.path()).unwrap());
    }

    #[test]
    fn install_is_idempotent_and_refreshes_content() {
        let tmp = tempfile::TempDir::new().unwrap();
        install_into(tmp.path(), Path::new("/old/squish")).unwrap();
        install_into(tmp.path(), Path::new("/new/squish")).unwrap();

        let doc = std::fs::read_to_string(
            tmp.path().join("Squish.workflow/Contents/document.wflow"),
        )
        .unwrap();
        assert!(doc.contains("/new/squish"));
        assert!(!doc.contains("/old/squish"));
    }
}
