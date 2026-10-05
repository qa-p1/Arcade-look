//! Arcade Link: Look's presence among the other Arcade apps.
//!
//! The manifest and the endpoint are set up on a background thread after
//! Tauri's setup, so startup never waits. With "Connect with other Arcade
//! apps" off, the manifest has no actions and nothing listens.
//!
//! Exposed actions: `look.preview` (files, batches navigable with ←/→,
//! folders, `file://` URLs), `look.inspect` (headless, also one-shot) and
//! `look.preview_selection` (Windows Explorer / macOS Finder; GNOME Files
//! can't be asked for its selection, so it is hidden on Linux).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use arcade_link::server::{Handler, InvokeContext, Reply};
use arcade_link::{
    ids, Action, Content, InvokeRequest, InvokeResult, LinkError, Locations, Manifest, Presence,
};
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::config::Config;
use crate::detect::Kind;

/// The manifest for this configuration (also printed by `--arcade-manifest`).
pub fn manifest(config: &Config) -> Manifest {
    let mut m = Manifest::new(
        ids::LOOK,
        env!("CARGO_PKG_VERSION"),
        &arcade_link::manifest::current_executable(),
    );
    m.launch.background = vec!["--background".into()];
    m.launch.invoke = Some(vec![arcade_link::oneshot::FLAG.into()]);
    if !config.global_shortcut.trim().is_empty() {
        m.shortcuts.push(arcade_link::manifest::Shortcut {
            id: "preview-selection".into(),
            accelerator: config.global_shortcut.clone(),
        });
    }
    m.settings.link_enabled = config.link_enabled;
    m.actions = actions();
    m
}

/// The actions Look exposes.
pub fn actions() -> Vec<Action> {
    vec![
        Action::new("look.preview", "Quick Look", "preview")
            .accepts(&["file/*", "file/*[]", "folder/reference", "text/url"])
            .effects(&["opens-ui"])
            .interactive(true),
        Action::new("look.inspect", "Inspect file", "inspect")
            .accepts(&["file/*", "folder/reference"])
            .produces(&["structured/file-info"]),
        Action::new(
            "look.preview_selection",
            "Preview the file manager's selection",
            "preview",
        )
        .effects(&["opens-ui"])
        .interactive(true)
        .platforms(&["windows", "macos"]),
    ]
    .into_iter()
    .filter(Action::on_this_platform)
    .collect()
}

/// The Link file kind for one of Look's kinds.
pub fn link_kind(kind: Kind) -> &'static str {
    match kind {
        Kind::Folder => "folder",
        Kind::Image | Kind::ImageDecode | Kind::ImageRaw | Kind::ImagePsd | Kind::Svg => "image",
        Kind::Video => "video",
        Kind::Audio => "audio",
        Kind::Pdf => "pdf",
        Kind::Markdown | Kind::Text => "text",
        Kind::Code | Kind::Json | Kind::Notebook | Kind::Html => "code",
        Kind::Csv | Kind::Spreadsheet => "spreadsheet",
        Kind::Document | Kind::Epub => "document",
        Kind::Presentation => "presentation",
        Kind::Archive => "archive",
        Kind::Font => "font",
        Kind::Model => "model",
        Kind::Binary => "any",
    }
}

/// The Link content type of a path, from Look's own detection.
pub fn link_type(path: &Path) -> String {
    match crate::detect::detect(path).kind {
        Kind::Folder => "folder/reference".into(),
        k => format!("file/{}", link_kind(k)),
    }
}

/// Paths from `look.preview`'s inputs; `file://` URLs become paths.
fn paths(request: &InvokeRequest) -> Result<Vec<PathBuf>, LinkError> {
    let mut out = Vec::new();
    for input in &request.inputs {
        if input.kind == "text/url" || input.kind == "text/plain" {
            let text = input.text.as_deref().unwrap_or_default().trim();
            match crate::util::normalize_arg(text, None).filter(|_| text.starts_with("file:")) {
                Some(p) => out.push(p),
                None => {
                    return Err(LinkError::unsupported(
                        "Arcade Look previews files; only file:// URLs work",
                    ))
                }
            }
        } else {
            out.extend(input.all_paths().into_iter().map(PathBuf::from));
        }
    }
    if out.is_empty() {
        return Err(LinkError::unsupported(
            "Arcade Look needs a file to preview",
        ));
    }
    if let Some(missing) = out.iter().find(|p| !p.exists()) {
        return Err(LinkError::unsupported(format!(
            "{} doesn't exist",
            missing.display()
        )));
    }
    Ok(out)
}

/// `look.inspect`: what Look knows about a file, within its read budgets.
pub fn inspect(path: &Path) -> Result<Value, LinkError> {
    let info =
        crate::commands::inspect_path(path, &[]).map_err(|e| LinkError::unsupported(e.clone()))?;
    let mut data = json!({
        "path": info.path,
        "name": info.name,
        "type": link_type(path),
        "kind": info.kind,
        "format": info.format,
        "mime": info.mime,
        "size": info.size,
        "modified": info.modified,
    });
    let extra = |data: &mut Value, key: &str, value: Value| {
        if !value.is_null() {
            data[key] = value;
        }
    };
    match info.kind {
        Kind::Image | Kind::ImageDecode | Kind::ImageRaw | Kind::ImagePsd | Kind::Svg => {
            if let Ok(i) = crate::imaging::info(path, &info.format) {
                extra(&mut data, "width", json!(i.width));
                extra(&mut data, "height", json!(i.height));
            }
        }
        Kind::Audio | Kind::Video => {
            if let Ok(a) = crate::media::info(path) {
                extra(&mut data, "durationMs", json!(a.duration_ms));
                if info.kind == Kind::Audio {
                    let tags = json!({
                        "title": a.title, "artist": a.artist, "album": a.album,
                        "year": a.year, "genre": a.genre,
                    });
                    extra(&mut data, "tags", tags);
                }
            }
        }
        Kind::Font => {
            if let Ok(f) = crate::font::info(path, &info.ext) {
                extra(&mut data, "family", json!(f.family));
            }
        }
        _ => {}
    }
    Ok(data)
}

