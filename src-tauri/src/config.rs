//! User configuration (`<config dir>/arcade-look/config.json`) and persisted window state.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// "system" | "light" | "dark"
    pub theme: String,
    /// Minutes a hidden window stays warm before its webview is released (0 = never).
    pub idle_minutes: u64,
    /// Global shortcut that previews the file manager's selection. Empty disables it.
    pub global_shortcut: String,
    /// Windows: press Space in Explorer to preview (requires the background service).
    pub explorer_space: bool,
    /// Linux: answer GNOME Files' Space-bar previewer requests over D-Bus.
    pub nautilus_previewer: bool,
    /// Text previews read at most this many MiB.
    pub text_limit_mb: u64,
    /// Syntax highlighting is skipped above this many KiB.
    pub highlight_limit_kb: u64,
    /// Start video/audio automatically.
    pub autoplay: bool,
    /// Load plugins from the plugins folder.
    pub plugins: bool,
    /// Show hidden files when stepping through a folder with ←/→.
    pub show_hidden: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            idle_minutes: 10,
            global_shortcut: if cfg!(target_os = "linux") {
                String::new()
            } else {
                "Ctrl+Alt+Space".into()
            },
            explorer_space: true,
            nautilus_previewer: true,
            text_limit_mb: 4,
            highlight_limit_kb: 512,
            autoplay: true,
            plugins: true,
            show_hidden: false,
        }
    }
}

pub fn dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("arcade-look")
}

pub fn config_path() -> PathBuf {
    dir().join("config.json")
}

pub fn plugins_dir() -> PathBuf {
    dir().join("plugins")
}

/// Load the config, writing a commented default on first run so it is discoverable.
pub fn load() -> Config {
    let p = config_path();
    if !p.exists() {
        let c = Config::default();
        if std::fs::create_dir_all(dir()).is_ok() {
            let _ = std::fs::write(&p, serde_json::to_string_pretty(&c).unwrap_or_default());
            let _ = std::fs::create_dir_all(plugins_dir());
        }
        return c;
    }
    try_load().unwrap_or_else(|e| {
        eprintln!("arcade-look: ignoring invalid {}: {e}", p.display());
        Config::default()
    })
}

/// Read the config file, failing (instead of falling back to defaults) if it is unreadable.
pub fn try_load() -> Result<Config, String> {
    let s = std::fs::read_to_string(config_path()).map_err(|e| e.to_string())?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

/// Modification time of the config file, to notice edits made while we run.
pub fn stamp() -> Option<std::time::SystemTime> {
    std::fs::metadata(config_path())
        .and_then(|m| m.modified())
        .ok()
}

/// Apply `patch` (camelCase keys) to the config file and return the result. Keys we don't
/// know, e.g. from a newer version, are kept in the file.
pub fn update(
    current: &Config,
    patch: serde_json::Map<String, serde_json::Value>,
) -> Result<Config, String> {
    let p = config_path();
    let mut doc = std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| serde_json::to_value(current).unwrap_or_default());
    let obj = doc.as_object_mut().ok_or("invalid config")?;
    for (k, v) in patch {
        obj.insert(k, v);
    }
    let config: Config = serde_json::from_value(doc.clone()).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    std::fs::write(&p, text).map_err(|e| format!("write {}: {e}", p.display()))?;
    Ok(config)
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowState {
    pub width: f64,
    pub height: f64,
    pub info_panel: bool,
    /// The first-launch setup (file manager integration, start on login) has been done.
    pub setup_done: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            width: 1040.0,
            height: 720.0,
            info_panel: false,
            setup_done: false,
        }
    }
}

fn state_path() -> PathBuf {
    dir().join("state.json")
}

pub fn load_state() -> WindowState {
    std::fs::read_to_string(state_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_state(s: &WindowState) {
    let _ = std::fs::create_dir_all(dir());
    let _ = std::fs::write(state_path(), serde_json::to_string(s).unwrap_or_default());
}
