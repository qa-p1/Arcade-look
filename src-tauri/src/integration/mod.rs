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
#[cfg(any(target_os = "linux", target_os = "macos"))]
use crate::util::OrStr;
use crate::util::Res;
use arcade_link::app::{ShortcutSetResult, ShortcutVia};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

static SHORTCUT_ACTIVE: AtomicBool = AtomicBool::new(false);
static SHORTCUT_RECORDING: AtomicBool = AtomicBool::new(false);
static ACTIVE_SHORTCUT: Mutex<Option<String>> = Mutex::new(None);

/// Keep receipt-owned login entries discoverable by repair and uninstall.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn set_login_entry(
    store: &arcade_link::receipt::Store,
    path: &std::path::Path,
    contents: Option<&[u8]>,
) -> Res<()> {
    let receipt = store.read(arcade_link::ids::LOOK).or_str()?;
    let mut document = None;
    if let Some(receipt) = &receipt {
        receipt.validate_native_paths().or_str()?;
        document = Some(
            serde_json::from_slice::<serde_json::Value>(
                &std::fs::read(store.path(arcade_link::ids::LOOK).or_str()?).or_str()?,
            )
            .or_str()?,
        );
    }
    if let Some(contents) = contents {
        if let Some(directory) = path.parent() {
            std::fs::create_dir_all(directory).or_str()?;
        }
        std::fs::write(path, contents).or_str()?;
    } else {
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(format!("remove {}: {e}", path.display()));
            }
            _ => {}
        }
    }
    if let Some(mut document) = document {
        document["integration"]["autostart"] =
            serde_json::to_value(contents.map(|_| path)).or_str()?;
        document["updatedAt"] = arcade_link::manifest::now_rfc3339().into();
        let bytes = serde_json::to_vec_pretty(&document).or_str()?;
        arcade_link::paths::write_atomic(
            &store.path(arcade_link::ids::LOOK).or_str()?,
            &bytes,
            true,
        )
        .or_str()?;
    }
    Ok(())
}

pub fn set_shortcut_recording(recording: bool) {
    SHORTCUT_RECORDING.store(recording, Ordering::SeqCst);
}

