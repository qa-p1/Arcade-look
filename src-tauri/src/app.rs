//! App state and window lifecycle.
//!
//! The process stays resident after a preview is closed so the next one is instant. While
//! hidden, the webview is released after `idleMinutes`; the process then exits, unless an
//! integration (Explorer Space hook, global shortcut) needs it to keep listening.

use crate::config::{Config, WindowState};
use crate::plugins::Plugin;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// CLI, drag & drop, Open With, in-app navigation.
    Local,
    /// GNOME Files over the NautilusPreviewer D-Bus protocol.
    Nautilus,
    /// Windows Explorer Space hook or global shortcut.
    Explorer,
    /// macOS Finder via global shortcut.
    Finder,
}

pub struct AppState {
    pub pending: Mutex<Option<PathBuf>>,
    pub config: RwLock<Config>,
    pub plugins: RwLock<Vec<Plugin>>,
    pub window_state: Mutex<WindowState>,
    pub hidden_since: Mutex<Option<Instant>>,
    pub service: AtomicBool,
    pub frontend_ready: AtomicBool,
    pub visible: AtomicBool,
    pub source: Mutex<Source>,
    pub current: Mutex<Option<PathBuf>>,
    pub activation_token: Mutex<Option<String>>,
}

impl AppState {
    pub fn new(config: Config, plugins: Vec<Plugin>, service: bool) -> Self {
        Self {
            pending: Mutex::new(None),
            config: RwLock::new(config),
            plugins: RwLock::new(plugins),
            window_state: Mutex::new(crate::config::load_state()),
            hidden_since: Mutex::new(None),
            service: AtomicBool::new(service),
            frontend_ready: AtomicBool::new(false),
            visible: AtomicBool::new(false),
            source: Mutex::new(Source::Local),
            current: Mutex::new(None),
            activation_token: Mutex::new(None),
        }
    }

    pub fn config(&self) -> Config {
        self.config.read().map(|c| c.clone()).unwrap_or_default()
    }

    pub fn is_visible(&self) -> bool {
        self.visible.load(Ordering::SeqCst)
    }
}

pub const MAIN: &str = "main";

/// Ask the UI to preview `path` (or show the welcome screen for `None`).
pub fn open(app: &AppHandle, path: Option<PathBuf>, source: Source) {
    let state = app.state::<AppState>();
    if let Ok(mut s) = state.source.lock() {
        *s = source;
    }
    if let Ok(mut c) = state.current.lock() {
        c.clone_from(&path);
    }
    let ready = state.frontend_ready.load(Ordering::SeqCst);
    match app.get_webview_window(MAIN) {
        Some(_) if ready => {
            let payload = path.map(|p| p.to_string_lossy().to_string());
            let _ = app.emit_to(MAIN, "open", payload);
        }
        Some(_) => {
            if let Ok(mut p) = state.pending.lock() {
                *p = path;
            }
        }
        None => {
            if let Ok(mut p) = state.pending.lock() {
                *p = path;
            }
            if let Err(e) = build_window(app) {
                eprintln!("arcade-look: could not create window: {e}");
            }
        }
    }
}

pub fn build_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let state = app.state::<AppState>();
    state.frontend_ready.store(false, Ordering::SeqCst);
    let ws = state.window_state.lock().map(|s| *s).unwrap_or_default();
    let dark = !matches!(state.config().theme.as_str(), "light");
    let builder = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("index.html".into()))
        .title("Arcade Look")
        .inner_size(ws.width.clamp(420.0, 6000.0), ws.height.clamp(300.0, 4000.0))
        .min_inner_size(420.0, 300.0)
        .visible(false)
        .resizable(true)
        .focused(true)
        .center()
        .background_color(if dark {
            tauri::window::Color(24, 24, 27, 255)
        } else {
            tauri::window::Color(246, 246, 248, 255)
        });
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true);
    #[cfg(not(target_os = "macos"))]
    let builder = builder.decorations(false);
    builder.build()
}

