//! Standard tray actions and the Arcade Tools tray-host watcher.

use crate::app;
use arcade_link::trayhost::{TrayHostWatcher, TrayState, WatcherConfig};
use std::sync::atomic::{AtomicU8, Ordering};
use tauri::{AppHandle, Manager};

const TOOLTIP: &str = "Arcade Look";
const TOOLTIP_DETAIL: &str = "Press Space, see anything.";
static VISIBLE_STATE: AtomicU8 = AtomicU8::new(0);

pub fn active() -> bool {
    VISIBLE_STATE.load(Ordering::SeqCst) != 0
}

pub fn status(app: &AppHandle) -> &'static str {
    app.state::<app::AppState>()
        .tray_state
        .lock()
        .map(|state| state.as_str())
        .unwrap_or("none")
}

#[derive(Clone, Copy)]
enum Action {
    Open,
    Settings,
    Restart,
    Quit,
}

const ITEMS: [(&str, &str, Action); 4] = [
    ("open", "Open Arcade Look", Action::Open),
    ("settings", "Settings", Action::Settings),
    ("restart", "Restart Arcade Look", Action::Restart),
    ("quit", "Quit Arcade Look", Action::Quit),
];

fn run(app: &AppHandle, action: Action) {
    match action {
        Action::Open => app::open_main(app),
        Action::Settings => app::open_settings(app),
        Action::Restart => {
            if let Err(error) = restart_request(app) {
                eprintln!("arcade-look: could not restart: {error}");
            }
        }
        Action::Quit => app.exit(0),
    }
}

/// Successors wait for the old process to release the single-instance slot.
pub const RESTART_ENV: &str = "ARCADE_LOOK_RESTART_AFTER";

pub fn restart_request(app: &AppHandle) -> Result<(), arcade_link::LinkError> {
    let mut command = std::process::Command::new(crate::integration::launcher_path());
    // The successor runs from a new mount; it mustn't keep this one alive.
    arcade_link::childenv::detach_from_image(&mut command)
        .arg("--service")
        .env(RESTART_ENV, std::process::id().to_string())
        .spawn()
        .map_err(|e| arcade_link::LinkError::unavailable(e.to_string()))?;
    app.exit(0);
    Ok(())
}

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
    // SAFETY: the process handle is only queried and closed here.
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) else {
            return false;
        };
        let alive = WaitForSingleObject(handle, 0) == WAIT_TIMEOUT;
        let _ = CloseHandle(handle);
        alive
    }
}

pub fn start(app: &AppHandle) {
    let app_state = app.state::<app::AppState>();
    let config = app_state.config();
    let watcher = TrayHostWatcher::start(
        WatcherConfig {
            locations: arcade_link::Locations::discover(),
            app: crate::link_consumer::me(),
            link_enabled: config.link_enabled,
            has_tray: true,
        },
        {
            let app = app.clone();
            move |state| {
                let app = app.clone();
                let _ = app
                    .clone()
                    .run_on_main_thread(move || set_state(&app, state));
            }
        },
    );
    match watcher {
        Ok(watcher) => {
            if let Ok(mut slot) = app_state.tray_watcher.lock() {
                *slot = Some(watcher);
            }
        }
        Err(error) => {
            eprintln!("arcade-look: tray watcher unavailable: {error}");
            set_state(app, TrayState::Own);
        }
    }
}

