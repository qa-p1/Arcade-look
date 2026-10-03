//! Windows: press Space in File Explorer (or on the desktop) to preview the selection.
//!
//! A low-level keyboard hook sees Space before Explorer does. We only swallow it when the
//! foreground window is Explorer *and* keyboard focus is in its item view (not the address
//! bar, the search box or a rename field), so typing is never affected. The selection is
//! read over COM (IShellWindows → IShellBrowser → IFolderView) on a worker thread, keeping
//! the hook callback instant.

use crate::app::{self, AppState, Source};
use crate::util::{OrStr, Res};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager};
use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, IServiceProvider, CLSCTX_ALL,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_SPACE,
};
use windows::Win32::UI::Shell::{
    IFolderView, IShellBrowser, IShellItemArray, IShellWindows, IWebBrowser2, SID_STopLevelBrowser,
    ShellWindows, CSIDL_DESKTOP, SIGDN_FILESYSPATH, SVGIO_SELECTION, SWC_DESKTOP,
    SWFO_NEEDDISPATCH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, FindWindowExW, GetClassNameW, GetForegroundWindow,
    GetGUIThreadInfo, GetMessageW, GetWindowThreadProcessId, SetWindowsHookExW, TranslateMessage,
    GUITHREADINFO, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, WH_KEYBOARD_LL, WM_KEYDOWN,
    WM_SYSKEYDOWN,
};

static HOOK_ACTIVE: AtomicBool = AtomicBool::new(false);
static SWALLOWING: AtomicBool = AtomicBool::new(false);
static WORKER: OnceLock<Mutex<Sender<isize>>> = OnceLock::new();

pub fn hook_active() -> bool {
    HOOK_ACTIVE.load(Ordering::SeqCst)
}

pub fn start_hook(app: AppHandle) {
    let (tx, rx) = channel::<isize>();
    if WORKER.set(Mutex::new(tx)).is_err() {
        return;
    }
    // Worker: resolves the selection over COM and opens the preview.
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        while let Ok(hwnd) = rx.recv() {
            let Some(path) = selection_of(HWND(hwnd as *mut _)) else {
                continue;
            };
            let state = app.state::<AppState>();
            let same = state.current.lock().ok().and_then(|c| c.clone()) == Some(path.clone());
            if same && state.is_visible() {
                app::hide(&app);
            } else {
                app::open(&app, Some(path), Source::Explorer);
            }
        }
    });
    // Hook thread: needs its own message loop.
    std::thread::spawn(|| unsafe {
        let Ok(hook) = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) else {
            eprintln!("arcade-look: could not install the Explorer keyboard hook");
            return;
        };
        HOOK_ACTIVE.store(true, Ordering::SeqCst);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = hook;
        HOOK_ACTIVE.store(false, Ordering::SeqCst);
    });
}

