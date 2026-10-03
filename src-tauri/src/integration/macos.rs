//! macOS: read Finder's selection for the global shortcut. (Finder's own Space key keeps
//! opening Apple's Quick Look; our shortcut, Ctrl+Alt+Space by default, opens Arcade Look.)

use crate::util::Res;
use std::path::PathBuf;
use std::process::Command;

const SCRIPT: &str = r#"tell application "Finder"
  set sel to selection
  if sel is {} then return ""
  return POSIX path of (item 1 of sel as alias)
end tell"#;

pub fn finder_selection() -> Option<PathBuf> {
    let out = Command::new("osascript")
        .arg("-e")
        .arg(SCRIPT)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then(|| PathBuf::from(s))
}

pub fn status() -> Vec<(String, bool)> {
    vec![("Finder selection via shortcut".into(), true)]
}

pub fn install() -> Res<String> {
    Ok(
        "macOS needs no setup: select a file in Finder and press the global shortcut \
        (Ctrl+Option+Space by default). The first time, allow Arcade Look to control Finder \
        when macOS asks. Change the shortcut in config.json (globalShortcut)."
            .into(),
    )
}
