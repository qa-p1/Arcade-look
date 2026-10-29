//! App state and window lifecycle.
//!
//! Preview and Settings use independent, on-demand webviews. Settings is destroyed on close.
//! The resident Rust process keeps Link, the tray and file-manager shortcuts available.
//! `idleMinutes` controls process exit when no service mode, tray or shortcut needs it.

use crate::config::{Config, WindowState};
use crate::plugins::Plugin;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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

// Thirty seconds keeps repeated Space previews warm while promptly releasing expensive
// WebKit/WebView2 processes after browsing stops (SPEC §17.5), even with idleMinutes = 0.
const WEBVIEW_IDLE_RELEASE: Duration = Duration::from_secs(30);

// Destroying the webview leaves WebKit's network process and the grown heap
// behind (about 145 MiB against 40 for a fresh process). After this much more
// quiet, a resident Look restarts into the background. Browsing sessions never
// see it; the first preview after a long pause costs about 50 ms more.
const LEAN_RESTART_AFTER: Duration = Duration::from_secs(5 * 60);

// Each released webview also leaves some of WebKit's shared memory behind in
// this process. Someone previewing a file every minute or two never gives Look
// the quiet above: an all-day session grew by about two descriptors for every
// three previews, toward the 1024 limit. After this many releases the next one
// restarts Look as soon as it is hidden, so the leftovers stay bounded.
const LEAN_RESTART_EVERY: u32 = 10;
static RELEASES: AtomicU32 = AtomicU32::new(0);

#[derive(Default)]
struct FrontendState {
    ready: bool,
    releasing: bool,
    pending: Vec<Request>,
    hidden_since: Option<Instant>,
}

impl FrontendState {
    fn request(&mut self, request: Request) -> Option<Request> {
        // Cancel an old release before rendering starts, not just after it paints.
        self.hidden_since = None;
        if self.ready {
            Some(request)
        } else {
            self.pending.push(request);
            None
        }
    }

    fn ready(&mut self) -> Vec<Request> {
        if self.releasing {
            return Vec::new();
        }
        self.ready = true;
        std::mem::take(&mut self.pending)
    }

    fn destroyed(&mut self) -> bool {
        self.ready = false;
        self.releasing = false;
        !self.pending.is_empty()
    }

    fn hide(&mut self, now: Instant) {
        self.pending.clear();
        self.hidden_since = Some(now);
    }

    fn still_hidden(&self, since: Instant) -> bool {
        self.hidden_since == Some(since)
    }
}

pub struct AppState {
    frontend: Mutex<FrontendState>,
    settings_frontend: Mutex<FrontendState>,
    pub config: RwLock<Config>,
    /// Modification time of config.json when it was last read or written.
    pub config_stamp: Mutex<Option<SystemTime>>,
    pub plugins: RwLock<Vec<Plugin>>,
    pub window_state: Mutex<WindowState>,
    pub service: AtomicBool,
    pub visible: AtomicBool,
    pub source: Mutex<Source>,
    pub current: Mutex<Option<PathBuf>>,
    pub activation_token: Mutex<Option<String>>,
    /// Files another Arcade app asked to preview together; ←/→ step through
    /// them instead of the folder.
    pub batch: Mutex<Option<Vec<PathBuf>>>,
    /// Serializes live shortcut/settings changes from UI and Link clients.
    pub settings_changes: Mutex<()>,
    /// Link tray-host worker. Dropping it restores the app's own icon.
    pub tray_watcher: Mutex<Option<arcade_link::trayhost::TrayHostWatcher>>,
    pub tray_state: Mutex<arcade_link::trayhost::TrayState>,
}

