//! Linux: GNOME Files Space-bar integration via the NautilusPreviewer D-Bus protocol, and
//! Open-With / custom-action entries for GNOME Files, Dolphin, Nemo and others.
//!
//! GNOME Files calls `ShowFile(uri, windowHandle, closeIfShown[, activationToken])` on
//! `org.gnome.NautilusPreviewer` (the 4-argument form is newer). We answer both forms and
//! emit `SelectionEvent` so ←/→ move the selection in the file manager, like Sushi does.

use crate::app::{self, AppState, Source};
use crate::util::{normalize_arg, OrStr, Res};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};
use zbus::message::Type as MsgType;
use zbus::zvariant::Value;

const NAME: &str = "org.gnome.NautilusPreviewer";
const PATH: &str = "/org/gnome/NautilusPreviewer";
const IFACE2: &str = "org.gnome.NautilusPreviewer2";
const IFACE1: &str = "org.gnome.NautilusPreviewer";
const PROPS: &str = "org.freedesktop.DBus.Properties";

static CONN: OnceLock<zbus::Connection> = OnceLock::new();
static OWNS_NAME: AtomicBool = AtomicBool::new(false);
static VISIBLE: AtomicBool = AtomicBool::new(false);

const INTROSPECT: &str = r#"<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">
<node>
  <interface name="org.gnome.NautilusPreviewer2">
    <method name="ShowFile">
      <arg type="s" direction="in" name="uri"/>
      <arg type="s" direction="in" name="windowHandle"/>
      <arg type="b" direction="in" name="closeIfAlreadyShown"/>
      <arg type="s" direction="in" name="activationToken"/>
    </method>
    <method name="Close"/>
    <property name="ParentHandle" type="s" access="read"/>
    <property name="Visible" type="b" access="read"/>
    <signal name="SelectionEvent"><arg type="u" name="direction"/></signal>
  </interface>
  <interface name="org.freedesktop.DBus.Properties">
    <method name="Get"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
    <method name="GetAll"><arg type="s" direction="in"/><arg type="a{sv}" direction="out"/></method>
    <signal name="PropertiesChanged"><arg type="s"/><arg type="a{sv}"/><arg type="as"/></signal>
  </interface>
  <interface name="org.freedesktop.DBus.Introspectable">
    <method name="Introspect"><arg type="s" direction="out"/></method>
  </interface>
  <interface name="org.freedesktop.DBus.Peer">
    <method name="Ping"/>
    <method name="GetMachineId"><arg type="s" direction="out"/></method>
  </interface>
</node>"#;

pub fn start_previewer(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = serve(app).await {
            eprintln!("arcade-look: GNOME Files integration unavailable: {e}");
        }
    });
}

async fn serve(app: AppHandle) -> zbus::Result<()> {
    let conn = zbus::connection::Builder::session()?.build().await?;
    ensure_activatable(&conn).await;
    let reply = conn
        .request_name_with_flags(NAME, zbus::fdo::RequestNameFlags::DoNotQueue.into())
        .await;
    match reply {
        Ok(
            zbus::fdo::RequestNameReply::PrimaryOwner | zbus::fdo::RequestNameReply::AlreadyOwner,
        ) => {}
        _ => {
            // Another previewer (e.g. Sushi) is running; stay out of its way.
            return Ok(());
        }
    }
    OWNS_NAME.store(true, Ordering::SeqCst);
    let _ = CONN.set(conn.clone());
    let mut stream = zbus::MessageStream::from(&conn);
    while let Some(msg) = stream.next().await {
        let Ok(msg) = msg else { continue };
        if msg.message_type() != MsgType::MethodCall {
            continue;
        }
        if let Err(e) = dispatch(&app, &conn, &msg).await {
            eprintln!("arcade-look: D-Bus call failed: {e}");
        }
    }
    Ok(())
}

