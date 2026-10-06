//! Test-only native-webview driver. No listener exists in production builds.
//! The ecosystem runner supplies a private socket beneath its throwaway root.
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Manager};

pub fn start(app: &AppHandle) {
    if std::env::var("ARCADE_E2E_INNER").as_deref() != Ok("1") {
        return;
    }
    let (Ok(root), Ok(path)) = (
        std::env::var("ARCADE_E2E_ROOT"),
        std::env::var("ALOOK_E2E_CONTROL"),
    ) else {
        return;
    };
    let root = PathBuf::from(root);
    let path = PathBuf::from(path);
    if path.parent() != Some(root.as_path()) || !crate::util::debug_enabled() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
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
    if BufReader::new(stream)
        .take(1 << 20)
        .read_line(&mut line)
        .is_err()
    {
        return;
    }
    let Ok(script) = serde_json::from_str::<String>(&line) else {
        return;
    };
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window(crate::app::MAIN) {
            let _ = window.eval(&script);
        }
    });
}
