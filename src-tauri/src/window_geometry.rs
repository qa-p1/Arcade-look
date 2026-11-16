//! Window geometry per surface and monitor; separate from the user's preferences.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tauri::{AppHandle, Manager, WebviewWindow};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
type Windows = BTreeMap<String, BTreeMap<String, Rect>>;
type Document = serde_json::Map<String, serde_json::Value>;

fn path() -> std::path::PathBuf {
    crate::config::dir().join("window-geometry.json")
}
/// The saved rectangles. The file has no `schema` while it is v1 (0.4.x reads it strictly);
/// a file that can't be read is set aside and one from a newer Look is read as far as it parses.
fn read() -> Windows {
    read_file(&path())
}
fn read_file(file: &std::path::Path) -> Windows {
    windows_in(&crate::config::load_json::<Document>(file))
}
fn windows_in(document: &Document) -> Windows {
    document
        .iter()
        .filter(|(label, _)| label.as_str() != "schema")
        .filter_map(|(label, monitors)| {
            let monitors = monitors.as_object()?;
            let rects = monitors
                .iter()
                .filter_map(|(key, rect)| {
                    Some((key.clone(), serde_json::from_value(rect.clone()).ok()?))
                })
                .collect();
            Some((label.clone(), rects))
        })
        .collect()
}
/// Save one rectangle in the file at `file`, keeping every other entry and field it holds
/// (including ones a newer Look wrote), and never writing over a newer file.
fn save(file: &std::path::Path, label: &str, monitor: &str, rect: Rect) -> std::io::Result<()> {
    if crate::config::newer_on_disk(file) {
        return Ok(());
    }
    let mut document = crate::config::load_json::<Document>(file);
    let entry = document
        .entry(label.to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !entry.is_object() {
        *entry = serde_json::json!({});
    }
    let fresh = serde_json::to_value(rect).map_err(std::io::Error::other)?;
    let monitors = entry
        .as_object_mut()
        .ok_or_else(|| std::io::Error::other("not an object"))?;
    let slot = monitors
        .entry(monitor.to_string())
        .or_insert_with(|| serde_json::json!({}));
    match (slot.as_object_mut(), fresh.as_object()) {
        (Some(old), Some(new)) => old.extend(new.clone()),
        _ => *slot = fresh,
    }
    let bytes = serde_json::to_vec_pretty(&document).map_err(std::io::Error::other)?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    arcade_link::paths::write_atomic(file, &bytes, true)
}
fn key(monitor: &tauri::Monitor) -> String {
    format!(
        "{}:{}:{}",
        monitor.name().map(String::as_str).unwrap_or("monitor"),
        monitor.size().width,
        monitor.size().height
    )
}
fn fit(mut rect: Rect, width: f64, height: f64) -> Rect {
    if !rect.width.is_finite()
        || !rect.height.is_finite()
        || !rect.x.is_finite()
        || !rect.y.is_finite()
    {
        rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 760.0,
            height: 560.0,
        };
    }
    rect.width = rect.width.max(720.0).min(width.max(720.0));
    rect.height = rect.height.max(520.0).min(height.max(520.0));
    rect.x = rect.x.max(0.0).min((width - rect.width).max(0.0));
    rect.y = rect.y.max(0.0).min((height - rect.height).max(0.0));
    rect
}

pub fn place(app: &AppHandle, window: &WebviewWindow) {
    let monitors = window.available_monitors().unwrap_or_default();
    let cursor = app.cursor_position().ok();
    let monitor = cursor
        .and_then(|point| {
            monitors.iter().find(|monitor| {
                let (p, s) = (monitor.position(), monitor.size());
                point.x >= p.x as f64
                    && point.y >= p.y as f64
                    && point.x < (p.x as f64 + s.width as f64)
                    && point.y < (p.y as f64 + s.height as f64)
            })
        })
        .cloned()
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let scale = monitor.scale_factor();
    let width = monitor.size().width as f64 / scale;
    let height = monitor.size().height as f64 / scale;
    let defaults = app
        .state::<crate::app::AppState>()
        .window_state
        .lock()
        .map(|state| *state)
        .unwrap_or_default();
    // Settings opens wider than the kit's 760 px sidebar breakpoint (LK-9).
    let (w, h) = if window.label() == crate::app::SETTINGS {
        (crate::app::SETTINGS_SIZE.0, crate::app::SETTINGS_SIZE.1)
    } else {
        (defaults.width, defaults.height)
    };
    let saved = read()
        .get(window.label())
        .and_then(|entries| entries.get(&key(&monitor)))
        .copied();
    let rect = fit(
        saved.unwrap_or(Rect {
            x: (width - w) / 2.0,
            y: (height - h) / 2.0,
            width: w,
            height: h,
        }),
        width,
        height,
    );
    let _ = window.set_size(tauri::LogicalSize::new(rect.width, rect.height));
    let _ = window.set_position(tauri::PhysicalPosition::new(
        monitor.position().x + (rect.x * scale) as i32,
        monitor.position().y + (rect.y * scale) as i32,
    ));
}