async fn dispatch(
    app: &AppHandle,
    conn: &zbus::Connection,
    msg: &zbus::Message,
) -> zbus::Result<()> {
    let hdr = msg.header();
    let path = hdr
        .path()
        .map(|p| p.as_str().to_string())
        .unwrap_or_default();
    let iface = hdr.interface().map(|i| i.as_str().to_string());
    let member = hdr
        .member()
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    let body = msg.body();

    match (iface.as_deref(), member.as_str()) {
        (Some("org.freedesktop.DBus.Peer"), "Ping") => return conn.reply(&hdr, &()).await,
        (Some("org.freedesktop.DBus.Peer"), "GetMachineId") => {
            let id = std::fs::read_to_string("/etc/machine-id").unwrap_or_default();
            return conn.reply(&hdr, &id.trim()).await;
        }
        _ => {}
    }
    if path != PATH {
        if member == "Introspect" {
            // Let introspection tools walk down to our object.
            let xml = r#"<node><node name="org"/></node>"#;
            let xml = match path.as_str() {
                "/" => xml.to_string(),
                "/org" => r#"<node><node name="gnome"/></node>"#.to_string(),
                "/org/gnome" => r#"<node><node name="NautilusPreviewer"/></node>"#.to_string(),
                _ => "<node/>".to_string(),
            };
            return conn.reply(&hdr, &xml).await;
        }
        return conn
            .reply_dbus_error(
                &hdr,
                zbus::fdo::Error::UnknownObject(format!("No object at {path}")),
            )
            .await;
    }

    match (iface.as_deref(), member.as_str()) {
        (Some(IFACE2) | Some(IFACE1) | None, "ShowFile") => {
            // zbus renders the body signature as a struct, e.g. "(ssbs)".
            let sig = body.signature().to_string();
            let sig = sig
                .trim_start_matches('(')
                .trim_end_matches(')')
                .to_string();
            let parsed: Option<(String, bool, String)> = match sig.as_str() {
                "ssbs" => body
                    .deserialize::<(String, String, bool, String)>()
                    .ok()
                    .map(|(u, _, c, t)| (u, c, t)),
                "ssb" => body
                    .deserialize::<(String, String, bool)>()
                    .ok()
                    .map(|(u, _, c)| (u, c, String::new())),
                "sib" => body
                    .deserialize::<(String, i32, bool)>()
                    .ok()
                    .map(|(u, _, c)| (u, c, String::new())),
                _ => None,
            };
            let Some((uri, close_if_shown, token)) = parsed else {
                return conn
                    .reply_dbus_error(
                        &hdr,
                        zbus::fdo::Error::InvalidArgs(format!("unexpected signature {sig}")),
                    )
                    .await;
            };
            conn.reply(&hdr, &()).await?;
            show_file(app, &uri, close_if_shown, token);
            Ok(())
        }
        (Some(IFACE2) | Some(IFACE1) | None, "Close") => {
            conn.reply(&hdr, &()).await?;
            let app = app.clone();
            let _ = app.clone().run_on_main_thread(move || app::hide(&app));
            Ok(())
        }
        (Some(PROPS), "Get") => {
            let (_, prop): (String, String) = body.deserialize().unwrap_or_default();
            match prop.as_str() {
                "Visible" => {
                    conn.reply(&hdr, &Value::from(VISIBLE.load(Ordering::SeqCst)))
                        .await
                }
                "ParentHandle" => conn.reply(&hdr, &Value::from("")).await,
                _ => {
                    conn.reply_dbus_error(&hdr, zbus::fdo::Error::UnknownProperty(prop))
                        .await
                }
            }
        }
        (Some(PROPS), "GetAll") => {
            let mut m: HashMap<&str, Value> = HashMap::new();
            m.insert("Visible", Value::from(VISIBLE.load(Ordering::SeqCst)));
            m.insert("ParentHandle", Value::from(""));
            conn.reply(&hdr, &m).await
        }
        (Some(PROPS), "Set") => {
            conn.reply_dbus_error(&hdr, zbus::fdo::Error::PropertyReadOnly("read-only".into()))
                .await
        }
        (Some("org.freedesktop.DBus.Introspectable"), "Introspect") => {
            conn.reply(&hdr, &INTROSPECT).await
        }
        _ => {
            conn.reply_dbus_error(
                &hdr,
                zbus::fdo::Error::UnknownMethod(format!("Unknown method {member}")),
            )
            .await
        }
    }
}

