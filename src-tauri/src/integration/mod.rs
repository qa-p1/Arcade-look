//! OS integration: how "press Space on a selected file" reaches us on each platform.
//!
//! * Linux: we implement GNOME Files' `org.gnome.NautilusPreviewer2` D-Bus previewer
//!   (the protocol Sushi uses), plus Open-With entries for other file managers.
//! * Windows: a low-level keyboard hook catches Space while Explorer's item view has focus
//!   and reads the selection over COM.
//! * macOS: a global shortcut previews Finder's selection (read through AppleScript).
//! * Everywhere: an optional global shortcut, the CLI, and drag & drop.

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(windows)]
pub mod windows;

use crate::app::{self, AppState, Source};
use crate::util::Res;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager};

static SHORTCUT_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn shortcut_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_global_shortcut::{Builder, ShortcutState};
    Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                let app = app.app_handle().clone();
                std::thread::spawn(move || on_global_shortcut(&app));
            }
        })
        .build()
}

/// Start whatever listens for "preview the selection" on this platform.
pub fn start(app: &AppHandle) {
    let cfg = app.state::<AppState>().config();
    let shortcut = cfg.global_shortcut.trim();
    if !shortcut.is_empty() {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        match app.global_shortcut().register(shortcut) {
            Ok(()) => SHORTCUT_ACTIVE.store(true, Ordering::SeqCst),
            Err(e) => eprintln!("arcade-look: could not register global shortcut {shortcut:?}: {e}"),
        }
    }
    #[cfg(target_os = "linux")]
    if cfg.nautilus_previewer {
        linux::start_previewer(app.clone());
    }
    #[cfg(windows)]
    if cfg.explorer_space {
        windows::start_hook(app.clone());
    }
}

/// True when something must keep running in the background to catch the next Space press.
pub fn has_listeners() -> bool {
    #[cfg(windows)]
    if windows::hook_active() {
        return true;
    }
    SHORTCUT_ACTIVE.load(Ordering::SeqCst)
}

fn on_global_shortcut(app: &AppHandle) {
    let state = app.state::<AppState>();
    match file_manager_selection() {
        Some((path, source)) => {
            let same = state.current.lock().ok().and_then(|c| c.clone()) == Some(path.clone());
            if same && state.is_visible() {
                app::hide(app);
            } else {
                app::open(app, Some(path), source);
            }
        }
        None if state.is_visible() => app::hide(app),
        None => app::open(app, state.current.lock().ok().and_then(|c| c.clone()), Source::Local),
    }
}

/// The selected file in the frontmost file manager, if we can read it on this platform.
pub fn file_manager_selection() -> Option<(PathBuf, Source)> {
    #[cfg(windows)]
    {
        return windows::foreground_selection().map(|p| (p, Source::Explorer));
    }
    #[cfg(target_os = "macos")]
    {
        return macos::finder_selection().map(|p| (p, Source::Finder));
    }
    #[allow(unreachable_code)]
    None
}

/// Called for ←/→ in the UI. Returns true if the file manager will drive navigation.
pub fn navigate(_app: &AppHandle, source: Source, delta: i64) -> bool {
    #[cfg(target_os = "linux")]
    if source == Source::Nautilus {
        return linux::selection_event(delta);
    }
    let _ = (source, delta);
    false
}

pub fn visibility_changed(_app: &AppHandle, _visible: bool) {
    #[cfg(target_os = "linux")]
    linux::visibility_changed(_visible);
}

/// Integration status rows shown on the welcome screen: (label, active).
pub fn status() -> Vec<(String, bool)> {
    let mut v = Vec::new();
    #[cfg(target_os = "linux")]
    v.extend(linux::status());
    #[cfg(windows)]
    v.extend(windows::status());
    #[cfg(target_os = "macos")]
    v.extend(macos::status());
    v.push(("Global shortcut".into(), SHORTCUT_ACTIVE.load(Ordering::SeqCst)));
    v
}

pub fn install() -> Res<String> {
    #[cfg(target_os = "linux")]
    return linux::install();
    #[cfg(windows)]
    return windows::install();
    #[cfg(target_os = "macos")]
    return macos::install();
    #[allow(unreachable_code)]
    Err("No integration is available for this platform.".into())
}

pub fn uninstall() -> Res<String> {
    #[cfg(target_os = "linux")]
    return linux::uninstall();
    #[cfg(windows)]
    return windows::uninstall();
    #[cfg(target_os = "macos")]
    return Ok("Nothing to remove on macOS.".into());
    #[allow(unreachable_code)]
    Err("No integration is available for this platform.".into())
}

/// Windows GUI-subsystem builds have no console; attach to the parent's for CLI output.
pub fn attach_console() {
    #[cfg(windows)]
    windows::attach_console();
}

/// Path of the executable that integrations should launch (the AppImage, not its mount).
pub fn launcher_path() -> PathBuf {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return PathBuf::from(appimage);
    }
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("arcade-look"))
}
