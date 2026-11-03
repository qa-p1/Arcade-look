//! Test-only native-webview driver. No listener exists in production builds.
//! The ecosystem runner supplies a private socket beneath its throwaway root.
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Manager};

fn control_path() -> Option<PathBuf> {
    if std::env::var("ARCADE_E2E_INNER").as_deref() != Ok("1") {
        return None;
    }
    let root = PathBuf::from(std::env::var("ARCADE_E2E_ROOT").ok()?);
    let path = PathBuf::from(std::env::var("ALOOK_E2E_CONTROL").ok()?);
    (path.parent() == Some(root.as_path()) && crate::util::debug_enabled()).then_some(path)
}

/// Shorten only the existing release delay inside the private native-test runner.
/// Production binaries do not compile this module or accept this setting.
pub fn idle_release_delay(default: Duration) -> Duration {
    control_path()
        .and_then(|_| {
            std::env::var("ALOOK_E2E_RELEASE_MILLIS")
                .ok()?
                .parse::<u64>()
                .ok()
        })
        .filter(|millis| (100..=30_000).contains(millis))
        .map_or(default, Duration::from_millis)
}

pub fn start(app: &AppHandle) {
    let Some(path) = control_path() else {
        return;
    };
    let app = app.clone();
    std::thread::spawn(move || {
        // An explicit Restart reuses this same private test root.
        let _ = std::fs::remove_file(&path);
        let Ok(listener) = UnixListener::bind(&path) else {
            return;
        };
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        for stream in listener.incoming().flatten() {
            evaluate(&app, stream);
        }
    });
}

fn evaluate(app: &AppHandle, stream: UnixStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut line = String::new();
    if BufReader::new(&stream)
        .take(1 << 20)
        .read_line(&mut line)
        .is_err()
    {
        return;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
        return;
    };
    if value.get("state").and_then(serde_json::Value::as_bool) == Some(true) {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let mut stream = stream;
            let state = serde_json::json!({
                "pid": std::process::id(),
                "windows": handle.webview_windows().keys().collect::<Vec<_>>(),
            });
            let _ = writeln!(stream, "{state}");
        });
        return;
    }
    let (label, script) = if let Some(script) = value.as_str() {
        (crate::app::MAIN, script.to_string())
    } else {
        let Some(label) = value.get("window").and_then(serde_json::Value::as_str) else {
            return;
        };
        if ![crate::app::MAIN, crate::app::SETTINGS].contains(&label) {
            return;
        }
        let Some(script) = value.get("script").and_then(serde_json::Value::as_str) else {
            return;
        };
        (label, script.to_string())
    };
    let label = label.to_string();
    let size = value
        .get("size")
        .and_then(|size| Some((size.get(0)?.as_f64()?, size.get(1)?.as_f64()?)));
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window(&label) {
            if let Some((width, height)) = size {
                if (720.0..=1920.0).contains(&width) && (520.0..=1080.0).contains(&height) {
                    let _ = window.set_size(tauri::LogicalSize::new(width, height));
                }
            }
            let _ = window.eval(&script);
        }
    });
}