/// Show and focus the window (called by the UI once the preview has painted).
pub fn show(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Some(w) = app.get_webview_window(MAIN) else { return };
    let was_visible = state.visible.swap(true, Ordering::SeqCst);
    if let Ok(mut h) = state.hidden_since.lock() {
        *h = None;
    }
    if !was_visible {
        center_on_cursor_monitor(app, &w);
        #[cfg(target_os = "linux")]
        if let Some(token) = state.activation_token.lock().ok().and_then(|mut t| t.take()) {
            crate::integration::linux::apply_activation_token(&w, &token);
        }
    }
    let _ = w.show();
    let _ = w.unminimize();
    let _ = w.set_focus();
    if !was_visible {
        crate::integration::visibility_changed(app, true);
    }
}

pub fn hide(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Some(w) = app.get_webview_window(MAIN) {
        if w.is_fullscreen().unwrap_or(false) {
            let _ = w.set_fullscreen(false);
        }
        remember_size(app, &w);
        let _ = w.hide();
        let _ = app.emit_to(MAIN, "hidden", ());
    }
    if let Ok(mut h) = state.hidden_since.lock() {
        *h = Some(Instant::now());
    }
    if state.visible.swap(false, Ordering::SeqCst) {
        crate::integration::visibility_changed(app, false);
    }
}

fn remember_size(app: &AppHandle, w: &WebviewWindow) {
    if w.is_maximized().unwrap_or(false) || w.is_fullscreen().unwrap_or(false) {
        return;
    }
    let (Ok(size), Ok(scale)) = (w.inner_size(), w.scale_factor()) else { return };
    let logical = size.to_logical::<f64>(scale);
    let state = app.state::<AppState>();
    if let Ok(mut s) = state.window_state.lock() {
        if (s.width - logical.width).abs() > 1.0 || (s.height - logical.height).abs() > 1.0 {
            s.width = logical.width;
            s.height = logical.height;
            crate::config::save_state(&s);
        }
    };
}

fn center_on_cursor_monitor(app: &AppHandle, w: &WebviewWindow) {
    let Ok(cursor) = app.cursor_position() else { return };
    let Ok(monitors) = w.available_monitors() else { return };
    let Some(m) = monitors.into_iter().find(|m| {
        let (p, s) = (m.position(), m.size());
        cursor.x >= p.x as f64
            && cursor.y >= p.y as f64
            && cursor.x < (p.x + s.width as i32) as f64
            && cursor.y < (p.y + s.height as i32) as f64
    }) else {
        return;
    };
    let Ok(size) = w.outer_size() else { return };
    let (mp, ms) = (m.position(), m.size());
    let x = mp.x + (ms.width as i32 - size.width as i32) / 2;
    let y = mp.y + (ms.height as i32 - size.height as i32) / 2;
    let _ = w.set_position(tauri::PhysicalPosition::new(x, y.max(mp.y)));
}

/// Whether the process must stay alive when idle (something is listening for Space).
pub fn keep_alive(app: &AppHandle) -> bool {
    app.state::<AppState>().service.load(Ordering::SeqCst) || crate::integration::has_listeners()
}

/// Release the webview (or exit) after the window has been hidden for a while.
pub fn start_idle_watcher(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(20));
        let state = app.state::<AppState>();
        let minutes = state.config().idle_minutes;
        if minutes == 0 {
            continue;
        }
        let idle = state
            .hidden_since
            .lock()
            .ok()
            .and_then(|h| *h)
            .is_some_and(|t| t.elapsed() > Duration::from_secs(minutes * 60));
        if !idle || state.is_visible() {
            continue;
        }
        if keep_alive(&app) {
            if let Some(w) = app.get_webview_window(MAIN) {
                state.frontend_ready.store(false, Ordering::SeqCst);
                let _ = w.destroy();
            }
            if let Ok(mut h) = state.hidden_since.lock() {
                *h = None;
            }
        } else {
            app.exit(0);
        }
    });
}

/// Act on command-line arguments, from our own launch or forwarded by a second instance.
pub fn handle_args(app: &AppHandle, args: &crate::cli::Args, cwd: Option<&std::path::Path>) {
    if args.quit {
        app.exit(0);
        return;
    }
    if args.service {
        app.state::<AppState>().service.store(true, Ordering::SeqCst);
    }
    let path = args.paths.iter().find_map(|p| crate::util::normalize_arg(p, cwd));
    match path {
        Some(p) => open(app, Some(p), Source::Local),
        None if args.service => {}
        None => open(app, None, Source::Local),
    }
}
