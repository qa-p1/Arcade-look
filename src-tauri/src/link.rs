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

use arcade_link::manifest::{ManifestAdditions, ManifestDocument};
use arcade_link::server::{Handler, InvokeContext, Reply};
use arcade_link::{
    ids, Action, Content, InvokeRequest, InvokeResult, LinkError, Locations, Manifest, Presence,
};
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::config::Config;
use crate::detect::Kind;

/// The complete manifest for this configuration (also printed by `--arcade-manifest`).
pub fn manifest(config: &Config) -> ManifestDocument {
    let mut m = Manifest::new(
        ids::LOOK,
        env!("CARGO_PKG_VERSION"),
        &arcade_link::manifest::current_executable(),
    );
    m.launch.background = vec!["--background".into()];
    m.launch.invoke = Some(vec![arcade_link::oneshot::FLAG.into()]);
    if let Ok(shortcut) = arcade_link::accelerator::normalize(&config.global_shortcut) {
        m.shortcuts.push(arcade_link::manifest::Shortcut {
            id: "preview-selection".into(),
            accelerator: shortcut,
        });
    }
    m.settings.link_enabled = config.link_enabled;
    m.actions = actions();
    let method = crate::installer::method();
    ManifestDocument {
        manifest: m,
        additions: ManifestAdditions {
            docs: Some(arcade_link::manifest::Docs {
                version: env!("CARGO_PKG_VERSION").into(),
            }),
            install: Some(method),
            ..Default::default()
        },
    }
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
        .produces(&["file/*[]"])
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
    background: bool,
}

fn resolved_selection(paths: Vec<PathBuf>) -> Result<InvokeResult, LinkError> {
    if paths.is_empty() {
        return Err(LinkError::unavailable(
            "Nothing is selected in the file manager.",
        ));
    }
    let outputs = paths
        .iter()
        .map(|path| crate::link_consumer::file_content(&path.to_string_lossy()))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(InvokeResult {
        message: Some(format!("{} files selected", outputs.len())),
        outputs,
        data: None,
    })
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
    fn methods(&self) -> Vec<String> {
        arcade_link::wire::method::OPTIONAL
            .iter()
            .map(|method| (*method).to_string())
            .collect()
    }

    fn status(&self) -> Value {
        json!({
            "mode": if self.background { "background" } else { "foreground" },
            "tray": self.app.as_ref().map(crate::tray::status).unwrap_or("none"),
        })
    }
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
                if request.options.get("resolveOnly").and_then(Value::as_bool) == Some(true) {
                    return resolved_selection(crate::integration::file_manager_selections())
                        .map(Reply::Done);
                }
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
                        "Nothing is selected in the file manager.",
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

    fn settings(&self) -> Result<(), LinkError> {
        on_main(self.app()?, crate::app::open_settings)
    }

    fn restart_ready(&self, mode: &str) -> Result<(), LinkError> {
        if mode != "force" && crate::link_consumer::busy() {
            return Err(LinkError::busy());
        }
        Ok(())
    }

    fn restart(&self, _mode: &str) -> Result<(), LinkError> {
        crate::tray::restart_request(self.app()?)
    }

    fn menu(&self) -> Result<Vec<arcade_link::manifest::MenuItem>, LinkError> {
        Ok(Vec::new())
    }

    fn menu_invoke(&self, id: &str) -> Result<(), LinkError> {
        Err(LinkError::unavailable(format!(
            "Arcade Look has no menu item {id}"
        )))
    }

    fn shortcuts_set(
        &self,
        id: &str,
        accelerator: &str,
    ) -> Result<arcade_link::app::ShortcutSetResult, LinkError> {
        if id != "preview-selection" {
            return Err(LinkError::denied("unknown shortcut id"));
        }
        let accelerator = if accelerator.is_empty() {
            String::new()
        } else {
            arcade_link::accelerator::normalize(accelerator)
                .map_err(|error| LinkError::denied(&error.to_string()))?
        };
        if let Some(owner) = crate::link_consumer::shortcut_owner(&accelerator) {
            return Err(LinkError::denied(&format!(
                "shortcut is already used by {owner}"
            )));
        }
        let patch = Map::from_iter([("globalShortcut".into(), Value::String(accelerator))]);
        let (_, result) = update_config(self.app()?, patch)?;
        result.ok_or_else(|| LinkError::internal("shortcut update did not return a result"))
    }

    fn settings_export(&self) -> Result<Vec<Content>, LinkError> {
        crate::portable_settings::export(&self.app()?.state::<crate::app::AppState>().config())
    }

    fn settings_import(
        &self,
        inputs: &[Content],
    ) -> Result<arcade_link::app::ImportResult, LinkError> {
        let document = crate::portable_settings::read_import(inputs)?;
        crate::portable_settings::backup_current()?;
        let _ = update_config(self.app()?, document.patch())?;
        Ok(arcade_link::app::ImportResult {
            imported: true,
            restart_required: false,
        })
    }
}

