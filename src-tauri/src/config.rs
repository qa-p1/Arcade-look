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
    match std::fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
            eprintln!("arcade-look: ignoring invalid {}: {e}", p.display());
            Config::default()
        }),
        Err(_) => {
            let c = Config::default();
            if std::fs::create_dir_all(dir()).is_ok() {
                let _ = std::fs::write(&p, serde_json::to_string_pretty(&c).unwrap_or_default());
                let _ = std::fs::create_dir_all(plugins_dir());
            }
            c
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowState {
    pub width: f64,
    pub height: f64,
    pub info_panel: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            width: 1040.0,
            height: 720.0,
            info_panel: false,
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
