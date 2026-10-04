//! App state and window lifecycle.
//!
//! The process stays resident after a preview is closed so the next one is instant. While
//! hidden, the webview is released after `idleMinutes`; the process then exits, unless an
//! integration (Explorer Space hook, global shortcut) needs it to keep listening, or the tray
//! icon is showing.

use crate::config::{Config, WindowState};
use crate::plugins::Plugin;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime};
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

/// What the UI should show once it has loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Welcome,
    Settings,
    File(PathBuf),
}

pub struct AppState {
    pub pending: Mutex<Option<Request>>,
    pub config: RwLock<Config>,
    /// Modification time of config.json when it was last read or written.
    pub config_stamp: Mutex<Option<SystemTime>>,
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
            config_stamp: Mutex::new(crate::config::stamp()),
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
    crate::dbg_log!("open {path:?} from {source:?}");
    let state = app.state::<AppState>();
    if let Ok(mut s) = state.source.lock() {
        *s = source;
    }
    if let Ok(mut c) = state.current.lock() {
        c.clone_from(&path);
    }
    let payload = path.as_ref().map(|p| p.to_string_lossy().to_string());
    let request = path.map_or(Request::Welcome, Request::File);
    deliver(app, request, "open", payload);
}

/// Show the settings screen (tray icon, `--settings`, welcome screen link).
pub fn open_settings(app: &AppHandle) {
    crate::dbg_log!("open settings");
    let state = app.state::<AppState>();
    if let Ok(mut s) = state.source.lock() {
        *s = Source::Local;
    }
    if let Ok(mut c) = state.current.lock() {
        *c = None;
    }
    deliver(app, Request::Settings, "settings", None);
}

/// Bring the window forward if it is showing; otherwise open the welcome screen.
pub fn open_main(app: &AppHandle) {
    if app.state::<AppState>().is_visible() {
        show(app);
    } else {
        open(app, None, Source::Local);
    }
}

/// Send `request` to a loaded UI, or keep it until the UI asks for it (`bootstrap`).
fn deliver(app: &AppHandle, request: Request, event: &str, payload: Option<String>) {
    refresh_config(app);
    let state = app.state::<AppState>();
    let ready = state.frontend_ready.load(Ordering::SeqCst);
    match app.get_webview_window(MAIN) {
        Some(_) if ready => {
            if let Ok(mut p) = state.pending.lock() {
                *p = None;
            }
            let _ = app.emit_to(MAIN, event, payload);
        }
        window => {
            if let Ok(mut p) = state.pending.lock() {
                *p = Some(request);
            }
            if window.is_none() {
                if let Err(e) = build_window(app) {
                    eprintln!("arcade-look: could not create window: {e}");
                }
            }
        }
    }
}

/// Pick up edits made to config.json while we were running (it is otherwise only read at
/// startup, and the background process may live for days).
fn refresh_config(app: &AppHandle) {
    let state = app.state::<AppState>();
    let stamp = crate::config::stamp();
    {
        let Ok(mut last) = state.config_stamp.lock() else {
            return;
        };
        if *last == stamp {
            return;
        }
        *last = stamp;
    }
    // Keep the current settings while the file is half-edited or invalid.
    let Ok(config) = crate::config::try_load() else {
        return;
    };
    crate::dbg_log!("config.json changed, reloaded");
    if let Ok(mut c) = state.config.write() {
        *c = config.clone();
    }
    crate::link::refresh(&config);
    if state.frontend_ready.load(Ordering::SeqCst) {
        let _ = app.emit_to(MAIN, "config", config);
    }
}

pub fn build_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    crate::dbg_log!("creating window");
    let state = app.state::<AppState>();
    state.frontend_ready.store(false, Ordering::SeqCst);
    let ws = state.window_state.lock().map(|s| *s).unwrap_or_default();
    let dark = !matches!(state.config().theme.as_str(), "light");
    let builder = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("index.html".into()))
        .title("Arcade Look")
        .inner_size(
            ws.width.clamp(420.0, 6000.0),
            ws.height.clamp(300.0, 4000.0),
        )
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
    crate::dbg_log!("show");
    let state = app.state::<AppState>();
    let Some(w) = app.get_webview_window(MAIN) else {
        return;
    };
    let was_visible = state.visible.swap(true, Ordering::SeqCst);
    if let Ok(mut h) = state.hidden_since.lock() {
        *h = None;
    }
    if !was_visible {
        let (_width, _height) = place_on_cursor_monitor(app, &w);
        #[cfg(target_os = "linux")]
        if let Some(token) = state
            .activation_token
            .lock()
            .ok()
            .and_then(|mut t| t.take())
        {
            crate::integration::linux::apply_activation_token(&w, &token);
        }
        #[cfg(target_os = "linux")]
        crate::integration::linux::pin_size_for_map(&w, _width, _height);
    }
    let _ = w.show();
    let _ = w.unminimize();
    focus(&w);
    if !was_visible {
        // Showing is asynchronous and a focus request for a window that isn't mapped yet is
        // dropped, so ask again once it is on screen. Without focus, Space/Esc/arrows would
        // keep going to the file manager.
        let handle = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            let app = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                if !app.state::<AppState>().is_visible() {
                    return;
                }
                if let Some(w) = app.get_webview_window(MAIN) {
                    if !w.is_focused().unwrap_or(true) {
                        crate::dbg_log!("window not focused after showing, asking again");
                        focus(&w);
                    }
                }
            });
        });
        crate::integration::visibility_changed(app, true);
    }
}

