//! User configuration (`<config dir>/arcade/look/settings.json`) and persisted window state.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The newest file format this build reads and writes (D19).
pub const SCHEMA: u32 = 1;

fn is_v1(schema: &u32) -> bool {
    *schema <= 1
}

/// The `schema` a parsed file declares; no field (or a non-number) is 1, the released shape.
pub fn schema_of(value: &serde_json::Value) -> u32 {
    value
        .get("schema")
        .and_then(serde_json::Value::as_u64)
        .map_or(1, |n| u32::try_from(n).unwrap_or(u32::MAX))
}

/// Whether the file on disk was made by a newer Look. Such a file is never written over.
pub fn newer_on_disk(path: &std::path::Path) -> bool {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|value| schema_of(&value) > SCHEMA)
}

const NEWER: &str = "made by a newer Arcade Look";

/// Move a file Look cannot read to `<name>.unreadable-<unix seconds>` (with a counter if that
/// name is taken) so the next save cannot destroy it. Returns where it went.
pub fn set_aside(path: &std::path::Path) -> Option<PathBuf> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let name = path.file_name()?.to_string_lossy().into_owned();
    let mut target = path.with_file_name(format!("{name}.unreadable-{seconds}"));
    let mut n = 1;
    while target.exists() {
        target = path.with_file_name(format!("{name}.unreadable-{seconds}-{n}"));
        n += 1;
    }
    std::fs::rename(path, &target).ok()?;
    Some(target)
}

/// Read a JSON data file into `T`. A missing file is the defaults. A file this build cannot
/// parse is moved aside (and logged); one made by a newer Look is read as far as it can be,
/// logged, and left where it is. Look never starts with a silent default over someone's file.
pub fn load_json<T: serde::de::DeserializeOwned + Default>(path: &std::path::Path) -> T {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return T::default(),
        Err(e) => {
            eprintln!(
                "arcade-look: can't read {}: {e}; using defaults",
                path.display()
            );
            return T::default();
        }
    };
    let value: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(e) => return unreadable(path, &e.to_string()),
    };
    let schema = schema_of(&value);
    let newer = schema > SCHEMA;
    if newer {
        eprintln!(
            "arcade-look: {} was {NEWER} (schema {schema}, this build reads {SCHEMA}); reading what I can and leaving the file as it is",
            path.display()
        );
    }
    match serde_json::from_value(value) {
        Ok(parsed) => parsed,
        Err(e) if newer => {
            eprintln!(
                "arcade-look: couldn't read all of {}: {e}; using defaults without writing",
                path.display()
            );
            T::default()
        }
        Err(e) => unreadable(path, &e.to_string()),
    }
}

fn unreadable<T: Default>(path: &std::path::Path, why: &str) -> T {
    match set_aside(path) {
        Some(kept) => eprintln!(
            "arcade-look: can't read {} ({why}); kept it as {} and started with defaults",
            path.display(),
            kept.display()
        ),
        None => eprintln!(
            "arcade-look: can't read {} ({why}); using defaults",
            path.display()
        ),
    }
    T::default()
}

