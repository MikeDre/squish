//! macOS Finder Quick Action: generate ~/Library/Services/Squish.workflow.

use std::path::Path;

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
}