/// dbus-broker reads service files only at login or on ReloadConfig. If GNOME Files can't
/// reach (or start) the previewer even once, it switches to `NautilusPreviewerDevel` and never
/// tries us again until it restarts, so make sure our service file is known to the bus.
async fn ensure_activatable(conn: &zbus::Connection) {
    if !files().dbus.exists() {
        return;
    }
    let Ok(proxy) = zbus::fdo::DBusProxy::new(conn).await else {
        return;
    };
    let known = proxy
        .list_activatable_names()
        .await
        .is_ok_and(|names| names.iter().any(|n| n.as_str() == NAME));
    if !known {
        crate::dbg_log!("reloading D-Bus config so {NAME} is activatable");
        let _ = proxy.reload_config().await;
    }
}

fn show_file(app: &AppHandle, uri: &str, close_if_shown: bool, token: String) {
    let Some(path) = normalize_arg(uri, None) else {
        return;
    };
    let state = app.state::<AppState>();
    if close_if_shown && state.is_visible() {
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || app::hide(&app2));
        return;
    }
    if !token.is_empty() {
        if let Ok(mut t) = state.activation_token.lock() {
            *t = Some(token);
        }
    }
    app::open(app, Some(path), Source::Nautilus);
}

/// Ask GNOME Files to move its selection; it answers with ShowFile for the new file.
pub fn selection_event(delta: i64) -> bool {
    let Some(conn) = CONN.get().cloned() else {
        return false;
    };
    // GtkDirectionType: LEFT = 4, RIGHT = 5.
    let dir: u32 = if delta < 0 { 4 } else { 5 };
    tauri::async_runtime::spawn(async move {
        let _ = conn
            .emit_signal(
                None::<zbus::names::BusName>,
                PATH,
                IFACE2,
                "SelectionEvent",
                &(dir,),
            )
            .await;
    });
    true
}

pub fn visibility_changed(visible: bool) {
    VISIBLE.store(visible, Ordering::SeqCst);
    let Some(conn) = CONN.get().cloned() else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        let mut changed: HashMap<&str, Value> = HashMap::new();
        changed.insert("Visible", Value::from(visible));
        let _ = conn
            .emit_signal(
                None::<zbus::names::BusName>,
                PATH,
                PROPS,
                "PropertiesChanged",
                &(IFACE2, changed, Vec::<&str>::new()),
            )
            .await;
    });
}

/// Give the window the startup token GNOME Files passed so Wayland/X11 let it take focus.
pub fn apply_activation_token(w: &tauri::WebviewWindow, token: &str) {
    use gtk::prelude::GtkWindowExt;
    if let Ok(gw) = w.gtk_window() {
        gw.set_startup_id(token);
    }
}

/// Generation of the current size pin (0 = not pinned).
static PINNED: AtomicU64 = AtomicU64::new(0);
static PIN_GEN: AtomicU64 = AtomicU64::new(0);

/// Tiling compositors (Hyprland, Sway, i3, ...) tile ordinary windows but float fixed-size ones,
/// which they also center. Pin the size while the window maps so it opens as a centered popup,
/// then make it resizable again once it has focus (or after a second, whichever comes first).
pub fn pin_size_for_map(w: &tauri::WebviewWindow, width: f64, height: f64) {
    let gen = PIN_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    PINNED.store(gen, Ordering::SeqCst);
    let size = tauri::LogicalSize::new(width, height);
    let _ = w.set_min_size(Some(size));
    let _ = w.set_max_size(Some(size));
    let w = w.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(1));
        if PINNED.load(Ordering::SeqCst) == gen {
            unpin_size(&w);
        }
    });
}

pub fn unpin_size(w: &tauri::WebviewWindow) {
    if PINNED.swap(0, Ordering::SeqCst) == 0 {
        return;
    }
    let _ = w.set_max_size(None::<tauri::Size>);
    let _ = w.set_min_size(Some(tauri::LogicalSize::new(420.0, 300.0)));
}

fn data_home() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
}

struct Files {
    desktop: PathBuf,
    dbus: PathBuf,
    dolphin: PathBuf,
    nemo: PathBuf,
    icon: PathBuf,
}

fn files() -> Files {
    let d = data_home();
    Files {
        desktop: d.join("applications/arcade-look.desktop"),
        dbus: d.join("dbus-1/services/org.gnome.NautilusPreviewer.service"),
        dolphin: d.join("kio/servicemenus/arcade-look.desktop"),
        nemo: d.join("nemo/actions/arcade-look.nemo_action"),
        icon: d.join("icons/hicolor/256x256/apps/arcade-look.png"),
    }
}