impl AppState {
    pub fn new(config: Config, plugins: Vec<Plugin>, service: bool) -> Self {
        Self {
            frontend: Mutex::new(FrontendState::default()),
            settings_frontend: Mutex::new(FrontendState::default()),
            config: RwLock::new(config),
            config_stamp: Mutex::new(crate::config::stamp()),
            plugins: RwLock::new(plugins),
            window_state: Mutex::new(crate::config::load_state()),
            service: AtomicBool::new(service),
            visible: AtomicBool::new(false),
            source: Mutex::new(Source::Local),
            current: Mutex::new(None),
            activation_token: Mutex::new(None),
            batch: Mutex::new(None),
            settings_changes: Mutex::new(()),
            tray_watcher: Mutex::new(None),
            tray_state: Mutex::new(arcade_link::trayhost::TrayState::None),
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
pub const SETTINGS: &str = "settings";

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
    if let Ok(mut b) = state.batch.lock() {
        if b.as_ref()
            .is_some_and(|list| !path.as_ref().is_some_and(|p| list.contains(p)))
        {
            *b = None;
        }
    }
    let request = path.map_or(Request::Welcome, Request::File);
    deliver(app, request);
}

/// Preview several files as one batch (from another Arcade app): ←/→ step
/// through them in this order.
pub fn open_batch(app: &AppHandle, paths: Vec<PathBuf>) {
    let first = paths.first().cloned();
    if let Ok(mut b) = app.state::<AppState>().batch.lock() {
        *b = (paths.len() > 1).then_some(paths);
    }
    open(app, first, Source::Local);
}

/// The next file in the current batch, if `path` belongs to one.
pub fn batch_neighbor(
    app: &AppHandle,
    path: &std::path::Path,
    delta: i64,
) -> Option<Option<String>> {
    let state = app.state::<AppState>();
    let batch = state.batch.lock().ok()?;
    let list = batch.as_ref()?;
    let i = list.iter().position(|p| p == path)? as i64;
    let j = (i + delta).rem_euclid(list.len() as i64) as usize;
    Some(Some(list[j].to_string_lossy().to_string()))
}

/// Show the settings screen (tray icon, `--settings`, welcome screen link).
pub fn open_settings(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        refresh_config(&handle);
        let state = handle.state::<AppState>();
        let window = handle.get_webview_window(SETTINGS);
        let (ready, create) = {
            let mut frontend = state
                .settings_frontend
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if window.is_none() {
                frontend.ready = false;
            }
            let create = window.is_none() && !frontend.releasing;
            (frontend.request(Request::Settings).is_some(), create)
        };
        if ready {
            show_settings(&handle);
        } else if create {
            if let Err(error) = build_settings_window(&handle) {
                eprintln!("arcade-look: couldn't create Settings: {error}");
            }
        }
    });
}

fn background(app: &AppHandle, window: &WebviewWindow) -> tauri::window::Color {
    use arcade_link::design::{palette, rgb, Theme};
    let system_dark = window
        .theme()
        .is_ok_and(|theme| theme == tauri::Theme::Dark);
    let theme = Theme::resolve(&app.state::<AppState>().config().theme, system_dark);
    let [r, g, b] = rgb(palette(theme, arcade_link::ids::LOOK).bg);
    tauri::window::Color(r, g, b, 255)
}

fn build_settings_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let window =
        WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App("index.html?settings".into()))
            .title("Settings — Arcade Look")
            .inner_size(760.0, 560.0)
            .min_inner_size(720.0, 520.0)
            .decorations(true)
            .visible(false)
            .resizable(true)
            .data_directory(
                arcade_link::paths::app_dirs(arcade_link::ids::LOOK)
                    .data
                    .join("webview"),
            )
            .build()?;
    window.set_background_color(Some(background(app, &window)))?;
    Ok(window)
}

pub fn settings_ready(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut frontend = state
        .settings_frontend
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    frontend.ready();
    drop(frontend);
    let _ = app.emit_to(SETTINGS, "config", state.config());
}

pub fn show_settings(app: &AppHandle) {
    let state = app.state::<AppState>();
    let frontend = state
        .settings_frontend
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if !frontend.ready || frontend.releasing {
        return;
    }
    drop(frontend);
    if let Some(window) = app.get_webview_window(SETTINGS) {
        crate::window_geometry::place(app, &window);
        let _ = window.show();
        let _ = window.unminimize();
        focus(&window);
    }
}

pub fn close_settings(app: &AppHandle) {
    crate::integration::set_shortcut_recording(false);
    let state = app.state::<AppState>();
    let Some(window) = app.get_webview_window(SETTINGS) else {
        return;
    };
    crate::window_geometry::remember(&window);
    let mut frontend = state
        .settings_frontend
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    frontend.hide(Instant::now());
    frontend.ready = false;
    frontend.releasing = true;
    drop(frontend);
    if let Err(error) = window.destroy() {
        let mut frontend = state
            .settings_frontend
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        frontend.releasing = false;
        frontend.ready = true;
        eprintln!("arcade-look: couldn't close Settings: {error}");
    }
}