/// Focus the window and the web view inside it. Keys only reach the page while the web view
/// itself holds keyboard focus, which raising the window alone doesn't guarantee (WebKitGTK
/// loses it across hide/show; WebView2 after the window is re-activated).
pub fn focus(w: &WebviewWindow) {
    let _ = w.set_focus();
    focus_webview(w);
}

pub fn focus_webview(w: &WebviewWindow) {
    let webview: &tauri::Webview = w.as_ref();
    let _ = webview.set_focus();
}

pub fn hide(app: &AppHandle) {
    crate::dbg_log!("hide");
    let state = app.state::<AppState>();
    if let Some(w) = app.get_webview_window(MAIN) {
        if w.is_fullscreen().unwrap_or(false) {
            let _ = w.set_fullscreen(false);
        }
        remember_size(app, &w);
        let _ = w.hide();
        let _ = app.emit_to(MAIN, "hidden", ());
    }
    // A preview still loading when the user closed it must not pop up afterwards.
    if let Ok(mut p) = state.pending.lock() {
        *p = None;
    }
    if let Ok(mut h) = state.hidden_since.lock() {
        *h = Some(Instant::now());
    }
    if state.visible.swap(false, Ordering::SeqCst) {
        crate::integration::visibility_changed(app, false);
    }
}

fn remember_size(app: &AppHandle, w: &WebviewWindow) {
    if w.is_fullscreen().unwrap_or(false) || (w.is_maximized().unwrap_or(false) && fills_monitor(w))
    {
        return;
    }
    let (Ok(size), Ok(scale)) = (w.inner_size(), w.scale_factor()) else {
        return;
    };
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

/// Some tiling compositors (Hyprland) report floating windows as maximized, so only trust
/// the flag when the window really covers its monitor.
fn fills_monitor(w: &WebviewWindow) -> bool {
    let (Ok(size), Ok(Some(m))) = (w.outer_size(), w.current_monitor()) else {
        return true;
    };
    let ms = m.size();
    size.width as f64 >= ms.width as f64 * 0.9 && size.height as f64 >= ms.height as f64 * 0.85
}

/// Center the window on the monitor under the cursor, shrinking it when the remembered size
/// doesn't fit there (e.g. after moving from an external display to a laptop screen).
/// Returns the logical size the window opens with.
fn place_on_cursor_monitor(app: &AppHandle, w: &WebviewWindow) -> (f64, f64) {
    let ws = app
        .state::<AppState>()
        .window_state
        .lock()
        .map(|s| *s)
        .unwrap_or_default();
    let (mut width, mut height) = (
        ws.width.clamp(420.0, 6000.0),
        ws.height.clamp(300.0, 4000.0),
    );
    let monitors = w.available_monitors().unwrap_or_default();
    let under_cursor = app.cursor_position().ok().and_then(|cursor| {
        monitors.iter().find(|m| {
            let (p, s) = (m.position(), m.size());
            cursor.x >= p.x as f64
                && cursor.y >= p.y as f64
                && cursor.x < (p.x + s.width as i32) as f64
                && cursor.y < (p.y + s.height as i32) as f64
        })
    });
    let m = match under_cursor {
        Some(m) => m.clone(),
        // Wayland can't report a global cursor position.
        None => match w
            .current_monitor()
            .ok()
            .flatten()
            .or_else(|| w.primary_monitor().ok().flatten())
        {
            Some(m) => m,
            None => return (width, height),
        },
    };
    let scale = m.scale_factor();
    let (mp, ms) = (m.position(), m.size());
    let fit_width = (ms.width as f64 / scale * 0.92).max(420.0);
    let fit_height = (ms.height as f64 / scale * 0.92).max(300.0);
    let shrunk = width > fit_width || height > fit_height;
    if shrunk {
        width = width.min(fit_width);
        height = height.min(fit_height);
        let _ = w.set_size(tauri::LogicalSize::new(width, height));
    }
    // An unmapped window may report a bogus size, so prefer the size we open it with.
    let mut outer_width = (width * scale) as i32;
    let mut outer_height = (height * scale) as i32;
    if let (false, Ok(size)) = (shrunk, w.outer_size()) {
        if size.width > 200 && size.height > 150 {
            outer_width = size.width as i32;
            outer_height = size.height as i32;
        }
    }
    let x = mp.x + (ms.width as i32 - outer_width) / 2;
    let y = mp.y + (ms.height as i32 - outer_height) / 2;
    let _ = w.set_position(tauri::PhysicalPosition::new(x.max(mp.x), y.max(mp.y)));
    (width, height)
}

/// Whether the process must stay alive when idle (something is listening for Space, or the
/// tray icon shows that we are running).
pub fn keep_alive(app: &AppHandle) -> bool {
    app.state::<AppState>().service.load(Ordering::SeqCst)
        || crate::integration::has_listeners()
        || crate::tray::active()
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
        app.state::<AppState>()
            .service
            .store(true, Ordering::SeqCst);
    }
    let path = args
        .paths
        .iter()
        .find_map(|p| crate::util::normalize_arg(p, cwd));
    match path {
        Some(p) => open(app, Some(p), Source::Local),
        None if args.settings => open_settings(app),
        None if args.service => {}
        // Launched again from the app menu: bring up what is already showing.
        None => open_main(app),
    }
}