fn key_down(vk: u16) -> bool {
    unsafe { (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if kb.vkCode == VK_SPACE.0 as u32 && (kb.flags.0 & LLKHF_INJECTED.0) == 0 {
            let down = wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN;
            if down {
                if SWALLOWING.load(Ordering::SeqCst) {
                    return LRESULT(1); // auto-repeat while held
                }
                let modifiers = [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN]
                    .iter()
                    .any(|k| key_down(k.0));
                if !modifiers {
                    if let Some(hwnd) = explorer_item_view_focused() {
                        SWALLOWING.store(true, Ordering::SeqCst);
                        if let Some(tx) = WORKER.get().and_then(|m| m.lock().ok()) {
                            let _ = tx.send(hwnd.0 as isize);
                        }
                        return LRESULT(1);
                    }
                }
            } else if SWALLOWING.swap(false, Ordering::SeqCst) {
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// The foreground Explorer window if keyboard focus is in its file list.
fn explorer_item_view_focused() -> Option<HWND> {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() {
            return None;
        }
        let class = class_of(fg);
        if !matches!(
            class.as_str(),
            "CabinetWClass" | "ExploreWClass" | "Progman" | "WorkerW"
        ) {
            return None;
        }
        let tid = GetWindowThreadProcessId(fg, None);
        let mut gui = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(tid, &mut gui).ok()?;
        if !gui.hwndCaret.0.is_null() {
            return None; // a text field (rename, address bar, search) is being edited
        }
        let focus = class_of(gui.hwndFocus);
        matches!(focus.as_str(), "DirectUIHWND" | "SysListView32").then_some(fg)
    }
}

fn variant_i4(v: i32) -> VARIANT {
    let mut var = VARIANT::default();
    unsafe {
        (*var.Anonymous.Anonymous).vt = VT_I4;
        (*var.Anonymous.Anonymous).Anonymous.lVal = v;
    }
    var
}

/// Selected item in the Explorer window `fg` (or the desktop).
fn selection_of(fg: HWND) -> Option<PathBuf> {
    unsafe {
        let windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL).ok()?;
        let class = class_of(fg);
        if class == "Progman" || class == "WorkerW" {
            let loc = variant_i4(CSIDL_DESKTOP as i32);
            let empty = VARIANT::default();
            let mut hwnd = 0i32;
            let disp = windows
                .FindWindowSW(&loc, &empty, SWC_DESKTOP, &mut hwnd, SWFO_NEEDDISPATCH)
                .ok()?;
            let sp: IServiceProvider = disp.cast().ok()?;
            let browser: IShellBrowser = sp.QueryService(&SID_STopLevelBrowser).ok()?;
            return first_selected(&browser);
        }
        // Windows 11 tabs: the first ShellTabWindowClass child is the active tab.
        let tab_class: Vec<u16> = "ShellTabWindowClass\0".encode_utf16().collect();
        let active_tab =
            FindWindowExW(Some(fg), None, PCWSTR(tab_class.as_ptr()), PCWSTR::null()).ok();
        let count = windows.Count().ok()?;
        for i in 0..count {
            let Ok(disp) = windows.Item(&variant_i4(i)) else {
                continue;
            };
            let Ok(wb) = disp.cast::<IWebBrowser2>() else {
                continue;
            };
            let Ok(h) = wb.HWND() else { continue };
            if h.0 != fg.0 as isize {
                continue;
            }
            let Ok(sp) = wb.cast::<IServiceProvider>() else {
                continue;
            };
            let Ok(browser) = sp.QueryService::<IShellBrowser>(&SID_STopLevelBrowser) else {
                continue;
            };
            if let Some(tab) = active_tab {
                if let Ok(w) = browser.GetWindow() {
                    if w != tab {
                        continue;
                    }
                }
            }
            if let Some(p) = first_selected(&browser) {
                return Some(p);
            }
        }
        None
    }
}

unsafe fn first_selected(browser: &IShellBrowser) -> Option<PathBuf> {
    unsafe {
        let view = browser.QueryActiveShellView().ok()?;
        let fv: IFolderView = view.cast().ok()?;
        let items: IShellItemArray = fv.Items(SVGIO_SELECTION).ok()?;
        if items.GetCount().ok()? == 0 {
            return None;
        }
        let item = items.GetItemAt(0).ok()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = name.to_string().ok();
        CoTaskMemFree(Some(name.0 as *const _));
        s.map(PathBuf::from)
    }
}

/// Used by the global shortcut.
pub fn foreground_selection() -> Option<PathBuf> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let fg = GetForegroundWindow();
        let class = class_of(fg);
        if !matches!(
            class.as_str(),
            "CabinetWClass" | "ExploreWClass" | "Progman" | "WorkerW"
        ) {
            return None;
        }
        selection_of(fg)
    }
}

pub fn attach_console() {
    use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

pub fn status() -> Vec<(String, bool)> {
    vec![("Explorer Space key".into(), hook_active())]
}

fn reg(args: &[&str]) -> Res<()> {
    use std::os::windows::process::CommandExt;
    let st = std::process::Command::new("reg")
        .args(args)
        .creation_flags(0x0800_0000)
        .status()
        .or_str()?;
    if st.success() {
        Ok(())
    } else {
        Err(format!("reg {} failed", args.join(" ")))
    }
}

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

pub fn install() -> Res<String> {
    let exe = super::launcher_path();
    let exe = exe.to_string_lossy();
    reg(&[
        "add",
        RUN_KEY,
        "/v",
        "ArcadeLook",
        "/d",
        &format!("\"{exe}\" --service"),
        "/f",
    ])?;
    for base in [
        r"HKCU\Software\Classes\*\shell\ArcadeLook",
        r"HKCU\Software\Classes\Directory\shell\ArcadeLook",
    ] {
        reg(&["add", base, "/v", "MUIVerb", "/d", "Quick Look", "/f"])?;
        reg(&["add", base, "/v", "Icon", "/d", &format!("\"{exe}\""), "/f"])?;
        reg(&[
            "add",
            &format!(r"{base}\command"),
            "/ve",
            "/d",
            &format!("\"{exe}\" \"%1\""),
            "/f",
        ])?;
    }
    // Start the background listener now so Space works without logging out.
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new(&*exe)
        .arg("--service")
        .creation_flags(0x0800_0000)
        .spawn();
    Ok(
        "Installed: press Space on any file in File Explorer. Arcade Look now starts with Windows \
        in the background, and \"Quick Look\" is in the right-click menu."
            .into(),
    )
}

pub fn uninstall() -> Res<String> {
    let _ = reg(&["delete", RUN_KEY, "/v", "ArcadeLook", "/f"]);
    let _ = reg(&["delete", r"HKCU\Software\Classes\*\shell\ArcadeLook", "/f"]);
    let _ = reg(&[
        "delete",
        r"HKCU\Software\Classes\Directory\shell\ArcadeLook",
        "/f",
    ]);
    Ok("Removed Arcade Look autostart and context menu entries.".into())
}