fn autostart_file() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".config"))
        .join("autostart/arcade-look.desktop")
}

/// Quote a path as one argument of a desktop entry's `Exec` key. The spec applies string
/// unescaping (`\\` -> `\`) first, then shell-like quoting, and `%` starts a field code.
fn exec_quote(path: &std::path::Path) -> String {
    let mut out = String::from("\"");
    for c in path.to_string_lossy().chars() {
        match c {
            '"' | '`' | '$' => {
                out.push_str("\\\\");
                out.push(c);
            }
            '\\' => out.push_str("\\\\\\\\"),
            '%' => out.push_str("%%"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Inverse of `exec_quote` for the first argument of an `Exec` value.
fn exec_program(exec: &str) -> Option<PathBuf> {
    let rest = exec.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(PathBuf::from(out)),
            '\\' => {
                // `\\x` in the file is an escaped x; `\\\\` is a backslash.
                let next: String = chars.clone().take(3).collect();
                if next.starts_with("\\\\\\") {
                    out.push('\\');
                    chars.nth(2);
                } else if next.starts_with('\\') {
                    chars.next();
                    out.extend(chars.next());
                } else {
                    out.push(c);
                }
            }
            '%' => {
                if chars.clone().next() == Some('%') {
                    chars.next();
                }
                out.push('%');
            }
            _ => out.push(c),
        }
    }
    None
}

fn autostart_entry(exe: &std::path::Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Arcade Look\nComment=Keep Arcade Look ready in the background\nExec={} --service\nIcon=arcade-look\nTerminal=false\nX-GNOME-Autostart-enabled=true\nX-GNOME-Autostart-Delay=2\n",
        exec_quote(exe)
    )
}

pub fn autostart_enabled() -> bool {
    autostart_file().exists()
}

pub fn set_autostart(enabled: bool) -> Res<()> {
    let path = autostart_file();
    if !enabled {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(format!("remove {}: {e}", path.display()))
            }
            _ => Ok(()),
        };
    }
    let exe = super::launcher_path();
    // An entry pointing into a temporary folder stops working after a reboot.
    if ["/tmp/", "/var/tmp/", "/run/"]
        .iter()
        .any(|t| exe.starts_with(t))
    {
        return Err(format!(
            "Arcade Look is running from a temporary folder ({}). Move it somewhere permanent, \
            then turn on Start on login again.",
            exe.display()
        ));
    }
    write(&path, &autostart_entry(&exe), false)
}

/// If our autostart entry points at an executable that no longer exists (the app was moved,
/// or reinstalled from a different package), point it at this one.
pub fn refresh_autostart() {
    let path = autostart_file();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    if !text.contains("\nName=Arcade Look\n") {
        return;
    }
    let target = text
        .lines()
        .find_map(|l| l.strip_prefix("Exec="))
        .and_then(exec_program);
    if target.is_some_and(|t| !t.exists()) {
        crate::dbg_log!("autostart entry points to a missing executable, updating it");
        let _ = set_autostart(true);
    }
}

pub fn status() -> Vec<(String, bool)> {
    let f = files();
    vec![
        (
            "GNOME Files Space key".into(),
            OWNS_NAME.load(Ordering::SeqCst) || f.dbus.exists(),
        ),
        (
            "Open With menu".into(),
            f.desktop.exists()
                || PathBuf::from("/usr/share/applications/arcade-look.desktop").exists(),
        ),
    ]
}

const MIME_TYPES: &str = "application/octet-stream;text/plain;text/markdown;text/csv;text/html;application/json;application/pdf;application/zip;application/gzip;application/x-tar;application/x-7z-compressed;application/x-xz;application/x-bzip2;application/zstd;image/png;image/jpeg;image/gif;image/webp;image/svg+xml;image/bmp;image/tiff;image/x-tga;image/vnd.adobe.photoshop;image/avif;image/heif;image/x-canon-cr2;image/x-nikon-nef;image/x-adobe-dng;video/mp4;video/webm;video/quicktime;video/x-matroska;video/ogg;audio/mpeg;audio/flac;audio/ogg;audio/wav;audio/x-wav;audio/mp4;audio/opus;font/ttf;font/otf;font/woff;font/woff2;application/x-font-ttf;model/gltf-binary;model/gltf+json;model/stl;model/obj;application/vnd.openxmlformats-officedocument.wordprocessingml.document;application/vnd.openxmlformats-officedocument.spreadsheetml.sheet;application/vnd.openxmlformats-officedocument.presentationml.presentation;application/vnd.oasis.opendocument.text;application/vnd.oasis.opendocument.spreadsheet;application/vnd.oasis.opendocument.presentation;application/vnd.ms-excel;application/rtf;application/epub+zip;application/x-ipynb+json;inode/directory;";