pub fn shortcut_supported() -> bool {
    !cfg!(target_os = "linux") || std::env::var_os("DISPLAY").is_some()
}

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
    // The plugin is only registered when a shortcut is configured (see lib.rs).
    if !shortcut.is_empty()
        && app
            .try_state::<tauri_plugin_global_shortcut::GlobalShortcut<tauri::Wry>>()
            .is_some()
    {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        match app.global_shortcut().register(shortcut) {
            Ok(()) => {
                SHORTCUT_ACTIVE.store(true, Ordering::SeqCst);
                *ACTIVE_SHORTCUT.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(shortcut.to_string());
            }
            Err(e) => {
                eprintln!("arcade-look: could not register global shortcut {shortcut:?}: {e}")
            }
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

/// Replace the global shortcut live. Link callers run on a worker; the
/// global-shortcut plugin marshals native registration to the UI thread.
pub fn set_live_shortcut(
    app: &AppHandle,
    input: &str,
) -> Result<ShortcutSetResult, arcade_link::LinkError> {
    let accelerator = if input.is_empty() {
        String::new()
    } else {
        arcade_link::accelerator::normalize(input)
            .map_err(|e| arcade_link::LinkError::denied(&e.to_string()))?
    };
    if !accelerator.is_empty() && !shortcut_supported() {
        return Ok(ShortcutSetResult {
            applied: false,
            via: ShortcutVia::Manual,
            hint: Some("Bind `arcade-look --toggle` in Settings → Keyboard".into()),
        });
    }
    let manager = app
        .try_state::<tauri_plugin_global_shortcut::GlobalShortcut<tauri::Wry>>()
        .ok_or_else(|| arcade_link::LinkError::unavailable("native shortcuts are unavailable"))?;
    let old = ACTIVE_SHORTCUT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    if old.as_deref() == Some(accelerator.as_str()) {
        return Ok(ShortcutSetResult {
            applied: true,
            via: ShortcutVia::Native,
            hint: None,
        });
    }
    if !accelerator.is_empty() {
        let parsed = tauri_plugin_global_shortcut::Shortcut::from_str(&accelerator)
            .map_err(|e| arcade_link::LinkError::denied(&e.to_string()))?;
        manager
            .register(parsed)
            .map_err(|e| arcade_link::LinkError::unavailable(e.to_string()))?;
    }
    if let Some(old) = old {
        if let Err(error) = manager.unregister(old.as_str()) {
            if !accelerator.is_empty() {
                let _ = manager.unregister(accelerator.as_str());
            }
            return Err(arcade_link::LinkError::unavailable(error.to_string()));
        }
    }
    *ACTIVE_SHORTCUT.lock().unwrap_or_else(|e| e.into_inner()) =
        (!accelerator.is_empty()).then_some(accelerator);
    SHORTCUT_ACTIVE.store(
        ACTIVE_SHORTCUT
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some(),
        Ordering::SeqCst,
    );
    Ok(ShortcutSetResult {
        applied: true,
        via: ShortcutVia::Native,
        hint: None,
    })
}

/// True when something must keep running in the background to catch the next Space press.
pub fn has_listeners() -> bool {
    #[cfg(windows)]
    if windows::hook_active() {
        return true;
    }
    SHORTCUT_ACTIVE.load(Ordering::SeqCst)
}

pub fn toggle_selection(app: &AppHandle) {
    on_global_shortcut(app);
}

fn on_global_shortcut(app: &AppHandle) {
    if SHORTCUT_RECORDING.load(Ordering::SeqCst) {
        return;
    }
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
        None => app::open(
            app,
            state.current.lock().ok().and_then(|c| c.clone()),
            Source::Local,
        ),
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

/// All selected files for Link's resolveOnly mode. Runs on the Link worker.
/// GNOME Files provides previews to Look but exposes no selection resolver.
pub fn file_manager_selections() -> Vec<PathBuf> {
    #[cfg(windows)]
    return windows::foreground_selections();
    #[cfg(target_os = "macos")]
    return macos::finder_selections();
    #[allow(unreachable_code)]
    Vec::new()
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
    v.push((
        "Global shortcut".into(),
        SHORTCUT_ACTIVE.load(Ordering::SeqCst),
    ));
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
    return macos::uninstall();
    #[allow(unreachable_code)]
    Err("No integration is available for this platform.".into())
}

/// Whether Arcade Look starts (in the background, with its tray icon) when the user logs in.
pub fn autostart_enabled() -> bool {
    #[cfg(target_os = "linux")]
    return linux::autostart_enabled();
    #[cfg(windows)]
    return windows::autostart_enabled();
    #[cfg(target_os = "macos")]
    return macos::autostart_enabled();
    #[allow(unreachable_code)]
    false
}

pub fn set_autostart(enabled: bool) -> Res<()> {
    #[cfg(target_os = "linux")]
    return linux::set_autostart(enabled);
    #[cfg(windows)]
    return windows::set_autostart(enabled);
    #[cfg(target_os = "macos")]
    return macos::set_autostart(enabled);
    #[allow(unreachable_code)]
    {
        let _ = enabled;
        Err("Start at login is not available on this platform.".into())
    }
}

/// Do once what an installer would: set up the file manager integration and start on login.
/// AppImages and macOS apps have no install step, so this runs on their first launch; the
/// Windows installer does it itself (`--install-integration`). It is retried on later launches
/// until it succeeds (e.g. the app was first started from a temporary folder or a disk image),
/// and never again afterwards, so turning start on login off sticks.
pub fn first_run_setup(app: &AppHandle) {
    // Development builds keep whatever integration the developer has installed.
    if cfg!(debug_assertions) || cfg!(windows) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        // A copy with a receipt was set up by its installer (the AppImage
        // self-installer or the macOS move); a bare AppImage asks first.
        if crate::installer::has_receipt() || std::env::var_os("APPIMAGE").is_some() {
            return;
        }
        let state = app.state::<AppState>();
        if state
            .window_state
            .lock()
            .map(|s| s.setup_done)
            .unwrap_or(true)
        {
            return;
        }
        match install() {
            Ok(_) if autostart_enabled() => {
                crate::dbg_log!("first-run setup done");
                if let Ok(mut s) = state.window_state.lock() {
                    s.setup_done = true;
                    crate::config::save_state(&s);
                }
            }
            Ok(_) => crate::dbg_log!("first-run setup incomplete, will retry next launch"),
            Err(e) => eprintln!("arcade-look: first-run setup failed: {e}"),
        }
    });
}

/// Repair a start-on-login entry that points at an executable that has since moved.
pub fn refresh_autostart() {
    #[cfg(target_os = "linux")]
    std::thread::spawn(linux::refresh_autostart);
}

/// Windows GUI-subsystem builds have no console; attach to the parent's for CLI output.
pub fn attach_console() {
    #[cfg(windows)]
    windows::attach_console();
}

/// Path of the executable that integrations should launch (the AppImage, not its mount).
/// `APPIMAGE` is only ours if we run from that AppImage's mount (`APPDIR`); a process started
/// from another AppImage inherits both variables.
pub fn launcher_path() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("arcade-look"));
    if let (Some(appimage), Some(appdir)) =
        (std::env::var_os("APPIMAGE"), std::env::var_os("APPDIR"))
    {
        if exe.starts_with(&appdir) {
            return PathBuf::from(appimage);
        }
    }
    exe
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod login_receipt_tests {
    use super::*;
    use arcade_link::install::{self, Environment, UninstallOptions};
    use arcade_link::receipt::{InstallMethod, Integration, ManagedBy, Receipt};
    use std::fs;

    struct Fixture {
        environment: Environment,
        path: PathBuf,
        receipt: Receipt,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "login-receipt-{}",
                arcade_link::endpoint::new_token().unwrap()
            ));
            let environment = Environment::under(&root);
            let path = if cfg!(target_os = "macos") {
                root.join("Library/LaunchAgents/app.plist")
            } else {
                environment.config.join("autostart/arcade-look.desktop")
            };
            let receipt = Receipt {
                schema: 1,
                id: arcade_link::ids::LOOK.into(),
                version: "0.2.0".into(),
                channel: "nightly".into(),
                method: InstallMethod::Appimage,
                managed_by: ManagedBy::Tools,
                path: environment.applications.join("app.AppImage"),
                integration: Integration {
                    cli: Some(environment.bin.join("app")),
                    ..Default::default()
                },
                previous: None,
                installed_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
            };
            Self {
                environment,
                path,
                receipt,
            }
        }

        fn set(&self, enabled: bool) -> Result<(), String> {
            set_login_entry(
                &self.environment.receipts(),
                &self.path,
                enabled.then_some(
                    b"[Desktop Entry]\nType=Application\nX-Arcade-Id=arcade.look\nExec=app\n"
                        .as_slice(),
                ),
            )
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            if self.environment.home.exists() {
                fs::remove_dir_all(&self.environment.home).unwrap();
            }
        }
    }

    #[test]
    fn login_toggle_preserves_receipt_metadata_and_uninstall_removes_the_entry() {
        let fixture = Fixture::new();
        let store = fixture.environment.receipts();
        store.write(&fixture.receipt).unwrap();
        let file = store.path(arcade_link::ids::LOOK).unwrap();
        let mut document = serde_json::to_value(&fixture.receipt).unwrap();
        document["futureField"] = serde_json::json!({"keep": true});
        document["integration"]["futureEntry"] = serde_json::json!("keep");
        std::fs::write(&file, serde_json::to_vec(&document).unwrap()).unwrap();
        fixture.set(true).unwrap();
        assert!(fixture.path.is_file());
        let enabled = store.read(arcade_link::ids::LOOK).unwrap().unwrap();
        let mut expected = fixture.receipt.clone();
        expected.integration.autostart = Some(fixture.path.clone());
        expected.updated_at = enabled.updated_at.clone();
        assert_eq!(enabled, expected);
        assert!(enabled.updated_at > fixture.receipt.updated_at);
        fixture.set(false).unwrap();
        fixture.set(false).unwrap();
        assert!(!fixture.path.exists());
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
        assert_eq!(saved["futureField"], document["futureField"]);
        assert_eq!(
            saved["integration"]["futureEntry"],
            document["integration"]["futureEntry"]
        );
        assert!(store
            .read(arcade_link::ids::LOOK)
            .unwrap()
            .unwrap()
            .integration
            .autostart
            .is_none());
        fixture.set(true).unwrap();
        assert!(install::uninstall(
            &fixture.environment,
            arcade_link::ids::LOOK,
            UninstallOptions {
                remove_data: false,
                data_folders: &[]
            }
        )
        .unwrap());
        assert!(!fixture.path.exists());
        assert!(store.read(arcade_link::ids::LOOK).unwrap().is_none());
    }

    #[test]
    fn portable_login_toggle_never_creates_an_install_receipt() {
        let fixture = Fixture::new();
        fixture.set(true).unwrap();
        assert!(fixture.path.is_file());
        assert!(fixture
            .environment
            .receipts()
            .read(arcade_link::ids::LOOK)
            .unwrap()
            .is_none());
        fixture.set(false).unwrap();
        assert!(!fixture.path.exists());
    }

    #[test]
    fn damaged_receipt_does_not_mutate_login_entries() {
        let fixture = Fixture::new();
        let store = fixture.environment.receipts();
        store.write(&fixture.receipt).unwrap();
        fixture.set(true).unwrap();
        let original = fs::read(&fixture.path).unwrap();
        fs::write(
            store.path(arcade_link::ids::LOOK).unwrap(),
            b"invalid receipt",
        )
        .unwrap();
        assert!(fixture.set(false).is_err());
        assert_eq!(fs::read(&fixture.path).unwrap(), original);
    }
}

