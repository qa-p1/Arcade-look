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

fn path() -> std::path::PathBuf {
    crate::config::dir().join("window-geometry.json")
}
fn read() -> Windows {
    std::fs::read(path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
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
    let (w, h) = if window.label() == crate::app::SETTINGS {
        (760.0, 560.0)
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
    let mut windows = read();
    windows
        .entry(window.label().into())
        .or_default()
        .insert(key(&monitor), rect);
    if let Ok(bytes) = serde_json::to_vec_pretty(&windows) {
        let _ = std::fs::create_dir_all(crate::config::dir());
        if let Err(error) = arcade_link::paths::write_atomic(&path(), &bytes, true) {
            eprintln!("arcade-look: couldn't remember window geometry: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