fn on_main(app: &AppHandle, f: impl FnOnce(&AppHandle) + Send + 'static) -> Result<(), LinkError> {
    let handle = app.clone();
    app.run_on_main_thread(move || f(&handle))
        .map_err(|e| LinkError::internal(e.to_string()))
}

struct LookHandler {
    /// `None` in one-shot mode (no window).
    app: Option<AppHandle>,
}

impl LookHandler {
    fn app(&self) -> Result<&AppHandle, LinkError> {
        self.app.as_ref().ok_or_else(|| {
            LinkError::new(
                arcade_link::ErrorCode::NotRunning,
                "one-shot mode has no window",
            )
        })
    }
}

impl Handler for LookHandler {
    fn describe(&self) -> Vec<Action> {
        actions()
    }

    fn invoke(&self, request: InvokeRequest, _ctx: &InvokeContext) -> Result<Reply, LinkError> {
        match request.action.as_str() {
            "look.preview" => {
                let paths = paths(&request)?;
                let message = match paths.as_slice() {
                    [one] => format!(
                        "Previewing {}",
                        one.file_name().unwrap_or_default().to_string_lossy()
                    ),
                    many => format!("Previewing {} files", many.len()),
                };
                on_main(self.app()?, move |app| crate::app::open_batch(app, paths))?;
                Ok(Reply::Done(InvokeResult::message(message)))
            }
            "look.inspect" => {
                let input = request
                    .inputs
                    .first()
                    .and_then(|c| c.path.clone())
                    .ok_or_else(|| LinkError::unsupported("Arcade Look needs a file to inspect"))?;
                let data = inspect(Path::new(&input))?;
                Ok(Reply::Done(InvokeResult {
                    outputs: vec![Content::structured("file-info", data)],
                    message: None,
                    data: None,
                }))
            }
            "look.preview_selection" => {
                let app = self.app()?;
                match crate::integration::file_manager_selection() {
                    Some((path, source)) => {
                        on_main(app, move |app| {
                            crate::app::open(app, Some(path), source);
                        })?;
                        Ok(Reply::Done(InvokeResult::message(
                            "Previewing the selection",
                        )))
                    }
                    None => Err(LinkError::unavailable(
                        "no file is selected in the file manager",
                    )),
                }
            }
            other => Err(LinkError::unavailable(format!(
                "Arcade Look has no action {other}"
            ))),
        }
    }

    fn activate(&self) -> Result<(), LinkError> {
        on_main(self.app()?, crate::app::open_main)
    }

    fn quit(&self) -> Result<(), LinkError> {
        let app = self.app()?.clone();
        app.exit(0);
        Ok(())
    }
}

/// `--arcade-invoke`: serves one request from stdin without any window.
pub fn serve_oneshot() -> i32 {
    arcade_link::oneshot::serve(&LookHandler { app: None })
}

static PRESENCE: OnceLock<Mutex<Option<Arc<Presence>>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Arc<Presence>>> {
    PRESENCE.get_or_init(|| Mutex::new(None))
}

/// Starts Look's presence on a background thread.
pub fn start(app: &AppHandle, config: &Config) {
    let m = manifest(config);
    let handler = Arc::new(LookHandler {
        app: Some(app.clone()),
    });
    let _ = std::thread::Builder::new()
        .name("look-link".into())
        .spawn(move || {
            let p = Presence::start(Locations::discover(), m, handler);
            if let Some(e) = p.last_error() {
                crate::dbg_log!("arcade link: {e}");
            }
            *slot().lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(p));
        });
}

/// Rewrites the manifest after a configuration change.
pub fn refresh(config: &Config) {
    let m = manifest(config);
    let p = slot().lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(p) = p {
        std::thread::spawn(move || p.update(m));
    }
}

/// Stops listening and removes the endpoint file (the manifest stays).
pub fn stop() {
    if let Some(p) = slot().lock().unwrap_or_else(|e| e.into_inner()).take() {
        p.stop();
    }
}

pub fn last_error() -> Option<String> {
    slot()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|p| p.last_error())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_reports_image_dimensions_and_link_type() {
        let dir = std::env::temp_dir().join(format!("look-link-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("a.png");
        image::RgbaImage::new(3, 2).save(&png).unwrap();
        let d = inspect(&png).unwrap();
        assert_eq!(d["type"], "file/image");
        assert_eq!(
            (d["width"].as_u64(), d["height"].as_u64()),
            (Some(3), Some(2))
        );
        assert_eq!(link_type(&dir), "folder/reference");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn preview_accepts_file_urls_only() {
        let req =
            InvokeRequest::new("look.preview", "t").input(Content::url("https://example.com"));
        assert_eq!(
            paths(&req).unwrap_err().code,
            arcade_link::ErrorCode::UnsupportedInput
        );
        let here = std::env::current_dir().unwrap();
        let url = format!("file://{}", here.display());
        let req = InvokeRequest::new("look.preview", "t").input(Content::url(url));
        assert_eq!(paths(&req).unwrap(), vec![here]);
    }

    #[test]
    fn selection_preview_is_hidden_where_it_cannot_work() {
        let a = actions();
        let sel = a.iter().find(|a| a.id == "look.preview_selection");
        assert_eq!(sel.is_some(), cfg!(any(windows, target_os = "macos")));
    }
}