/// Top-level fields of `path` that `known` does not name, so a save keeps what a newer
/// (or a plugin-aware) Look wrote. Empty when the file is missing or not an object.
fn unknown_fields(
    path: &std::path::Path,
    known: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut map = std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| match value {
            serde_json::Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default();
    map.retain(|key, _| !known.contains_key(key) && key != "schema");
    map
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// File format version (FS-2, D19). Absent means 1, the shape every release so far wrote, and
    /// it is written only above 1: Look 0.4.x does not know the field and must read a v1 file as before.
    #[serde(skip_serializing_if = "is_v1")]
    pub schema: u32,
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
    /// Text previews read at most this many MiB (not in Settings; 1 by default:
    /// a 5 MB text or Markdown preview peaked near 900 MB, LK1).
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
            schema: SCHEMA,
            theme: "system".into(),
            idle_minutes: 10,
            keep_ready: true,
            notifications: false,
            global_shortcut: default_shortcut(std::env::consts::OS).into(),
            explorer_space: true,
            nautilus_previewer: true,
            text_limit_mb: 1,
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
    /// LK1: big text previews are capped at 1 MiB, also for 0.4 settings.
    #[test]
    fn the_text_limit_moves_from_the_old_default() {
        assert_eq!(Config::default().text_limit_mb, 1);
        let old: Config = serde_json::from_value(serde_json::json!({"textLimitMb": 4})).unwrap();
        assert_eq!(upgrade(old).text_limit_mb, 1);
        let chosen: Config = serde_json::from_value(serde_json::json!({"textLimitMb": 8})).unwrap();
        assert_eq!(upgrade(chosen).text_limit_mb, 8);
    }

    // FS-2 (D19): every data file carries a schema version, 0.4.3 files load with every value
    // kept, and an older Look never loses or corrupts a newer file.
    static HOME: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// A fresh, private config folder (every home variable) for one test; never the real user's.
    fn private_home() -> (std::sync::MutexGuard<'static, ()>, PathBuf) {
        let guard = HOME.lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!(
            "look-schema-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        // Every root Link's `app_dirs` can start from, on Linux, Windows and macOS.
        for key in [
            "HOME",
            "USERPROFILE",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            "APPDATA",
            "LOCALAPPDATA",
        ] {
            std::env::set_var(key, &root);
        }
        std::fs::create_dir_all(dir()).unwrap();
        assert!(
            dir().starts_with(&root),
            "tests must not touch the real config"
        );
        (guard, root)
    }

    const V043_SETTINGS: &str = include_str!("../tests/data/v0.4.3/settings.json");
    const V043_STATE: &str = include_str!("../tests/data/v0.4.3/state.json");

    fn aside(name: &str) -> Vec<PathBuf> {
        std::fs::read_dir(dir())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(&format!("{name}.unreadable-"))
            })
            .collect()
    }

    #[test]
    fn v043_settings_load_with_every_value_kept() {
        let (_guard, root) = private_home();
        std::fs::write(config_path(), V043_SETTINGS).unwrap();
        let c = load();
        assert_eq!(c.theme, "dark");
        assert_eq!(c.idle_minutes, 25);
        assert!(!c.keep_ready);
        assert!(c.notifications);
        assert_eq!(c.global_shortcut, "Ctrl+Alt+Space");
        assert!(!c.explorer_space);
        assert!(!c.nautilus_previewer);
        assert_eq!(c.highlight_limit_kb, 256);
        assert!(!c.autoplay);
        assert!(!c.plugins);
        assert!(c.show_hidden);
        assert!(!c.link_enabled);
        assert_eq!(c.link_disabled_peers, ["arcade.box", "arcade.tools"]);
        // 0.4.3's default text limit (4) is the upgrade() migration, not a choice.
        assert_eq!(c.text_limit_mb, 1);
        assert_eq!(
            std::fs::read_to_string(config_path()).unwrap(),
            V043_SETTINGS
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn upgrade_from_the_v043_file_keeps_a_chosen_limit() {
        let (_guard, root) = private_home();
        let chosen = V043_SETTINGS.replace("\"textLimitMb\": 4", "\"textLimitMb\": 8");
        assert_ne!(chosen, V043_SETTINGS);
        std::fs::write(config_path(), chosen).unwrap();
        assert_eq!(try_load().unwrap().text_limit_mb, 8);
        std::fs::write(config_path(), V043_SETTINGS).unwrap();
        assert_eq!(try_load().unwrap().text_limit_mb, 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_v1_settings_file_stays_in_the_released_shape() {
        let (_guard, root) = private_home();
        std::fs::write(config_path(), V043_SETTINGS).unwrap();
        let patch = serde_json::json!({"theme": "light"});
        update(&load(), patch.as_object().unwrap().clone()).unwrap();
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(config_path()).unwrap()).unwrap();
        assert!(
            saved.get("schema").is_none(),
            "0.4.x must read the file as it did"
        );
        assert_eq!(saved["theme"], "light");
        assert_eq!(saved["idleMinutes"], 25);
        let fresh = serde_json::to_value(Config::default()).unwrap();
        assert!(fresh.get("schema").is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_fields_default() {
        let (_guard, root) = private_home();
        std::fs::write(config_path(), "{}").unwrap();
        let d = Config::default();
        let c = load();
        assert_eq!((c.theme, c.idle_minutes), (d.theme.clone(), d.idle_minutes));
        std::fs::write(config_path(), r#"{"theme":"light"}"#).unwrap();
        let c = load();
        assert_eq!(c.theme, "light");
        assert_eq!(c.idle_minutes, d.idle_minutes);
        assert_eq!(c.link_disabled_peers, d.link_disabled_peers);
        std::fs::write(state_path(), "{}").unwrap();
        assert_eq!(load_state().width, WindowState::default().width);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unknown_settings_fields_survive_an_update() {
        let (_guard, root) = private_home();
        std::fs::write(
            config_path(),
            r#"{"theme":"dark","futureOption":{"nested":[1,2],"deep":{"x":true}}}"#,
        )
        .unwrap();
        let patch = serde_json::json!({"autoplay": false});
        update(&load(), patch.as_object().unwrap().clone()).unwrap();
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(config_path()).unwrap()).unwrap();
        assert_eq!(saved["futureOption"]["deep"]["x"], true);
        assert_eq!(saved["futureOption"]["nested"][1], 2);
        assert_eq!(saved["autoplay"], false);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn v043_window_state_loads_and_unknown_fields_survive_a_save() {
        let (_guard, root) = private_home();
        std::fs::write(state_path(), V043_STATE).unwrap();
        let mut s = load_state();
        assert_eq!((s.width, s.height), (1180.0, 800.0));
        assert!(s.info_panel && s.setup_done);
        // A v1 state with nothing unknown is saved exactly as 0.4.3 wrote it.
        save_state(&s);
        assert_eq!(std::fs::read_to_string(state_path()).unwrap(), V043_STATE);
        std::fs::write(
            state_path(),
            r#"{"width":1180.0,"height":800.0,"infoPanel":true,"setupDone":true,"futureZoom":1.5,"future":{"a":[1]}}"#,
        )
        .unwrap();
        s = load_state();
        s.width = 1300.0;
        save_state(&s);
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(state_path()).unwrap()).unwrap();
        assert_eq!(saved["width"], 1300.0);
        assert_eq!(saved["futureZoom"], 1.5);
        assert_eq!(saved["future"]["a"][0], 1);
        assert!(saved.get("schema").is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_newer_settings_file_is_read_and_never_written_over() {
        let (_guard, root) = private_home();
        let newer = r#"{"schema":2,"theme":"dark","idleMinutes":7,"shiny":{"a":1}}"#;
        std::fs::write(config_path(), newer).unwrap();
        let c = load();
        assert_eq!((c.theme.as_str(), c.idle_minutes, c.schema), ("dark", 7, 2));
        let patch = serde_json::json!({"theme": "light"});
        let error = update(&c, patch.as_object().unwrap().clone()).unwrap_err();
        assert!(error.contains("newer Arcade Look"), "{error}");
        assert_eq!(std::fs::read_to_string(config_path()).unwrap(), newer);
        assert!(aside("settings.json").is_empty());
        // Even when this build cannot read a field of it, it stays where it is.
        let odd = r#"{"schema":3,"theme":{"name":"dark"}}"#;
        std::fs::write(config_path(), odd).unwrap();
        assert_eq!(load().theme, "system");
        assert_eq!(std::fs::read_to_string(config_path()).unwrap(), odd);
        assert!(aside("settings.json").is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_newer_window_state_is_never_written_over() {
        let (_guard, root) = private_home();
        let newer = r#"{"schema":2,"width":900.0,"layout":"tabs"}"#;
        std::fs::write(state_path(), newer).unwrap();
        let mut s = load_state();
        assert_eq!(s.width, 900.0);
        s.width = 1500.0;
        save_state(&s);
        assert_eq!(std::fs::read_to_string(state_path()).unwrap(), newer);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_unreadable_settings_file_is_moved_aside_not_destroyed() {
        let (_guard, root) = private_home();
        for (n, bytes) in [&b"{not json"[..], br#"{"idleMinutes":"soon"}"#, b"[1,2]"]
            .into_iter()
            .enumerate()
        {
            std::fs::write(config_path(), bytes).unwrap();
            let c = load();
            assert_eq!(c.idle_minutes, Config::default().idle_minutes);
            let kept = aside("settings.json");
            assert_eq!(kept.len(), 1, "case {n}");
            assert_eq!(std::fs::read(&kept[0]).unwrap(), bytes);
            // The next save writes a good file next to it instead of over the bad one.
            let patch = serde_json::json!({"autoplay": false});
            update(&c, patch.as_object().unwrap().clone()).unwrap();
            assert!(try_load().is_ok());
            assert_eq!(std::fs::read(&kept[0]).unwrap(), bytes);
            std::fs::remove_file(&kept[0]).unwrap();
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_unreadable_window_state_is_moved_aside_not_destroyed() {
        let (_guard, root) = private_home();
        std::fs::write(state_path(), b"\x00garbage").unwrap();
        let s = load_state();
        assert_eq!(s.width, WindowState::default().width);
        let kept = aside("state.json");
        assert_eq!(kept.len(), 1);
        assert_eq!(std::fs::read(&kept[0]).unwrap(), b"\x00garbage");
        save_state(&s);
        assert_eq!(std::fs::read(&kept[0]).unwrap(), b"\x00garbage");
        std::fs::remove_dir_all(root).unwrap();
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

/// Load the config, writing a default on first run so it is discoverable. A file that can't be
/// read is kept as `settings.json.unreadable-<seconds>` and Look starts with defaults (D19).
pub fn load() -> Config {
    let p = config_path();
    let c = upgrade(load_json::<Config>(&p));
    if !p.exists() && std::fs::create_dir_all(dir()).is_ok() {
        let _ = std::fs::write(&p, serde_json::to_string_pretty(&c).unwrap_or_default());
        let _ = std::fs::create_dir_all(plugins_dir());
    }
    c
}

/// Read the config file, failing (instead of falling back to defaults) if it is unreadable.
pub fn try_load() -> Result<Config, String> {
    let s = std::fs::read_to_string(config_path()).map_err(|e| e.to_string())?;
    serde_json::from_str(&s)
        .map(upgrade)
        .map_err(|e| e.to_string())
}

/// Saved values that were only ever the old default move to the new one.
/// `textLimitMb` was never in Settings, so 4 is 0.4's default, not a choice (LK1).
fn upgrade(mut c: Config) -> Config {
    if c.text_limit_mb == 4 {
        c.text_limit_mb = 1;
    }
    c
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
    let existing = std::fs::read(&p)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    if existing.as_ref().is_some_and(|v| schema_of(v) > SCHEMA) {
        return Err(format!(
            "settings.json was {NEWER}; this version leaves it unchanged. Update Arcade Look to change settings."
        ));
    }
    let mut doc = match existing.filter(serde_json::Value::is_object) {
        Some(doc) => doc,
        None => {
            if p.exists() {
                // Not a settings object: keep it instead of replacing it.
                set_aside(&p);
            }
            serde_json::to_value(current).unwrap_or_default()
        }
    };
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
    /// File format version; see `Config::schema`. Written only above 1.
    #[serde(skip_serializing_if = "is_v1")]
    pub schema: u32,
    pub width: f64,
    pub height: f64,
    pub info_panel: bool,
    /// The first-launch setup (file manager integration, start on login) has been done.
    pub setup_done: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
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
    load_json(&state_path())
}

/// Save the window state. Fields this build doesn't know stay in the file, and a file made by a
/// newer Look is not written at all.
pub fn save_state(s: &WindowState) {
    let path = state_path();
    if newer_on_disk(&path) {
        return;
    }
    let _ = std::fs::create_dir_all(dir());
    let text = match serde_json::to_value(s) {
        Ok(serde_json::Value::Object(mut ours)) => {
            let extra = unknown_fields(&path, &ours);
            if extra.is_empty() {
                // Exactly the bytes 0.4.x wrote.
                serde_json::to_string(s)
            } else {
                ours.extend(extra);
                serde_json::to_string(&ours)
            }
        }
        _ => return,
    };
    if let Ok(text) = text {
        let _ = arcade_link::paths::write_atomic(&path, text.as_bytes(), true);
    }
}