pub fn remember(window: &WebviewWindow) {
    if window.is_fullscreen().unwrap_or(false) || window.is_maximized().unwrap_or(false) {
        return;
    }
    let (Ok(Some(monitor)), Ok(position), Ok(size), Ok(scale)) = (
        window.current_monitor(),
        window.outer_position(),
        window.inner_size(),
        window.scale_factor(),
    ) else {
        return;
    };
    let rect = Rect {
        x: (position.x - monitor.position().x) as f64 / scale,
        y: (position.y - monitor.position().y) as f64 / scale,
        width: size.width as f64 / scale,
        height: size.height as f64 / scale,
    };
    if let Err(error) = save(&path(), window.label(), &key(&monitor), rect) {
        eprintln!("arcade-look: couldn't remember window geometry: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "look-geometry-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("window-geometry.json")
    }
    const V043: &str = include_str!("../tests/data/v0.4.3/window-geometry.json");
    const RECT: Rect = Rect {
        x: 1.0,
        y: 2.0,
        width: 800.0,
        height: 600.0,
    };

    #[test]
    fn a_v043_geometry_file_loads_with_every_rectangle() {
        let file = temp_file();
        std::fs::write(&file, V043).unwrap();
        let windows = windows_in(&crate::config::load_json::<Document>(&file));
        assert_eq!(windows["main"].len(), 2);
        assert_eq!(windows["main"]["eDP-1:1920:1080"].x, 10.5);
        assert_eq!(windows["settings"]["DP-1:2560:1440"].height, 640.0);
        // Saving another window keeps those, and writes no `schema` (0.4.x reads it strictly).
        save(&file, "viewer", "DP-1:2560:1440", RECT).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert!(saved.get("schema").is_none());
        assert_eq!(saved["main"]["DP-1:2560:1440"]["width"], 1100.0);
        assert_eq!(saved["viewer"]["DP-1:2560:1440"]["width"], 800.0);
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn unknown_geometry_fields_survive_a_save() {
        let file = temp_file();
        std::fs::write(
            &file,
            r#"{"main":{"m":{"x":0.0,"y":0.0,"width":900.0,"height":700.0,"maximized":true}},"notes":{"a":1}}"#,
        )
        .unwrap();
        save(&file, "main", "m", RECT).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(saved["main"]["m"]["maximized"], true);
        assert_eq!(saved["main"]["m"]["width"], 800.0);
        assert_eq!(saved["notes"]["a"], 1);
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_newer_geometry_file_is_read_and_never_written_over() {
        let file = temp_file();
        let newer = r#"{"schema":2,"main":{"m":{"x":3.0,"y":4.0,"width":850.0,"height":650.0}}}"#;
        std::fs::write(&file, newer).unwrap();
        let windows = windows_in(&crate::config::load_json::<Document>(&file));
        assert_eq!(windows["main"]["m"].width, 850.0);
        assert!(!windows.contains_key("schema"));
        save(&file, "main", "m", RECT).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), newer);
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn an_unreadable_geometry_file_is_moved_aside_not_destroyed() {
        let file = temp_file();
        std::fs::write(&file, b"{broken").unwrap();
        assert!(read_file(&file).is_empty());
        save(&file, "main", "m", RECT).unwrap();
        let kept: Vec<_> = std::fs::read_dir(file.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.to_string_lossy().contains(".unreadable-"))
            .collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(std::fs::read(&kept[0]).unwrap(), b"{broken");
        assert!(file.exists());
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }
    #[test]
    fn changed_monitor_never_leaves_a_window_offscreen_or_below_minimum() {
        let rect = fit(
            Rect {
                x: 4000.0,
                y: -200.0,
                width: 5000.0,
                height: 200.0,
            },
            1280.0,
            720.0,
        );
        assert_eq!(
            (rect.x, rect.y, rect.width, rect.height),
            (0.0, 0.0, 1280.0, 520.0)
        );
        let rect = fit(
            Rect {
                x: f64::NAN,
                y: 0.0,
                width: 5.0,
                height: 4.0,
            },
            1920.0,
            1080.0,
        );
        assert_eq!((rect.width, rect.height), (760.0, 560.0));
    }
}
