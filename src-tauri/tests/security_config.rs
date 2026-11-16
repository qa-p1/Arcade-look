//! What the webview may load and call, pinned so that widening either is a deliberate change (SECURITY.md,
//! "Windows"). The policy lives in `tauri.conf.json` (Content-Security-Policy) and
//! `capabilities/default.json` (Tauri's own permissions for the preview and settings windows).

use serde_json::Value;
use std::collections::BTreeMap;

fn json(file: &str) -> Value {
    let path = format!("{}/{file}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn csp() -> BTreeMap<String, Vec<String>> {
    json("tauri.conf.json")["app"]["security"]["csp"]
        .as_object()
        .expect("the CSP is written as directives")
        .iter()
        .map(|(name, sources)| {
            let sources = sources.as_str().unwrap();
            (
                name.clone(),
                sources.split_whitespace().map(str::to_string).collect(),
            )
        })
        .collect()
}

fn sources<'a>(csp: &'a BTreeMap<String, Vec<String>>, directive: &str) -> Vec<&'a str> {
    csp.get(directive)
        .unwrap_or_else(|| panic!("the CSP has no {directive}"))
        .iter()
        .map(String::as_str)
        .collect()
}

#[test]
fn csp_runs_only_looks_own_scripts_and_no_plugins_objects() {
    let csp = csp();
    assert_eq!(sources(&csp, "default-src"), ["'self'"]);
    assert_eq!(sources(&csp, "object-src"), ["'none'"]);
    // Scripts: the bundle, and script plugins served by the alook:// protocol. Never inline, eval or a remote host.
    assert_eq!(
        sources(&csp, "script-src"),
        [
            "'self'",
            "alook:",
            "http://alook.localhost",
            "'wasm-unsafe-eval'"
        ]
    );
    // Workers, frames and the base URL: Look's own origin, blobs and data made in the window, nothing remote.
    assert_eq!(sources(&csp, "worker-src"), ["'self'", "blob:"]);
    assert_eq!(sources(&csp, "frame-src"), ["'self'", "blob:", "data:"]);
    assert_eq!(
        sources(&csp, "base-uri"),
        ["'self'", "alook:", "http://alook.localhost"]
    );
}

#[test]
fn only_images_may_come_from_the_web() {
    let csp = csp();
    // Connections: Tauri's IPC, the alook:// protocol and the loopback media server, nothing remote.
    assert_eq!(
        sources(&csp, "connect-src"),
        [
            "'self'",
            "ipc:",
            "http://ipc.localhost",
            "alook:",
            "http://alook.localhost",
            "http://127.0.0.1:*"
        ]
    );
    // https images a previewed document links to are fetched (SECURITY.md says so); http ones and every other remote
    // kind of content are not.
    assert_eq!(
        sources(&csp, "img-src"),
        [
            "'self'",
            "alook:",
            "http://alook.localhost",
            "data:",
            "blob:",
            "https:"
        ]
    );
    for (directive, list) in &csp {
        if directive == "img-src" {
            continue;
        }
        for source in list {
            assert!(
                !(source == "https:"
                    || source == "http:"
                    || source == "*"
                    || source.contains("://")
                        && !source.contains("localhost")
                        && !source.contains("127.0.0.1")),
                "{directive} allows {source}"
            );
        }
    }
}

#[test]
fn the_webview_gets_only_core_dialog_notification_and_window_permissions() {
    let config = json("tauri.conf.json");
    assert_eq!(config["app"]["withGlobalTauri"], false);
    let capability = json("capabilities/default.json");
    assert_eq!(
        capability["windows"],
        serde_json::json!(["main", "settings"])
    );
    let granted: Vec<&str> = capability["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    assert_eq!(
        granted,
        [
            "core:default",
            "dialog:allow-open",
            "notification:default",
            "core:window:allow-close",
            "core:window:allow-start-dragging",
            "core:window:allow-minimize",
            "core:window:allow-toggle-maximize",
            "core:window:allow-is-maximized",
            "core:window:allow-set-fullscreen",
            "core:window:allow-is-fullscreen",
            "core:window:allow-hide",
            "core:window:allow-show",
            "core:window:allow-set-focus",
            "core:window:allow-set-title",
            "core:webview:allow-internal-toggle-devtools",
        ]
    );
    // The fs, shell, HTTP, process and updater plugins are not dependencies at all. The opener plugin is, but only
    // Look's own commands call it: no `opener:` permission is granted to the webview.
    let manifest =
        std::fs::read_to_string(format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR"))).unwrap();
    for plugin in [
        "tauri-plugin-fs",
        "tauri-plugin-shell",
        "tauri-plugin-http",
        "tauri-plugin-process",
        "tauri-plugin-updater",
    ] {
        assert!(
            !manifest.contains(&format!("{plugin} =")),
            "{plugin} is a dependency"
        );
    }
}

#[test]
fn looks_own_code_has_no_http_or_tls_client() {
    let manifest =
        std::fs::read_to_string(format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let dependencies: Vec<&str> = manifest
        .lines()
        .filter_map(|line| line.split_once('=').map(|(name, _)| name.trim()))
        .collect();
    for client in [
        "reqwest",
        "hyper",
        "ureq",
        "curl",
        "isahc",
        "surf",
        "attohttpc",
        "rustls",
        "native-tls",
        "openssl",
    ] {
        assert!(!dependencies.contains(&client), "{client} is a dependency");
    }
}