pub fn settings_destroyed(app: &AppHandle) {
    let state = app.state::<AppState>();
    let pending = state
        .settings_frontend
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .destroyed();
    if pending {
        if let Err(error) = build_settings_window(app) {
            eprintln!("arcade-look: couldn't reopen Settings: {error}");
        }
        return;
    }
    // WebKit retains its network process after the last webview is destroyed.
    // Use Look's existing lean restart to return an otherwise idle service to
    // its pre-Settings memory level. A new preview cancels this timer, and an
    // active peer job or another window prevents the restart.
    if app.webview_windows().is_empty() {
        let since = Instant::now();
        let mut frontend = state.frontend.lock().unwrap_or_else(|e| e.into_inner());
        if !frontend.pending.is_empty() {
            return;
        }
        frontend.hide(since);
        drop(frontend);
        schedule_lean_restart(app, since, true);
    }
}

/// Bring the window forward if it is showing; otherwise open the welcome screen.
pub fn open_main(app: &AppHandle) {
    if app.state::<AppState>().is_visible() {
        show(app);
    } else {
        open(app, None, Source::Local);
    }
}

/// Serialize delivery and destruction on the main thread, including tray callbacks.
fn deliver(app: &AppHandle, request: Request) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        refresh_config(&handle);
        let state = handle.state::<AppState>();
        let window = handle.get_webview_window(MAIN);
        let (request, create) = {
            let mut frontend = state.frontend.lock().unwrap_or_else(|e| e.into_inner());
            if window.is_none() {
                frontend.ready = false;
            }
            let create = window.is_none() && !frontend.releasing;
            (frontend.request(request), create)
        };
        if let Some(request) = request {
            emit_request(&handle, request);
        } else if create {
            if let Err(e) = build_window(&handle) {
                eprintln!("arcade-look: could not create window: {e}");
            }
        }
    });
}

fn emit_request(app: &AppHandle, request: Request) {
    match request {
        Request::File(path) => {
            let _ = app.emit_to(MAIN, "open", Some(path.to_string_lossy().to_string()));
        }
        Request::Welcome => {
            let _ = app.emit_to(MAIN, "open", None::<String>);
        }
        Request::Settings => {
            let _ = app.emit_to(MAIN, "settings", ());
        }
    }
}

/// Called only after the page has initialized and installed every event listener.
pub fn frontend_ready(app: &AppHandle) {
    // Test runners (scripts/verify-rendering.py, the Link e2e checks) wait for this.
    crate::dbg_log!("frontend ready");
    let state = app.state::<AppState>();
    let requests = state
        .frontend
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .ready();
    // Settings might have changed between bootstrap and listener registration.
    let _ = app.emit_to(MAIN, "config", state.config());
    for request in requests {
        emit_request(app, request);
    }
}

/// Tauri removes the old window from its manager before this RunEvent. Destruction
/// is asynchronous: requests arriving after destroy() must wait for this event.
pub fn window_destroyed(app: &AppHandle) {
    let pending = app
        .state::<AppState>()
        .frontend
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .destroyed();
    if pending {
        if let Err(error) = build_window(app) {
            eprintln!("arcade-look: could not recreate window: {error}");
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
    crate::link_consumer::refresh(app);
    if state
        .frontend
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .ready
    {
        let _ = app.emit_to(MAIN, "config", config);
    }
}

pub fn build_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    crate::dbg_log!("creating window");
    let state = app.state::<AppState>();
    state
        .frontend
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .ready = false;
    let ws = state.window_state.lock().map(|s| *s).unwrap_or_default();
    let builder = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("index.html".into()))
        .title("Arcade Look")
        .inner_size(
            ws.width.clamp(720.0, 6000.0),
            ws.height.clamp(520.0, 4000.0),
        )
        .min_inner_size(720.0, 520.0)
        // Test-only: WebKitGTK needs a mapped window to boot under Xvfb
        // without a window manager. Production still reveals after painting.
        .visible(
            std::env::var("ARCADE_E2E_INNER").as_deref() == Ok("1")
                && std::env::var("ALOOK_E2E_MAP_EARLY").as_deref() == Ok("1"),
        )
        .resizable(true)
        .focused(true)
        .center()
        .on_page_load(|_, payload| {
            crate::dbg_log!("page load {:?}: {}", payload.event(), payload.url());
        })
        .data_directory(
            arcade_link::paths::app_dirs(arcade_link::ids::LOOK)
                .data
                .join("webview"),
        );
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true);
    #[cfg(not(target_os = "macos"))]
    let builder = builder.decorations(false);
    let window = builder.build()?;
    window.set_background_color(Some(background(app, &window)))?;
    Ok(window)
}

