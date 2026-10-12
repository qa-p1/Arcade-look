//! Explicit portable settings allowlist for Arcade Link handoff.
//!
//! Window state, plugins, integration choices, peer preferences, caches and
//! all runtime data are intentionally excluded.

use arcade_link::{Content, Handoff, LinkError, Locations};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortableSettings {
    pub schema: u32,
    pub app: String,
    pub version: String,
    pub theme: String,
    pub idle_minutes: u64,
    pub global_shortcut: String,
    pub text_limit_mb: u64,
    pub highlight_limit_kb: u64,
    pub autoplay: bool,
    pub show_hidden: bool,
}

fn safe_text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}

impl PortableSettings {
    pub fn from_config(config: &crate::config::Config) -> Self {
        Self {
            schema: 1,
            app: arcade_link::ids::LOOK.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            theme: config.theme.clone(),
            idle_minutes: config.idle_minutes,
            global_shortcut: if config.global_shortcut.is_empty() {
                String::new()
            } else {
                arcade_link::accelerator::normalize(&config.global_shortcut)
                    .unwrap_or_else(|_| config.global_shortcut.clone())
            },
            text_limit_mb: config.text_limit_mb,
            highlight_limit_kb: config.highlight_limit_kb,
            autoplay: config.autoplay,
            show_hidden: config.show_hidden,
        }
    }

    pub fn validate(&self) -> Result<(), LinkError> {
        if self.schema != 1
            || self.app != arcade_link::ids::LOOK
            || !safe_text(&self.version)
            || !["system", "light", "dark"].contains(&self.theme.as_str())
            || self.idle_minutes > 1440
            || !(1..=1024).contains(&self.text_limit_mb)
            || !(16..=65_536).contains(&self.highlight_limit_kb)
        {
            return Err(LinkError::denied("invalid Arcade Look settings document"));
        }
        if !self.global_shortcut.is_empty() {
            let canonical = arcade_link::accelerator::normalize(&self.global_shortcut)
                .map_err(|e| LinkError::denied(&e.to_string()))?;
            if canonical != self.global_shortcut {
                return Err(LinkError::denied("global shortcut is not canonical"));
            }
        }
        Ok(())
    }

    pub fn patch(&self) -> Map<String, Value> {
        Map::from_iter([
            ("theme".into(), Value::String(self.theme.clone())),
            ("idleMinutes".into(), Value::from(self.idle_minutes)),
            (
                "globalShortcut".into(),
                Value::String(self.global_shortcut.clone()),
            ),
            ("textLimitMb".into(), Value::from(self.text_limit_mb)),
            (
                "highlightLimitKb".into(),
                Value::from(self.highlight_limit_kb),
            ),
            ("autoplay".into(), Value::Bool(self.autoplay)),
            ("showHidden".into(), Value::Bool(self.show_hidden)),
        ])
    }
}

pub fn export(config: &crate::config::Config) -> Result<Vec<Content>, LinkError> {
    let document = PortableSettings::from_config(config);
    document.validate()?;
    let bytes =
        serde_json::to_vec_pretty(&document).map_err(|e| LinkError::internal(e.to_string()))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(LinkError::denied("settings file exceeds 1 MiB"));
    }
    let locations = Locations::discover();
    let handoff = Handoff::create(&locations, arcade_link::ids::LOOK)
        .map_err(|e| LinkError::unavailable(e.to_string()))?;
    let mut content = handoff
        .file("arcade-look-settings.json", &bytes)
        .map_err(|e| LinkError::unavailable(e.to_string()))?;
    content.kind = "file/any".into();
    handoff.keep();
    Ok(vec![content])
}

pub fn read_import(inputs: &[Content]) -> Result<PortableSettings, LinkError> {
    let [input] = inputs else {
        return Err(LinkError::denied("import requires one settings file"));
    };
    if input.kind != "file/any" || !input.paths.is_empty() {
        return Err(LinkError::denied("import requires a file/any path"));
    }
    let path = input
        .path
        .as_deref()
        .map(Path::new)
        .filter(|p| p.is_absolute())
        .ok_or_else(|| LinkError::denied("import requires an absolute path"))?;
    let file = fs::File::open(path).map_err(io_error)?;
    if !file.metadata().map_err(io_error)?.is_file() {
        return Err(LinkError::denied("import requires a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(LinkError::denied("settings file exceeds 1 MiB"));
    }
    let document: PortableSettings = serde_json::from_slice(&bytes)
        .map_err(|e| LinkError::denied(&format!("invalid settings: {e}")))?;
    document.validate()?;
    Ok(document)
}

/// Keep a private copy of the raw settings file before applying an import.
pub fn backup_current() -> Result<Option<PathBuf>, LinkError> {
    let source = crate::config::config_path();
    let bytes = match fs::read(&source) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(error)),
    };
    let backup_dir = crate::config::dir().join("backups");
    arcade_link::paths::ensure_private_dir(&backup_dir).map_err(io_error)?;
    let backup = backup_dir.join(format!(
        "settings-{}.json",
        arcade_link::endpoint::new_token().map_err(io_error)?
    ));
    arcade_link::paths::write_atomic(&backup, &bytes, true).map_err(io_error)?;
    Ok(Some(backup))
}

pub fn restore_raw(path: &Path, bytes: Option<&[u8]>) {
    if let Some(bytes) = bytes {
        let _ = arcade_link::paths::write_atomic(path, bytes, true);
    } else {
        let _ = fs::remove_file(path);
    }
}

fn io_error(error: std::io::Error) -> LinkError {
    LinkError::unavailable(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_allowlist_excludes_local_and_device_settings() {
        let config = crate::config::Config::default();
        let document = PortableSettings::from_config(&config);
        let value = serde_json::to_value(&document).unwrap();
        assert!(value.get("plugins").is_none());
        assert!(value.get("linkEnabled").is_none());
        assert!(value.get("linkDisabledPeers").is_none());
        assert!(value.get("explorerSpace").is_none());
        document.validate().unwrap();
    }

    #[test]
    fn import_rejects_unknown_fields_and_noncanonical_shortcuts() {
        let mut value = serde_json::to_value(PortableSettings::from_config(
            &crate::config::Config::default(),
        ))
        .unwrap();
        value["history"] = Value::Array(Vec::new());
        assert!(serde_json::from_value::<PortableSettings>(value).is_err());

        let mut document = PortableSettings::from_config(&crate::config::Config::default());
        document.global_shortcut = "Ctrl+Shift+Alt+K".into();
        assert!(document.validate().is_err());
    }
}