fn set_state(app: &AppHandle, state: TrayState) {
    VISIBLE_STATE.store(
        match state {
            TrayState::None => 0,
            TrayState::Own | TrayState::Hosted => 1,
        },
        Ordering::SeqCst,
    );
    if let Ok(mut current) = app.state::<app::AppState>().tray_state.lock() {
        *current = state;
    }
    #[cfg(target_os = "linux")]
    linux::set_state(app, state);
    #[cfg(not(target_os = "linux"))]
    native::set_state(app, state);
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use ksni::TrayMethods;
    use std::sync::atomic::AtomicBool;
    use std::sync::OnceLock;

    struct Tray {
        app: AppHandle,
    }

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
        fn activate(&mut self, _x: i32, _y: i32) {
            app::open_main(&self.app);
        }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            static ICONS: OnceLock<Vec<ksni::Icon>> = OnceLock::new();
            ICONS
                .get_or_init(|| {
                    [include_bytes!("../icons/tray-white.png").as_slice()]
                        .iter()
                        .filter_map(|png| image::load_from_memory(png).ok())
                        .map(|image| {
                            let image = image.into_rgba8();
                            let (width, height) = image.dimensions();
                            let mut data = image.into_vec();
                            for pixel in data.as_chunks_mut::<4>().0 {
                                pixel.rotate_right(1);
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
                    activate: Box::new(move |tray: &mut Self| run(&tray.app, action)),
                    ..Default::default()
                }
                .into()
            };
            let mut items: Vec<_> = ITEMS
                .iter()
                .map(|(_, label, action)| item(label, *action))
                .collect();
            items.insert(2, ksni::MenuItem::Separator);
            items
        }
    }

    /// The latest requested state; one serialized task reconciles to it.
    static VISIBLE: AtomicBool = AtomicBool::new(false);
    static HANDLE: OnceLock<tauri::async_runtime::Mutex<Option<ksni::Handle<Tray>>>> =
        OnceLock::new();

    /// Only Own registers an item. Hosts such as Caelestia and Waybar still
    /// show Passive items, so a hosted icon is unregistered, not hidden. Each
    /// change takes the lock in turn and applies the newest request, so an old
    /// item is gone before a new one registers.
    pub fn set_state(app: &AppHandle, state: TrayState) {
        VISIBLE.store(state == TrayState::Own, Ordering::SeqCst);
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let mut handle = HANDLE
                .get_or_init(|| tauri::async_runtime::Mutex::new(None))
                .lock()
                .await;
            if handle.as_ref().is_some_and(ksni::Handle::is_closed) {
                *handle = None;
            }
            let visible = VISIBLE.load(Ordering::SeqCst);
            if visible && handle.is_none() {
                match (Tray { app }).assume_sni_available(true).spawn().await {
                    Ok(spawned) => *handle = Some(spawned),
                    Err(error) => eprintln!("arcade-look: tray registration failed: {error}"),
                }
            } else if !visible {
                if let Some(old) = handle.take() {
                    old.shutdown().await;
                }
            }
        });
    }
}

#[cfg(not(target_os = "linux"))]
mod native {
    use super::*;
    use std::cell::RefCell;
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    struct NativeTray {
        prepared: Option<TrayIconBuilder<tauri::Wry>>,
        icon: Option<tauri::tray::TrayIcon<tauri::Wry>>,
    }
    thread_local! {
        static TRAY: RefCell<Option<NativeTray>> = const { RefCell::new(None) };
    }

    fn prepare(app: &AppHandle) -> tauri::Result<TrayIconBuilder<tauri::Wry>> {
        let [open, settings, restart, quit] =
            ITEMS.map(|(id, label, _)| MenuItem::with_id(app, id, label, true, None::<&str>));
        let menu = Menu::with_items(
            app,
            &[
                &open?,
                &settings?,
                &PredefinedMenuItem::separator(app)?,
                &restart?,
                &quit?,
            ],
        )?;
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
                        app::open_main(tray.app_handle());
                    }
                }
            });
        let decoded = image::load_from_memory(include_bytes!("../icons/tray-black.png"))
            .map_err(|error| tauri::Error::Anyhow(error.into()))?
            .into_rgba8();
        let (width, height) = decoded.dimensions();
        let image = tauri::image::Image::new_owned(decoded.into_raw(), width, height);
        builder = builder
            .icon(image)
            .icon_as_template(cfg!(target_os = "macos"));
        Ok(builder)
    }

    pub fn set_state(app: &AppHandle, state: TrayState) {
        let result = TRAY.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.is_none() && state != TrayState::None {
                *slot = Some(NativeTray {
                    prepared: Some(prepare(app)?),
                    icon: None,
                });
            }
            if let Some(tray) = slot.as_mut() {
                if tray.icon.is_none() && state == TrayState::Own {
                    tray.icon = Some(tray.prepared.take().expect("prepared tray").build(app)?);
                }
                if let Some(icon) = &tray.icon {
                    icon.set_visible(state == TrayState::Own)?;
                }
            }
            Ok::<(), tauri::Error>(())
        });
        if let Err(error) = result {
            TRAY.with(|slot| *slot.borrow_mut() = None);
            eprintln!("arcade-look: tray registration failed: {error}");
        }
    }
}
