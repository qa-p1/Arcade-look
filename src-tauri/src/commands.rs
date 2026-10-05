//! IPC commands called by the web UI. Heavy work runs on the blocking pool and panics are
//! turned into errors (`util::blocking`).

use crate::detect::{self, Kind};
use crate::util::{blocking, millis, normalize_arg, OrStr, Res};
use crate::{app, plugins};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    pub dir: Option<String>,
    pub ext: String,
    pub size: u64,
    pub modified: Option<i64>,
    pub created: Option<i64>,
    pub accessed: Option<i64>,
    pub readonly: bool,
    pub hidden: bool,
    pub symlink: Option<String>,
    pub mode: Option<u32>,
    pub kind: Kind,
    pub format: String,
    pub lang: Option<String>,
    pub mime: String,
    pub plugin: Option<plugins::Plugin>,
    pub fallback_plugin: Option<plugins::Plugin>,
}

fn path_arg(p: &str) -> Res<PathBuf> {
    normalize_arg(p, None).ok_or_else(|| "empty path".to_string())
}

pub fn inspect_path(path: &Path, plugins: &[plugins::Plugin]) -> Res<FileInfo> {
    let lmeta = std::fs::symlink_metadata(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => "This file no longer exists.".to_string(),
        std::io::ErrorKind::PermissionDenied => "Permission denied.".to_string(),
        _ => e.to_string(),
    })?;
    let symlink = lmeta.file_type().is_symlink().then(|| {
        std::fs::read_link(path)
            .map(|t| t.to_string_lossy().to_string())
            .unwrap_or_default()
    });
    let meta = std::fs::metadata(path).unwrap_or(lmeta);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    let d = detect::detect(path);
    let ext = if meta.is_dir() {
        String::new()
    } else {
        detect::extension_of(&name)
    };
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        Some(meta.permissions().mode())
    };
    #[cfg(not(unix))]
    let mode = None;
    let kind_str = serde_json::to_value(d.kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    let (over, fall) = plugins::matching(plugins, &ext, &kind_str);
    Ok(FileInfo {
        path: path.to_string_lossy().to_string(),
        dir: path.parent().map(|p| p.to_string_lossy().to_string()),
        hidden: crate::fsx::is_hidden(&name, path),
        name,
        ext,
        size: if meta.is_dir() { 0 } else { meta.len() },
        modified: millis(meta.modified()),
        created: millis(meta.created()),
        accessed: millis(meta.accessed()),
        readonly: meta.permissions().readonly(),
        symlink,
        mode,
        kind: d.kind,
        format: d.format,
        lang: d.lang,
        mime: d.mime,
        plugin: over.cloned(),
        fallback_plugin: fall.cloned(),
    })
}

#[tauri::command]
pub async fn inspect(state: State<'_, app::AppState>, path: String) -> Res<FileInfo> {
    let plugins = state.plugins.read().map(|p| p.clone()).unwrap_or_default();
    blocking(move || inspect_path(&path_arg(&path)?, &plugins)).await
}

