//! User configuration (`<config dir>/arcade/look/settings.json`) and persisted window state.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// "system" | "light" | "dark"
    pub theme: String,
    /// Minutes before a non-resident process exits (0 = never). Webviews release after 30 s.
    pub idle_minutes: u64,
    /// Keep the service resident after its windows close.
    pub keep_ready: bool,
    /// System notifications for completed actions while every window is hidden.
    pub notifications: bool,
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
    /// "Connect with other Arcade apps" (Settings → Connected apps).
    pub link_enabled: bool,
    /// Arcade apps (`arcade.box`, …) whose actions are hidden in Look.
    pub link_disabled_peers: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            idle_minutes: 10,
            keep_ready: true,
            notifications: false,
            global_shortcut: default_shortcut(std::env::consts::OS).into(),
            explorer_space: true,
            nautilus_previewer: true,
            text_limit_mb: 4,
            highlight_limit_kb: 512,
            autoplay: true,
            plugins: true,
            show_hidden: false,
            link_enabled: true,
            link_disabled_peers: Vec::new(),
        }
    }
}

fn default_shortcut(_platform: &str) -> &'static str {
    "Ctrl+Alt+Shift+Space"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_installs_use_the_standard_shortcut_and_saved_shortcuts_roundtrip() {
        assert_eq!(default_shortcut("windows"), "Ctrl+Alt+Shift+Space");
        assert_eq!(default_shortcut("linux"), "Ctrl+Alt+Shift+Space");
        assert_eq!(default_shortcut("macos"), "Ctrl+Alt+Shift+Space");
        for saved in ["Ctrl+Alt+Space", "Super+Shift+P", ""] {
            let config: Config =
                serde_json::from_value(serde_json::json!({"globalShortcut": saved})).unwrap();
            assert_eq!(config.global_shortcut, saved);
            let roundtrip: Config =
                serde_json::from_value(serde_json::to_value(config).unwrap()).unwrap();
            assert_eq!(roundtrip.global_shortcut, saved);
        }
    }
    #[test]
    fn migration_keeps_preferences_unknown_fields_and_plugin_files() {
        let root = std::env::temp_dir().join(format!(
            "look-migrate-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("plugins/custom")).unwrap();
        let bytes = br#"{"globalShortcut":"Super+Shift+P","theme":"dark","futureOption":17}"#;
        std::fs::write(root.join("config.json"), bytes).unwrap();
        std::fs::write(root.join("plugins/custom/plugin.json"), b"plugin-content").unwrap();
        migrate_settings_file(&root).unwrap();
        assert_eq!(std::fs::read(root.join("settings.json")).unwrap(), bytes);
        assert!(!root.join("config.json").exists());
        assert_eq!(
            std::fs::read(root.join("plugins/custom/plugin.json")).unwrap(),
            b"plugin-content"
        );
        migrate_settings_file(&root).unwrap();
        std::fs::write(root.join("config.json"), b"different").unwrap();
        assert!(migrate_settings_file(&root).is_err());
        assert_eq!(std::fs::read(root.join("settings.json")).unwrap(), bytes);
        assert_eq!(
            std::fs::read(root.join("config.json")).unwrap(),
            b"different"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

pub fn dir() -> PathBuf {
    arcade_link::paths::app_dirs(arcade_link::ids::LOOK).config
}

pub fn config_path() -> PathBuf {
    dir().join("settings.json")
}

/// Link copies and verifies every legacy folder before removing it. Refuse to
/// start on a conflict instead of silently replacing the user's preferences.
pub fn migrate() -> Result<(), String> {
    let report = arcade_link::paths::migrate_app_dirs(arcade_link::ids::LOOK);
    if !report.kept.is_empty() {
        return Err(report
            .kept
            .iter()
            .map(|(path, reason)| format!("{}: {reason}", path.display()))
            .collect::<Vec<_>>()
            .join("; "));
    }
    migrate_settings_file(&dir())
}

fn migrate_settings_file(directory: &std::path::Path) -> Result<(), String> {
    let old = directory.join("config.json");
    let new = directory.join("settings.json");
    if !old.exists() {
        return Ok(());
    }
    let bytes = std::fs::read(&old).map_err(|e| e.to_string())?;
    if new.exists() && std::fs::read(&new).map_err(|e| e.to_string())? != bytes {
        return Err(
            "Both old and new settings exist with different content; both have been kept.".into(),
        );
    }
    arcade_link::paths::write_atomic(&new, &bytes, true).map_err(|e| e.to_string())?;
    if std::fs::read(&new).map_err(|e| e.to_string())? != bytes {
        return Err("Settings verification failed; the old file has been kept.".into());
    }
    std::fs::remove_file(old).map_err(|e| e.to_string())
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
    if let Some(shortcut) = obj
        .get("globalShortcut")
        .and_then(serde_json::Value::as_str)
    {
        if shortcut.is_empty() {
            obj.insert(
                "globalShortcut".into(),
                serde_json::Value::String(String::new()),
            );
        } else {
            let canonical = arcade_link::accelerator::normalize(shortcut)
                .map_err(|e| format!("invalid global shortcut: {e}"))?;
            obj.insert(
                "globalShortcut".into(),
                serde_json::Value::String(canonical),
            );
        }
    }
    let config: Config = serde_json::from_value(doc.clone()).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    arcade_link::paths::write_atomic(&p, text.as_bytes(), true)
        .map_err(|e| format!("write {}: {e}", p.display()))?;
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