fn write(path: &PathBuf, content: &str, executable: bool) -> Res<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ctx(&format!("create {}", dir.display()))?;
    }
    std::fs::write(path, content).ctx(&format!("write {}", path.display()))?;
    if executable {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

pub fn install() -> Res<String> {
    let exe = super::launcher_path();
    let exe_q = format!("\"{}\"", exe.display());
    let exec = exec_quote(&exe);
    let f = files();
    write(
        &f.desktop,
        &format!(
            "[Desktop Entry]\nType=Application\nName=Arcade Look\nGenericName=File Previewer\nComment=Preview any file instantly\nExec={exec} %U\nIcon=arcade-look\nTerminal=false\nCategories=Utility;Viewer;\nMimeType={MIME_TYPES}\nStartupNotify=true\nInitialPreference=1\nActions=settings;\n\n[Desktop Action settings]\nName=Settings\nExec={exec} --settings\n"
        ),
        false,
    )?;
    write(
        &f.dbus,
        &format!("[D-BUS Service]\nName={NAME}\nExec={exe_q} --service\n"),
        false,
    )?;
    write(
        &f.dolphin,
        &format!(
            "[Desktop Entry]\nType=Service\nMimeType=all/allfiles;inode/directory;\nActions=arcadeLook\nX-KDE-Priority=TopLevel\n\n[Desktop Action arcadeLook]\nName=Quick Look\nIcon=arcade-look\nExec={exec} %u\n"
        ),
        true,
    )?;
    write(
        &f.nemo,
        &format!(
            "[Nemo Action]\nName=Quick Look\nComment=Preview with Arcade Look\nExec={exec} %F\nIcon-Name=arcade-look\nSelection=s\nExtensions=any;dir;\n"
        ),
        false,
    )?;
    if let Some(dir) = f.icon.parent() {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(&f.icon, include_bytes!("../../icons/128x128@2x.png"));
    }
    tauri::async_runtime::block_on(async {
        if let Ok(conn) = zbus::Connection::session().await {
            ensure_activatable(&conn).await;
        }
    });
    let _ = std::process::Command::new("update-desktop-database")
        .arg(data_home().join("applications"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    // Like on Windows: stay ready in the background (with a tray icon) from the next login.
    let autostart = match set_autostart(true) {
        Ok(()) => "  • Arcade Look now starts in the background when you log in (change this in Settings).\n".to_string(),
        Err(e) => format!("  • Start on login could not be turned on: {e}\n"),
    };
    Ok(format!(
        "Installed:\n  • GNOME Files: press Space on any file (restart Files once: `nautilus -q`).\n    If GNOME Sushi is installed and running, Arcade Look takes over the next time it starts.\n  • Dolphin: right-click → Quick Look\n  • Nemo: right-click → Quick Look\n  • \"Open With → Arcade Look\" in any file manager\n{autostart}Files written under {}",
        data_home().display()
    ))
}

pub fn uninstall() -> Res<String> {
    let f = files();
    for p in [
        &f.desktop,
        &f.dbus,
        &f.dolphin,
        &f.nemo,
        &f.icon,
        &autostart_file(),
    ] {
        let _ = std::fs::remove_file(p);
    }
    Ok("Removed Arcade Look file manager integration and start on login.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_quoting_round_trips() {
        for p in [
            "/usr/bin/arcade-look",
            "/home/a b/Arcade \"Look\"/$x`y`/50%/c\\d",
        ] {
            let quoted = exec_quote(std::path::Path::new(p));
            assert_eq!(
                exec_program(&format!("{quoted} --service")),
                Some(PathBuf::from(p))
            );
        }
        assert_eq!(exec_quote(std::path::Path::new("/a\\b%")), r#""/a\\\\b%%""#);
    }
}
