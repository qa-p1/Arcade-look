//! Arcade Link: Look's presence among the other Arcade apps.
//!
//! The manifest and the endpoint are set up on a background thread after
//! Tauri's setup, so startup never waits. With "Connect with other Arcade
//! apps" off, the manifest has no actions and nothing listens.

use std::sync::{Arc, Mutex, OnceLock};

use arcade_link::server::{Handler, InvokeContext, Reply};
use arcade_link::{ids, Action, InvokeRequest, LinkError, Locations, Manifest, Presence};
use tauri::AppHandle;

use crate::config::Config;

/// The manifest for this configuration (also printed by `--arcade-manifest`).
pub fn manifest(config: &Config) -> Manifest {
    let mut m = Manifest::new(
        ids::LOOK,
        env!("CARGO_PKG_VERSION"),
        &arcade_link::manifest::current_executable(),
    );
    m.launch.background = vec!["--background".into()];
    if !config.global_shortcut.trim().is_empty() {
        m.shortcuts.push(arcade_link::manifest::Shortcut {
            id: "preview-selection".into(),
            accelerator: config.global_shortcut.clone(),
        });
    }
    m.settings.link_enabled = config.link_enabled;
    m.actions = actions();
    m
}

/// The actions Look exposes.
pub fn actions() -> Vec<Action> {
    Vec::new()
}

struct LookHandler {
    #[allow(dead_code)]
    app: Option<AppHandle>,
}

impl Handler for LookHandler {
    fn describe(&self) -> Vec<Action> {
        actions()
    }

    fn invoke(&self, request: InvokeRequest, _ctx: &InvokeContext) -> Result<Reply, LinkError> {
        Err(LinkError::unavailable(format!(
            "Arcade Look has no action {}",
            request.action
        )))
    }
}

static PRESENCE: OnceLock<Mutex<Option<Arc<Presence>>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Arc<Presence>>> {
    PRESENCE.get_or_init(|| Mutex::new(None))
}

/// Starts Look's presence on a background thread.
pub fn start(app: &AppHandle, config: &Config) {
    let m = manifest(config);
    let handler = Arc::new(LookHandler {
        app: Some(app.clone()),
    });
    let _ = std::thread::Builder::new()
        .name("look-link".into())
        .spawn(move || {
            let p = Presence::start(Locations::discover(), m, handler);
            if let Some(e) = p.last_error() {
                crate::dbg_log!("arcade link: {e}");
            }
            *slot().lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(p));
        });
}

/// Rewrites the manifest after a configuration change.
pub fn refresh(config: &Config) {
    let m = manifest(config);
    let p = slot().lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(p) = p {
        std::thread::spawn(move || p.update(m));
    }
}

/// Stops listening and removes the endpoint file (the manifest stays).
pub fn stop() {
    if let Some(p) = slot().lock().unwrap_or_else(|e| e.into_inner()).take() {
        p.stop();
    }
}