/// Copy, verify and only then retire an owned legacy desktop/login entry.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn migrate_entry(
    old: &std::path::Path,
    new: &std::path::Path,
    text: String,
    field: &str,
) -> Res<()> {
    migrate_entry_using(
        old,
        new,
        text,
        field,
        &arcade_link::receipt::Store::discover(),
    )
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn migrate_entry_using(
    old: &std::path::Path,
    new: &std::path::Path,
    text: String,
    field: &str,
    store: &arcade_link::receipt::Store,
) -> Res<()> {
    if !old.is_file() {
        return Ok(());
    }
    if std::fs::symlink_metadata(old)
        .or_str()?
        .file_type()
        .is_symlink()
    {
        return Err(format!("{} is a link; it was kept", old.display()));
    }
    if new.exists() && std::fs::read(new).or_str()? != text.as_bytes() {
        return Err(format!(
            "{} already exists; both entries were kept",
            new.display()
        ));
    }
    if let Some(parent) = new.parent() {
        std::fs::create_dir_all(parent).or_str()?;
    }
    arcade_link::paths::write_atomic(new, text.as_bytes(), true).or_str()?;
    if std::fs::read(new).or_str()? != text.as_bytes() {
        return Err("Entry verification failed; the old entry was kept.".into());
    }
    let receipt_path = store.path(arcade_link::ids::LOOK).or_str()?;
    if receipt_path.is_file() {
        let mut document: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&receipt_path).or_str()?).or_str()?;
        if document["integration"][field].as_str() == old.to_str() {
            document["integration"][field] = new.to_string_lossy().into_owned().into();
            let bytes = serde_json::to_vec_pretty(&document).or_str()?;
            arcade_link::paths::write_atomic(&receipt_path, &bytes, true).or_str()?;
            if std::fs::read(&receipt_path).or_str()? != bytes {
                return Err("Receipt verification failed; the old entry was kept.".into());
            }
        }
    }
    std::fs::remove_file(old).or_str()
}

