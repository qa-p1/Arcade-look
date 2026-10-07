//! System tray icon: shows that Arcade Look is running in the background. Clicking it opens
//! Settings; its menu (right click) is the one every Arcade app has: Open Look, Open
//! Settings, Restart Arcade Look and, below a separator, Quit Arcade Look.
//!
//! Linux speaks StatusNotifierItem over D-Bus (KDE, GNOME with AppIndicator, waybar,
//! Quickshell, ...); Windows and macOS use Tauri's native tray.

use crate::app;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::AppHandle;

/// Whether a tray host is showing our icon right now.
static ONLINE: AtomicBool = AtomicBool::new(false);

const TOOLTIP: &str = "Arcade Look";
const TOOLTIP_DETAIL: &str = "Running in the background. Select a file and press Space.";

/// True while the icon is actually visible (a tray host exists), so the user can always
/// reach or quit the background process.
pub fn active() -> bool {
    ONLINE.load(Ordering::SeqCst)
}

#[derive(Clone, Copy)]
enum Action {
    Open,
    Settings,
    Restart,
    Quit,
}

/// The separator goes before the last item (Quit).
const ITEMS: [(&str, &str, Action); 4] = [
    ("open", "Open Look", Action::Open),
    ("settings", "Open Settings", Action::Settings),
    ("restart", "Restart Arcade Look", Action::Restart),
    ("quit", "Quit Arcade Look", Action::Quit),
];

fn run(app: &AppHandle, action: Action) {
    match action {
        Action::Open => app::open_main(app),
        Action::Settings => app::open_settings(app),
        Action::Restart => restart(app),
        Action::Quit => app.exit(0),
    }
}

/// Set on the successor so it waits for this process to exit before taking the
/// single-instance slot (otherwise it would hand its arguments to us and quit).
pub const RESTART_ENV: &str = "ARCADE_LOOK_RESTART_AFTER";

/// Starts a new background instance, then quits. Quitting without a successor would
/// silently remove Look, so a failed launch keeps this one running.
fn restart(app: &AppHandle) {
    let started = std::process::Command::new(crate::integration::launcher_path())
        .arg("--service")
        .env(RESTART_ENV, std::process::id().to_string())
        .spawn();
    match started {
        Ok(_) => app.exit(0),
        Err(e) => eprintln!("arcade-look: could not start a new instance, so it kept running: {e}"),
    }
}

/// In a successor started by [`restart`]: waits (up to 5 s) for the old instance to exit.
pub fn wait_for_predecessor() {
    let Some(pid) = std::env::var(RESTART_ENV)
        .ok()
        .and_then(|p| p.parse::<u32>().ok())
    else {
        return;
    };
    for _ in 0..50 {
        if !process_alive(pid) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[cfg(target_os = "linux")]
fn process_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(target_os = "macos")]
fn process_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };
    // SAFETY: plain Win32 calls on a handle we own and close.
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) else {
            return false;
        };
        let alive = WaitForSingleObject(handle, 0) == WAIT_TIMEOUT;
        let _ = CloseHandle(handle);
        alive
    }
}

/// At login we usually start before the panel that hosts tray icons, and shells (Quickshell,
/// waybar, plasmashell...) can restart at any time. So register optimistically and let ksni
/// re-register whenever an `org.kde.StatusNotifierWatcher` (re)appears.
#[cfg(target_os = "linux")]
pub fn start(app: &AppHandle) {
    use ksni::TrayMethods;
    use std::sync::OnceLock;
    static HANDLE: OnceLock<ksni::Handle<Tray>> = OnceLock::new();

    let tray = Tray(app.clone());
    // Cleared again by `watcher_offline` if no tray host is running yet.
    ONLINE.store(true, Ordering::SeqCst);
    tauri::async_runtime::spawn(async move {
        match tray.assume_sni_available(true).spawn().await {
            Ok(handle) => {
                let _ = HANDLE.set(handle);
            }
            Err(e) => {
                ONLINE.store(false, Ordering::SeqCst);
                eprintln!("arcade-look: no system tray available: {e}");
            }
        }
    });
}

#[cfg(target_os = "linux")]
struct Tray(AppHandle);

#[cfg(target_os = "linux")]
impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "arcade-look".into()
    }

    fn title(&self) -> String {
        TOOLTIP.into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::ApplicationStatus
    }

    /// Left click: open Settings (the menu is on right click), like Arcade Wheel.
    fn activate(&mut self, _x: i32, _y: i32) {
        app::open_settings(&self.0);
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        static ICONS: std::sync::OnceLock<Vec<ksni::Icon>> = std::sync::OnceLock::new();
        ICONS
            .get_or_init(|| {
                let pngs: [&[u8]; 3] = [
                    include_bytes!("../icons/32x32.png"),
                    include_bytes!("../icons/64x64.png"),
                    include_bytes!("../icons/128x128.png"),
                ];
                pngs.iter()
                    .filter_map(|png| {
                        image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()
                    })
                    .map(|img| {
                        let img = img.into_rgba8();
                        let (width, height) = img.dimensions();
                        let mut data = img.into_vec();
                        for px in data.as_chunks_mut::<4>().0 {
                            px.rotate_right(1); // RGBA -> ARGB
                        }
                        ksni::Icon {
                            width: width as i32,
                            height: height as i32,
                            data,
                        }
                    })
                    .collect()
            })
            .clone()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: TOOLTIP.into(),
            description: TOOLTIP_DETAIL.into(),
            ..Default::default()
        }
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        let item = |label: &str, action: Action| -> ksni::MenuItem<Self> {
            StandardItem {
                label: label.into(),
                activate: Box::new(move |t: &mut Self| run(&t.0, action)),
                ..Default::default()
            }
            .into()
        };
        let mut items: Vec<ksni::MenuItem<Self>> =
            ITEMS.iter().map(|&(_, label, a)| item(label, a)).collect();
        items.insert(ITEMS.len() - 1, ksni::MenuItem::Separator);
        items
    }

    fn watcher_online(&self) {
        crate::dbg_log!("tray host appeared");
        ONLINE.store(true, Ordering::SeqCst);
    }

    /// Keep the service alive: the tray host may still be starting, or be restarting.
    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        crate::dbg_log!("tray host offline: {reason:?}");
        ONLINE.store(false, Ordering::SeqCst);
        true
    }
}

#[cfg(not(target_os = "linux"))]
pub fn start(app: &AppHandle) {
    if let Err(e) = build_native(app) {
        eprintln!("arcade-look: could not create the tray icon: {e}");
    }
}

#[cfg(not(target_os = "linux"))]
fn build_native(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let [open, settings, restart, quit] =
        ITEMS.map(|(id, label, _)| MenuItem::with_id(app, id, label, true, None::<&str>));
    let menu = Menu::with_items(
        app,
        &[
            &open?,
            &settings?,
            &restart?,
            &PredefinedMenuItem::separator(app)?,
            &quit?,
        ],
    )?;
    // macOS menu bar items conventionally open their menu on click; on Windows a left click
    // opens Settings and the menu stays on right click.
    let menu_on_left = cfg!(target_os = "macos");
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip(format!("{TOOLTIP}\n{TOOLTIP_DETAIL}"))
        .menu(&menu)
        .show_menu_on_left_click(menu_on_left)
        .on_menu_event(|app, event| {
            if let Some(&(_, _, action)) =
                ITEMS.iter().find(|(id, _, _)| event.id().as_ref() == *id)
            {
                run(app, action);
            }
        })
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if !menu_on_left {
                    app::open_settings(tray.app_handle());
                }
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    ONLINE.store(true, Ordering::SeqCst);
    Ok(())
}
