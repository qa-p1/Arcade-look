//! macOS: read Finder's selection for the global shortcut. (Finder's own Space key keeps
//! opening Apple's Quick Look; our shortcut, Ctrl+Alt+Space by default, opens Arcade Look.)

use crate::util::Res;
use std::path::PathBuf;
use std::process::Command;

const SCRIPT: &str = r#"tell application "Finder"
  set sel to selection
  if sel is {} then return ""
  set paths to ""
  repeat with entry in sel
    set paths to paths & (POSIX path of (entry as alias)) & (ASCII character 0)
  end repeat
  return paths
end tell"#;

pub fn finder_selection() -> Option<PathBuf> {
    finder_selections().into_iter().next()
}

pub fn finder_selections() -> Vec<PathBuf> {
    let out = Command::new("osascript").arg("-e").arg(SCRIPT).output();
    let Ok(out) = out else { return Vec::new() };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .collect()
}

pub fn status() -> Vec<(String, bool)> {
    vec![("Finder selection via shortcut".into(), true)]
}

pub fn install() -> Res<String> {
    let autostart = match set_autostart(true) {
        Ok(()) => " Arcade Look now starts in the background when you log in (change this in \
            Settings)."
            .to_string(),
        Err(e) => format!(" Start on login could not be turned on: {e}"),
    };
    Ok(format!(
        "macOS needs no setup: select a file in Finder and press the global shortcut \
        (Ctrl+Option+Space by default). The first time, allow Arcade Look to control Finder \
        when macOS asks. Change the shortcut in config.json (globalShortcut).{autostart}"
    ))
}

pub fn uninstall() -> Res<String> {
    set_autostart(false)?;
    Ok("Removed Arcade Look start on login.".into())
}

fn launch_agent() -> PathBuf {
    arcade_link::install::Environment::discover()
        .ok()
        .map(|e| e.home)
        .or_else(dirs::home_dir)
        .unwrap_or_default()
        .join("Library/LaunchAgents/app.arcadelook.plist")
}

pub fn autostart_enabled() -> bool {
    launch_agent().exists()
}

pub fn set_autostart(enabled: bool) -> Res<()> {
    let path = launch_agent();
    if !enabled {
        return super::set_login_entry(&arcade_link::receipt::Store::discover(), &path, None);
    }
    let exe = super::launcher_path();
    // Apps run from the disk image, or from Downloads before being moved (macOS then runs a
    // randomized "translocated" copy), live at paths that are gone after a restart.
    let path_str = exe.to_string_lossy();
    if path_str.starts_with("/Volumes/") || path_str.contains("/AppTranslocation/") {
        return Err(
            "Move Arcade Look to the Applications folder, open it from there, then turn on \
            Start on login."
                .into(),
        );
    }
    let exe = exe
        .to_string_lossy()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>app.arcadelook</string>
  <key>ProgramArguments</key>
  <array><string>{exe}</string><string>--service</string></array>
  <key>RunAtLoad</key><true/>
  <key>ProcessType</key><string>Interactive</string>
</dict>
</plist>
"#
    );
    super::set_login_entry(
        &arcade_link::receipt::Store::discover(),
        &path,
        Some(plist.as_bytes()),
    )
}