pub fn migrate_entries() -> Res<()> {
    #[cfg(target_os = "linux")]
    linux::migrate_entries()?;
    #[cfg(target_os = "macos")]
    macos::migrate_entries()?;
    Ok(())
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod entry_migration_tests {
    use super::*;
    #[test]
    fn login_entry_migration_preserves_exec_receipt_and_conflicting_copies() {
        let root = std::env::temp_dir().join(format!(
            "look-entry-{}",
            arcade_link::endpoint::new_token().unwrap()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let environment = arcade_link::install::Environment::under(&root);
        let store = environment.receipts();
        let receipt = store.path(arcade_link::ids::LOOK).unwrap();
        std::fs::create_dir_all(receipt.parent().unwrap()).unwrap();
        let old = root.join("old.desktop");
        let new = root.join("dev.arcade.Look.desktop");
        let text = "[Desktop Entry]\nExec=\"/saved/custom binary.AppImage\" --service\n";
        std::fs::write(&old, text).unwrap();
        let document = serde_json::json!({"integration":{"autostart":old,"futureEntry":"keep"},"futureField":17});
        std::fs::write(&receipt, serde_json::to_vec(&document).unwrap()).unwrap();
        migrate_entry_using(&old, &new, text.into(), "autostart", &store).unwrap();
        assert!(!old.exists());
        assert_eq!(std::fs::read_to_string(&new).unwrap(), text);
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&receipt).unwrap()).unwrap();
        assert_eq!(saved["integration"]["autostart"], serde_json::json!(new));
        assert_eq!(saved["integration"]["futureEntry"], "keep");
        assert_eq!(saved["futureField"], 17);
        migrate_entry_using(&old, &new, text.into(), "autostart", &store).unwrap();
        std::fs::write(&old, "conflicting legacy entry").unwrap();
        assert!(migrate_entry_using(
            &old,
            &new,
            "different replacement".into(),
            "autostart",
            &store
        )
        .is_err());
        assert_eq!(
            std::fs::read_to_string(&old).unwrap(),
            "conflicting legacy entry"
        );
        assert_eq!(std::fs::read_to_string(&new).unwrap(), text);
        std::fs::remove_dir_all(root).unwrap();
    }
}