#[tauri::command]
pub async fn read_text(
    state: State<'_, app::AppState>,
    path: String,
    max_bytes: Option<u64>,
) -> Res<crate::text::TextData> {
    let limit = state.config().text_limit_mb.max(1) << 20;
    let max = max_bytes.unwrap_or(limit).min(limit * 4);
    blocking(move || crate::text::read_text(&path_arg(&path)?, max)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Markdown {
    html: String,
    truncated: bool,
}

#[tauri::command]
pub async fn render_markdown(state: State<'_, app::AppState>, path: String) -> Res<Markdown> {
    let limit = state.config().text_limit_mb.max(1) << 20;
    blocking(move || {
        let t = crate::text::read_text(&path_arg(&path)?, limit)?;
        Ok(Markdown {
            html: crate::markdown::to_html(&t.text),
            truncated: t.truncated,
        })
    })
    .await
}

#[tauri::command]
pub async fn markdown_batch(sources: Vec<String>) -> Res<Vec<String>> {
    blocking(move || {
        Ok(sources
            .iter()
            .map(|s| crate::markdown::to_html(s))
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn read_bytes(path: String, offset: u64, length: u64) -> Res<tauri::ipc::Response> {
    blocking(move || {
        use std::io::{Read, Seek, SeekFrom};
        let mut f = std::fs::File::open(path_arg(&path)?).or_str()?;
        f.seek(SeekFrom::Start(offset)).or_str()?;
        let mut v = Vec::with_capacity(length.min(1 << 20) as usize);
        f.take(length.min(1 << 20)).read_to_end(&mut v).or_str()?;
        Ok(tauri::ipc::Response::new(v))
    })
    .await
}

#[tauri::command]
pub async fn list_archive(path: String, format: String) -> Res<crate::archive::Listing> {
    blocking(move || crate::archive::list(&path_arg(&path)?, &format)).await
}

#[tauri::command]
pub async fn extract_entry(path: String, format: String, entry: String) -> Res<String> {
    blocking(move || {
        Ok(crate::archive::extract(&path_arg(&path)?, &format, &entry)?
            .to_string_lossy()
            .to_string())
    })
    .await
}

#[tauri::command]
pub async fn read_table(
    path: String,
    format: String,
    sheet: Option<usize>,
) -> Res<crate::table::TableData> {
    blocking(move || {
        let p = path_arg(&path)?;
        match format.as_str() {
            "csv" | "tsv" | "psv" => crate::table::read_csv(&p, &format),
            _ => crate::table::read_sheet(&p, sheet.unwrap_or(0)),
        }
    })
    .await
}

#[tauri::command]
pub async fn read_document(path: String, format: String) -> Res<crate::office::DocOut> {
    blocking(move || crate::office::read(&path_arg(&path)?, &format)).await
}

#[tauri::command]
pub async fn read_slides(path: String, format: String) -> Res<crate::slides::Deck> {
    blocking(move || crate::slides::read(&path_arg(&path)?, &format)).await
}

#[tauri::command]
pub async fn font_info(path: String, ext: String) -> Res<crate::font::FontInfo> {
    blocking(move || crate::font::info(&path_arg(&path)?, &ext)).await
}

#[tauri::command]
pub async fn audio_info(path: String) -> Res<crate::media::AudioInfo> {
    blocking(move || crate::media::info(&path_arg(&path)?)).await
}

#[tauri::command]
pub async fn image_info(path: String, format: String) -> Res<crate::imaging::ImageInfo> {
    blocking(move || crate::imaging::info(&path_arg(&path)?, &format)).await
}

#[tauri::command]
pub async fn list_dir(path: String) -> Res<crate::fsx::DirListing> {
    blocking(move || crate::fsx::list_dir(&path_arg(&path)?)).await
}

#[tauri::command]
pub async fn dir_size(path: String) -> Res<crate::fsx::DirSize> {
    blocking(move || {
        Ok(crate::fsx::dir_size(
            &path_arg(&path)?,
            Duration::from_millis(1500),
        ))
    })
    .await
}

#[tauri::command]
pub async fn neighbor(app: AppHandle, path: String, delta: i64) -> Res<Option<String>> {
    if let Some(next) = app::batch_neighbor(&app, &path_arg(&path)?, delta) {
        return Ok(next);
    }
    blocking(move || crate::fsx::neighbor(&path_arg(&path)?, delta)).await
}

#[tauri::command]
pub async fn run_plugin(
    state: State<'_, app::AppState>,
    id: String,
    path: String,
) -> Res<plugins::PluginOutput> {
    let plugin = state
        .plugins
        .read()
        .ok()
        .and_then(|ps| ps.iter().find(|p| p.id == id).cloned())
        .ok_or("plugin not found")?;
    blocking(move || plugins::run(&plugin, &path_arg(&path)?)).await
}

#[tauri::command]
pub fn open_default(app: AppHandle, path: String) -> Res<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_path(path, None::<&str>).or_str()
}

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Res<()> {
    use tauri_plugin_opener::OpenerExt;
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("mailto:"))
    {
        return Err("only web and mail links can be opened".into());
    }
    app.opener().open_url(url, None::<&str>).or_str()
}

#[tauri::command]
pub fn reveal(app: AppHandle, path: String) -> Res<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener().reveal_item_in_dir(path).or_str()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    /// What to show first: "file" (`pending`), "welcome", "settings", or nothing (`None`).
    screen: Option<&'static str>,
    pending: Option<String>,
    platform: &'static str,
    version: &'static str,
    config: crate::config::Config,
    config_path: String,
    plugins_dir: String,
    info_panel: bool,
    service: bool,
    debug: bool,
    media_base: Option<String>,
    integration: Vec<(String, bool)>,
}

#[tauri::command]
pub fn bootstrap(state: State<'_, app::AppState>) -> Bootstrap {
    state
        .frontend_ready
        .store(true, std::sync::atomic::Ordering::SeqCst);
    crate::dbg_log!("frontend ready");
    let (screen, pending) = match state.pending.lock().ok().and_then(|mut p| p.take()) {
        Some(app::Request::File(p)) => (Some("file"), Some(p.to_string_lossy().to_string())),
        Some(app::Request::Welcome) => (Some("welcome"), None),
        Some(app::Request::Settings) => (Some("settings"), None),
        None => (None, None),
    };
    Bootstrap {
        screen,
        pending,
        platform: std::env::consts::OS,
        version: env!("CARGO_PKG_VERSION"),
        config: state.config(),
        config_path: crate::config::config_path().to_string_lossy().to_string(),
        plugins_dir: crate::config::plugins_dir().to_string_lossy().to_string(),
        info_panel: state
            .window_state
            .lock()
            .map(|s| s.info_panel)
            .unwrap_or(false),
        service: state.service.load(std::sync::atomic::Ordering::SeqCst),
        debug: crate::util::debug_enabled(),
        media_base: crate::mediaserver::base(),
        integration: crate::integration::status(),
    }
}

/// Frontend diagnostics, printed only with `ALOOK_DEBUG=1`.
#[tauri::command]
pub fn log(level: String, message: String) {
    crate::dbg_log!("ui {level}: {message}");
}

#[tauri::command]
pub fn show_window(app: AppHandle) {
    app::show(&app);
}

#[tauri::command]
pub fn hide_window(app: AppHandle) {
    app::hide(&app);
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn set_info_panel(state: State<'_, app::AppState>, open: bool) {
    if let Ok(mut s) = state.window_state.lock() {
        s.info_panel = open;
        crate::config::save_state(&s);
    }
}

/// ←/→ in the UI. When GNOME Files drives the preview, let it move its own selection
/// (it then calls ShowFile with the new file); returns false to navigate locally.
#[tauri::command]
pub fn navigate_external(app: AppHandle, delta: i64) -> bool {
    let state = app.state::<app::AppState>();
    let source = state
        .source
        .lock()
        .map(|s| *s)
        .unwrap_or(app::Source::Local);
    crate::integration::navigate(&app, source, delta)
}

#[tauri::command]
pub async fn install_integration() -> Res<String> {
    blocking(crate::integration::install).await
}

#[tauri::command]
pub async fn get_autostart() -> Res<bool> {
    blocking(|| Ok(crate::integration::autostart_enabled())).await
}

/// Turn "Start on login" on or off; returns the resulting state.
#[tauri::command]
pub async fn set_autostart(enabled: bool) -> Res<bool> {
    blocking(move || {
        crate::integration::set_autostart(enabled)?;
        Ok(crate::integration::autostart_enabled())
    })
    .await
}

#[tauri::command]
pub async fn integration_status() -> Res<Vec<(String, bool)>> {
    blocking(|| Ok(crate::integration::status())).await
}

/// Change settings from the settings screen; returns the new config.
#[tauri::command]
pub fn set_config(
    state: State<'_, app::AppState>,
    patch: serde_json::Map<String, serde_json::Value>,
) -> Res<crate::config::Config> {
    let config = crate::config::update(&state.config(), patch)?;
    if let Ok(mut c) = state.config.write() {
        *c = config.clone();
    }
    crate::link::refresh(&config);
    if let Ok(mut s) = state.config_stamp.lock() {
        *s = crate::config::stamp();
    }
    Ok(config)
}

#[tauri::command]
pub fn reload_plugins(state: State<'_, app::AppState>) -> usize {
    let list = plugins::load_all(state.config().plugins);
    let n = list.len();
    if let Ok(mut p) = state.plugins.write() {
        *p = list;
    }
    n
}