/// `--arcade-invoke`: serves one request from stdin without any window.
pub fn serve_oneshot() -> i32 {
    arcade_link::oneshot::serve(&LookHandler {
        app: None,
        background: false,
    })
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
        // Capture the startup mode once; showing a preview or a second
        // instance's --background must not change how Tools relaunches us.
        background: app
            .state::<crate::app::AppState>()
            .service
            .load(std::sync::atomic::Ordering::SeqCst),
    });
    let _ = std::thread::Builder::new()
        .name("look-link".into())
        .spawn(move || {
            let locations = Locations::discover();
            let p = Presence::start_document(locations, m, handler);
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
        std::thread::spawn(move || p.update_document(m));
    }
}

/// Apply one serialized settings change, updating the live shortcut before
/// writing and verifying the manifest. Restore both disk and native state on
/// any failed step.
pub fn update_config(
    app: &AppHandle,
    patch: Map<String, Value>,
) -> Result<(Config, Option<arcade_link::app::ShortcutSetResult>), LinkError> {
    let state = app.state::<crate::app::AppState>();
    let _change = state
        .settings_changes
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let old = state.config();
    let config_path = crate::config::config_path();
    let old_raw = std::fs::read(&config_path).ok();
    let mut preview = serde_json::to_value(&old).map_err(|e| LinkError::internal(e.to_string()))?;
    for (key, value) in &patch {
        preview[key] = value.clone();
    }
    let shortcut = preview
        .get("globalShortcut")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !shortcut.is_empty() {
        let canonical = arcade_link::accelerator::normalize(shortcut)
            .map_err(|e| LinkError::denied(&e.to_string()))?;
        preview["globalShortcut"] = Value::String(canonical);
    }
    let new: Config = serde_json::from_value(preview)
        .map_err(|e| LinkError::denied(&format!("invalid settings: {e}")))?;
    let shortcut_result = if new.global_shortcut != old.global_shortcut {
        Some(crate::integration::set_live_shortcut(
            app,
            &new.global_shortcut,
        )?)
    } else {
        None
    };

    let saved = match crate::config::update(&old, patch) {
        Ok(saved) => saved,
        Err(error) => {
            if shortcut_result.is_some() {
                let _ = crate::integration::set_live_shortcut(app, &old.global_shortcut);
            }
            return Err(LinkError::denied(&error));
        }
    };
    if let Err(error) = publish_config(&saved) {
        crate::portable_settings::restore_raw(&config_path, old_raw.as_deref());
        if let Some(old_shortcut) = shortcut_result.as_ref() {
            let _ = old_shortcut;
            let _ = crate::integration::set_live_shortcut(app, &old.global_shortcut);
        }
        let _ = publish_config(&old);
        return Err(error);
    }
    if let Ok(mut current) = state.config.write() {
        *current = saved.clone();
    }
    if let Ok(mut stamp) = state.config_stamp.lock() {
        *stamp = crate::config::stamp();
    }
    if let Ok(mut tray) = state.tray_watcher.lock() {
        if let Some(watcher) = tray.as_mut() {
            watcher.set_link_enabled(saved.link_enabled);
        }
    }
    crate::link_consumer::refresh(app);
    let _ = app.emit_to(crate::app::MAIN, "config", saved.clone());
    Ok((saved, shortcut_result))
}

fn publish_config(config: &Config) -> Result<(), LinkError> {
    let document = manifest(config);
    let presence = slot().lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(presence) = presence {
        presence.update_document(document.clone());
    } else {
        arcade_link::manifest::write_document(&Locations::discover(), &document)
            .map_err(|e| LinkError::unavailable(e.to_string()))?;
    }
    let path = Locations::discover().manifest(ids::LOOK);
    let text = std::fs::read_to_string(path).map_err(|e| LinkError::unavailable(e.to_string()))?;
    let mut actual =
        ManifestDocument::from_json(&text).map_err(|e| LinkError::unavailable(e.to_string()))?;
    actual.manifest.written_at = document.manifest.written_at.clone();
    if actual != document {
        return Err(LinkError::unavailable(
            "couldn't publish settings in the Arcade Link manifest",
        ));
    }
    Ok(())
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
        if let Some(selection) = sel {
            assert_eq!(selection.produces, ["file/*[]"]);
        }
    }

    #[test]
    fn status_preserves_the_startup_mode() {
        for (background, mode) in [(true, "background"), (false, "foreground")] {
            assert_eq!(
                LookHandler {
                    app: None,
                    background
                }
                .status()["mode"],
                mode
            );
        }
    }

    #[test]
    fn resolve_selection_returns_typed_paths_without_a_window() {
        let root = std::env::temp_dir().join(format!("look-selection-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let image = root.join("selected.png");
        image::RgbImage::new(2, 2).save(&image).unwrap();
        let text = root.join("selected.txt");
        std::fs::write(&text, "selected file").unwrap();
        let result = resolved_selection(vec![image.clone(), text]).unwrap();
        assert_eq!(result.message.as_deref(), Some("2 files selected"));
        assert_eq!(
            result
                .outputs
                .iter()
                .map(|o| o.kind.as_str())
                .collect::<Vec<_>>(),
            ["file/image", "file/text"]
        );
        // Paths come back canonical (macOS: /var is /private/var).
        let image = image.canonicalize().unwrap();
        assert_eq!(result.outputs[0].path.as_deref(), image.to_str());
        let error = resolved_selection(Vec::new()).unwrap_err();
        assert_eq!(error.code, arcade_link::ErrorCode::Unavailable);
        assert_eq!(
            error.reason.as_deref(),
            Some("Nothing is selected in the file manager.")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