/// Show and focus the window (called by the UI once the preview has painted).
pub fn show(app: &AppHandle) {
    crate::dbg_log!("show");
    let state = app.state::<AppState>();
    let Some(w) = app.get_webview_window(MAIN) else {
        return;
    };
    let frontend = state.frontend.lock().unwrap_or_else(|e| e.into_inner());
    // Ignore a late paint notification from a preview closed while it was loading.
    if frontend.hidden_since.is_some() || frontend.releasing {
        return;
    }
    drop(frontend);
    let was_visible = state.visible.swap(true, Ordering::SeqCst);
    if !was_visible {
        let (_width, _height) = place_on_cursor_monitor(app, &w);
        crate::window_geometry::place(app, &w);
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
    crate::integration::set_shortcut_recording(false);
    crate::dbg_log!("hide");
    let state = app.state::<AppState>();
    if let Some(w) = app.get_webview_window(MAIN) {
        if w.is_fullscreen().unwrap_or(false) {
            let _ = w.set_fullscreen(false);
        }
        remember_size(app, &w);
        crate::window_geometry::remember(&w);
        let _ = w.hide();
        let _ = app.emit_to(MAIN, "hidden", ());
    }
    // A preview still loading when the user closed it must not pop up afterwards.
    let since = Instant::now();
    state
        .frontend
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .hide(since);
    schedule_idle_release(app, since);
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
        ws.width.clamp(720.0, 6000.0),
        ws.height.clamp(520.0, 4000.0),
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
    let fit_width = (ms.width as f64 / scale * 0.92).max(720.0);
    let fit_height = (ms.height as f64 / scale * 0.92).max(520.0);
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

/// Whether service mode, shortcuts or the tray need the resident process. Link
/// doesn't: a peer that finds Look stopped launches it (Link SPEC §7).
pub fn keep_alive(app: &AppHandle) -> bool {
    app.state::<AppState>().config().keep_ready
        && (app.state::<AppState>().service.load(Ordering::SeqCst)
            || crate::integration::has_listeners()
            || crate::tray::active())
}

/// One-shot release, never a periodic idle poll. Retain only the app handle, so the
/// timer itself cannot keep a destroyed webview alive. Stale timers cannot destroy
/// a reopened window (including one still loading) or a later window generation.
fn schedule_idle_release(app: &AppHandle, since: Instant) {
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(WEBVIEW_IDLE_RELEASE);
        let app = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            let state = app.state::<AppState>();
            let mut frontend = state.frontend.lock().unwrap_or_else(|e| e.into_inner());
            if !frontend.still_hidden(since) || state.is_visible() {
                return;
            }
            if let Some(window) = app.get_webview_window(MAIN) {
                if window.is_visible().unwrap_or(true) {
                    return;
                }
                let was_ready = frontend.ready;
                frontend.ready = false;
                frontend.releasing = true;
                drop(frontend);
                if let Err(error) = window.destroy() {
                    let mut frontend = state.frontend.lock().unwrap_or_else(|e| e.into_inner());
                    frontend.ready = was_ready;
                    frontend.releasing = false;
                    eprintln!("arcade-look: could not release webview: {error}");
                } else {
                    let releases = RELEASES.fetch_add(1, Ordering::SeqCst) + 1;
                    schedule_lean_restart(&app, since, releases >= LEAN_RESTART_EVERY);
                }
            }
        });
        // Preserve the optional non-resident process timeout separately. Link,
        // --service, the tray and shortcuts never depend on a surviving webview.
        let minutes = handle.state::<AppState>().config().idle_minutes;
        if minutes == 0 || keep_alive(&handle) {
            return;
        }
        let timeout = Duration::from_secs(minutes.saturating_mul(60));
        std::thread::sleep(timeout.saturating_sub(since.elapsed()));
        let app = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            let state = app.state::<AppState>();
            let hidden = state
                .frontend
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .still_hidden(since);
            if hidden && !state.is_visible() && !keep_alive(&app) {
                app.exit(0);
            }
        });
    });
}

/// Restart a resident Look into the background once it has stayed hidden for
/// [`LEAN_RESTART_AFTER`] after its webview was released (Linux, where WebKit
/// keeps its network process for the life of the process).
fn schedule_lean_restart(app: &AppHandle, since: Instant, soon: bool) {
    if !cfg!(target_os = "linux") || std::env::var_os("ARCADE_LOOK_NO_LEAN_RESTART").is_some() {
        return;
    }
    let handle = app.clone();
    // Debug builds can shorten the wait to test it.
    let after = if soon {
        Duration::from_secs(1)
    } else {
        cfg!(debug_assertions)
            .then(|| std::env::var("ALOOK_LEAN_RESTART_SECS").ok()?.parse().ok())
            .flatten()
            .map_or(LEAN_RESTART_AFTER, Duration::from_secs)
    };
    std::thread::spawn(move || {
        std::thread::sleep(after);
        let app = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            let state = app.state::<AppState>();
            let hidden = state
                .frontend
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .still_hidden(since);
            // A Look that would exit when idle needs no restart; one serving a
            // request keeps running.
            crate::dbg_log!(
                "lean restart check: hidden {hidden}, visible {}, windows {}, keep alive {}, busy {}",
                state.is_visible(),
                app.webview_windows().len(),
                keep_alive(&app),
                crate::link_consumer::busy()
            );
            if hidden
                && !state.is_visible()
                && app.webview_windows().is_empty()
                && keep_alive(&app)
                && !crate::link_consumer::busy()
            {
                crate::dbg_log!("lean restart");
                if let Err(error) = crate::tray::restart_request(&app) {
                    eprintln!("arcade-look: lean restart: {error}");
                }
            }
        });
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
        None if args.hide => hide(app),
        None if args.toggle => {
            crate::integration::toggle_selection(app);
        }
        None if args.show => open_main(app),
        None if args.service => {}
        // Launched again from the app menu: bring up what is already showing.
        None => open_main(app),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_preview_requests_wait_for_listeners_and_drain_once() {
        let mut frontend = FrontendState::default();
        let requests = vec![
            Request::File("first.png".into()),
            Request::File("second.pdf".into()),
            Request::Settings,
        ];
        for request in &requests {
            assert_eq!(frontend.request(request.clone()), None);
        }
        assert_eq!(frontend.ready(), requests);
        assert!(frontend.ready().is_empty());
        assert_eq!(frontend.request(Request::Welcome), Some(Request::Welcome));
    }

    #[test]
    fn reopening_while_loading_cancels_release_and_old_timers_stay_cancelled() {
        let mut frontend = FrontendState::default();
        let first_close = Instant::now();
        frontend.hide(first_close);
        assert!(frontend.still_hidden(first_close));
        assert_eq!(frontend.request(Request::File("slow.pdf".into())), None);
        assert!(!frontend.still_hidden(first_close));
        // Close again before loading finishes: the first timer must not shorten
        // this second grace, and the cancelled preview must never be delivered.
        let second_close = first_close + Duration::from_secs(10);
        frontend.hide(second_close);
        assert!(!frontend.still_hidden(first_close));
        assert!(frontend.still_hidden(second_close));
        assert!(frontend.ready().is_empty());
        assert!(frontend.still_hidden(second_close));
    }

    #[test]
    fn request_during_asynchronous_destruction_waits_for_the_new_page() {
        let mut frontend = FrontendState::default();
        frontend.ready();
        frontend.hide(Instant::now());
        frontend.ready = false;
        frontend.releasing = true;
        let file = Request::File("during-destroy.pdf".into());
        assert_eq!(frontend.request(file.clone()), None);
        // A late handshake from the old page must not consume the request.
        assert!(frontend.ready().is_empty());
        assert!(frontend.destroyed());
        assert_eq!(frontend.ready(), vec![file]);
    }

    #[test]
    fn settings_and_preview_can_reopen_warm_then_cold_after_destruction() {
        let mut frontend = FrontendState::default();
        frontend.ready();
        let closed = Instant::now();
        frontend.hide(closed);
        assert_eq!(frontend.request(Request::Settings), Some(Request::Settings));
        assert!(!frontend.still_hidden(closed));
        frontend.hide(closed + Duration::from_secs(1));
        assert!(!frontend.destroyed());
        let file = Request::File("reopened.png".into());
        assert_eq!(frontend.request(file.clone()), None);
        assert_eq!(frontend.ready(), vec![file]);
    }
}
