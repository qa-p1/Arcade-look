//! Look's cached peer actions. Discovery, probes, handoffs and invocations run
//! on workers; opening the strip only reads the already-rendered entries.
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use arcade_link::client::{self, Client};
use arcade_link::content::accepts_content;
use arcade_link::wire::{method, JobDone, Kind, LineReader, Message};
use arcade_link::{
    ids, Action, Content, ErrorCode, Handoff, InvokeRequest, InvokeResult, JobProgress, LinkError,
    Locations, Manifest, PeerInfo, SharedRegistry,
};
use serde::Serialize;
use serde_json::json;
use tauri::{ipc::Channel, AppHandle, Emitter, Manager};

use crate::config::Config;
use crate::util::{blocking, Res};

const CLIPBOARD_LIMIT: u64 = 16 << 20;
const JOB_TIMEOUT: Duration = Duration::from_secs(30 * 60);
type Change = Arc<dyn Fn() + Send + Sync>;
type LiveActions = HashMap<String, (String, Vec<Action>)>;

pub struct Consumer {
    pub registry: SharedRegistry,
    pub locations: Locations,
    live: Mutex<LiveActions>,
    subscriptions: Mutex<HashMap<String, String>>,
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    last_error: Mutex<Option<String>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    pub app: String,
    pub action: String,
    pub title: String,
    pub reason: Option<String>,
    pub outbound: bool,
    pub pdf_page: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedPeer {
    pub id: String,
    pub name: String,
    pub state: String,
    pub installed: bool,
    pub enabled: bool,
    pub pitch: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedApps {
    peers: Vec<ConnectedPeer>,
    registry: String,
    endpoint: String,
    last_error: Option<String>,
}

pub fn me() -> PeerInfo {
    PeerInfo {
        id: ids::LOOK.into(),
        version: env!("CARGO_PKG_VERSION").into(),
    }
}

impl Consumer {
    /// Call off the UI thread.
    pub fn new(locations: Locations) -> Arc<Self> {
        Arc::new(Self {
            registry: SharedRegistry::load(&locations),
            locations,
            live: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(HashMap::new()),
            jobs: Mutex::new(HashMap::new()),
            last_error: Mutex::new(None),
        })
    }

    pub fn watch(self: &Arc<Self>, changed: impl Fn() + Send + Sync + 'static) -> bool {
        let changed: Change = Arc::new(changed);
        let consumer = Arc::downgrade(self);
        let notify = changed.clone();
        let watching = self.registry.watch(move |_| {
            if let Some(consumer) = consumer.upgrade() {
                consumer.subscribe_live(notify.clone());
                notify();
            }
        });
        self.subscribe_live(changed.clone());
        changed();
        watching
    }

    fn subscribe_live(self: &Arc<Self>, changed: Change) {
        for manifest in self.registry.snapshot().peers(ids::LOOK) {
            if !ids::APPS.contains(&manifest.id.as_str()) || !manifest.settings.link_enabled {
                continue;
            }
            let Ok(endpoint) = arcade_link::endpoint::read(&self.locations, &manifest.id) else {
                self.live.lock().unwrap().remove(&manifest.id);
                continue;
            };
            let id = manifest.id.clone();
            let token = endpoint.token;
            {
                let mut subscriptions = self.subscriptions.lock().unwrap();
                if subscriptions.get(&id) == Some(&token) {
                    continue;
                }
                subscriptions.insert(id.clone(), token.clone());
            }
            let consumer = self.clone();
            let notify = changed.clone();
            std::thread::spawn(move || {
                if let Ok(mut client) = Client::connect(&consumer.locations, &id, &me()) {
                    // Subscribe first: changes during describe remain queued.
                    if client.subscribe(&["app.changed"]).is_ok() {
                        loop {
                            if let Ok(actions) = client.describe() {
                                consumer
                                    .live
                                    .lock()
                                    .unwrap()
                                    .insert(id.clone(), (token.clone(), actions));
                                notify();
                            }
                            if client.next_notification(None).is_err() {
                                break;
                            }
                        }
                    }
                }
                let mut subscriptions = consumer.subscriptions.lock().unwrap();
                if subscriptions.get(&id) == Some(&token) {
                    subscriptions.remove(&id);
                    consumer.live.lock().unwrap().remove(&id);
                }
                drop(subscriptions);
                notify();
            });
        }
    }

    fn peers(&self) -> Vec<Manifest> {
        let mut peers: Vec<_> = self.registry.snapshot().peers(ids::LOOK).cloned().collect();
        let live = self.live.lock().unwrap().clone();
        let subscriptions = self.subscriptions.lock().unwrap();
        for peer in &mut peers {
            if let Some((token, actions)) = live.get(&peer.id) {
                if subscriptions.get(&peer.id) == Some(token) {
                    peer.actions.clone_from(actions);
                }
            }
        }
        peers
    }

    pub fn has_peers(&self, config: &Config) -> bool {
        config.link_enabled
            && self.peers().iter().any(|peer| {
                !config.link_disabled_peers.contains(&peer.id)
                    && peer.settings.link_enabled
                    && peer.actions.iter().any(Action::on_this_platform)
            })
    }

    /// Probe on a worker when settings opens or the watcher reports a change.
    pub fn connected_peers(&self, config: &Config) -> Vec<ConnectedPeer> {
        let registry = self.registry.snapshot();
        ids::APPS
            .iter()
            .filter(|id| **id != ids::LOOK)
            .map(|id| {
                let installed = registry.get(id).is_some();
                let state = match client::app_state(&self.locations, &registry, id, &me()) {
                    client::AppState::Running { version } => format!("Running · v{version}"),
                    client::AppState::Installed { .. } => "Installed".into(),
                    client::AppState::NotInstalled => "Not installed".into(),
                };
                ConnectedPeer {
                    id: (*id).into(),
                    name: arcade_link::manifest::app_name(id).into(),
                    state,
                    installed,
                    enabled: !config.link_disabled_peers.iter().any(|peer| peer == id),
                    pitch: arcade_link::manifest::app_pitch(id).into(),
                }
            })
            .collect()
    }

    pub fn shortcut_owner(&self, accelerator: &str) -> Option<String> {
        self.registry
            .with(|registry| registry.shortcut_owner(ids::LOOK, accelerator))
    }

    pub fn offers(&self, config: &Config, content: &Content, pdf_page: bool) -> Vec<Offer> {
        if !config.link_enabled {
            return Vec::new();
        }
        let mut offers = Vec::new();
        for peer in self.peers() {
            if !peer.settings.link_enabled || config.link_disabled_peers.contains(&peer.id) {
                continue;
            }
            let mut featured = 0;
            for action in &peer.actions {
                let page = peer.id == ids::LENS && content.kind == "file/pdf" && pdf_page;
                let mut input = content.clone();
                if page {
                    input.kind = "file/image".into();
                    input.size = None; // checked again against the actual PNG at invoke time
                }
                if !action.on_this_platform() || !accepts_content(&action.accepts, &input) {
                    continue;
                }
                let title = match (peer.id.as_str(), action.id.as_str()) {
                    (ids::BOX, "box.open") => "More in Arcade Box…",
                    (ids::BOX, _)
                        if action.preset.is_some()
                            && accepts_content(&action.featured_for, content)
                            && featured < 3 =>
                    {
                        featured += 1;
                        &action.title
                    }
                    (ids::CLIPBOARD, "clipboard.add") => "Send to my devices",
                    (ids::LENS, "lens.analyze") if input.kind == "file/image" => {
                        "Analyze with Lens"
                    }
                    (ids::WHEEL, "wheel.add_action") => "Add to Wheel",
                    _ => continue,
                };
                offers.push(Offer {
                    app: peer.id.clone(),
                    action: action.id.clone(),
                    title: title.into(),
                    reason: availability(&peer, action, &input)
                        .err()
                        .map(|e| e.user_message(&peer.name)),
                    outbound: action.has_effect("sends-to-device")
                        || action.has_effect("uploads-content")
                        || action.has_effect("network")
                        || action.privacy != "local",
                    pdf_page: page,
                });
            }
        }
        offers
    }

    pub fn invoke(
        &self,
        config: &Config,
        target: (&str, &str),
        content: Content,
        progress: &mut dyn FnMut(&JobProgress),
        cancel: &AtomicBool,
        timeout: Duration,
    ) -> Result<InvokeResult, LinkError> {
        let (app_id, action_id) = target;
        let peer = self
            .peers()
            .into_iter()
            .find(|peer| peer.id == app_id)
            .ok_or_else(|| LinkError::new(ErrorCode::NotInstalled, "no manifest"))?;
        if !config.link_enabled || config.link_disabled_peers.iter().any(|id| id == app_id) {
            return Err(LinkError::denied(arcade_link::error::reason::DISABLED));
        }
        let action = peer
            .action(action_id)
            .ok_or_else(|| LinkError::unavailable("action no longer offered"))?;
        availability(&peer, action, &content)?;
        let mut req = InvokeRequest::new(action_id, ids::LOOK).input(content);
        req.version = Some(action.version);
        req.context.interactive = true;
        let mut client = match Client::connect(&self.locations, app_id, &me()) {
            Ok(client) => client,
            Err(_) if !action.interactive && peer.launch.invoke.is_some() => {
                return oneshot(&peer, &req, progress, cancel, timeout);
            }
            Err(_) => client::launch_and_connect(&self.locations, &peer, &me())?,
        };
        // Recheck live availability on the same connection before sending content.
        let live = client.describe()?;
        let current = live
            .iter()
            .find(|a| a.id == action_id)
            .ok_or_else(|| LinkError::unavailable("action no longer offered"))?;
        availability(&peer, current, &req.inputs[0])?;
        if cancel.load(Ordering::SeqCst) {
            return Err(LinkError::cancelled());
        }
        invoke_connected(&mut client, &req, progress, cancel, timeout)
    }
}

fn availability(peer: &Manifest, action: &Action, input: &Content) -> Result<(), LinkError> {
    if !peer.settings.link_enabled {
        return Err(LinkError::denied(arcade_link::error::reason::DISABLED));
    }
    if !action.available {
        return Err(LinkError::unavailable(
            action.reason.clone().unwrap_or_default(),
        ));
    }
    if !action.on_this_platform() || !accepts_content(&action.accepts, input) {
        return Err(LinkError::unsupported("input does not match"));
    }
    let limit = if peer.id == ids::CLIPBOARD {
        Some(
            action
                .max_bytes
                .unwrap_or(CLIPBOARD_LIMIT)
                .min(CLIPBOARD_LIMIT),
        )
    } else {
        action.max_bytes
    };
    if let Some(limit) = limit {
        if input.size.is_some_and(|size| size > limit) {
            return Err(LinkError::too_large(limit));
        }
    }
    Ok(())
}

/// Bounded job wait using the shared client's wire API. The crate's default
/// wait_job retries timeouts forever, so Look enforces its deadline here.
pub fn invoke_connected(
    client: &mut Client,
    request: &InvokeRequest,
    progress: &mut dyn FnMut(&JobProgress),
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<InvokeResult, LinkError> {
    let result = client.call(
        method::INVOKE,
        serde_json::to_value(request).map_err(|e| LinkError::internal(e.to_string()))?,
    )?;
    let Some(job) = result.get("job").and_then(|v| v.as_str()) else {
        return serde_json::from_value(result).map_err(|e| LinkError::internal(e.to_string()));
    };
    let deadline = Instant::now() + timeout;
    loop {
        let stopped = if cancel.load(Ordering::SeqCst) {
            Some(LinkError::cancelled())
        } else if Instant::now() >= deadline {
            Some(LinkError::new(ErrorCode::Timeout, "job deadline exceeded"))
        } else {
            None
        };
        if let Some(error) = stopped {
            let _ = client.call(method::JOB_CANCEL, json!({"job": job}));
            return Err(error); // caller drops the connection, also cancelling on the server
        }
        let message = match client.next_notification(Some(Duration::from_millis(100))) {
            Ok(m) => m,
            Err(e) if e.code == ErrorCode::Timeout => continue,
            Err(_) => {
                return Err(LinkError::new(
                    ErrorCode::NotRunning,
                    "peer stopped while working",
                ))
            }
        };
        if message.params().get("job").and_then(|v| v.as_str()) != Some(job) {
            continue;
        }
        match message.method.as_deref() {
            Some(method::JOB_PROGRESS) => {
                if let Ok(p) = serde_json::from_value(message.params().clone()) {
                    progress(&p);
                }
            }
            Some(method::JOB_DONE) => {
                return serde_json::from_value::<JobDone>(message.params().clone())
                    .map_err(|e| LinkError::internal(e.to_string()))?
                    .into_result()
            }
            _ => {}
        }
    }
}

// The shared one-shot runner only checks cancellation after a quiet 100 ms.
// Look checks every message as well, so frequent progress cannot starve Cancel.
fn oneshot(
    peer: &Manifest,
    req: &InvokeRequest,
    progress: &mut dyn FnMut(&JobProgress),
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<InvokeResult, LinkError> {
    let line = Message::request(
        1,
        method::INVOKE,
        serde_json::to_value(req).map_err(|e| LinkError::internal(e.to_string()))?,
    )
    .to_line();
    let mut child = Command::new(&peer.executable)
        .args(peer.launch.invoke.as_ref().unwrap())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| LinkError::new(ErrorCode::LaunchFailed, e.to_string()))?;
    let deadline = Instant::now() + timeout;
    let result = (|| {
        child
            .stdin
            .take()
            .ok_or_else(|| LinkError::internal("no stdin"))?
            .write_all(line.as_bytes())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LinkError::internal("no stdout"))?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = LineReader::new(stdout);
            loop {
                let message = reader.read_message();
                let ended = !matches!(message, Ok(Some(_)));
                if tx.send(message).is_err() || ended {
                    break;
                }
            }
        });
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err(LinkError::cancelled());
            }
            if Instant::now() >= deadline {
                return Err(LinkError::new(ErrorCode::Timeout, "job deadline exceeded"));
            }
            let message = match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Ok(Some(message))) => message,
                Ok(Err(error)) => return Err(error),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                _ => {
                    return Err(LinkError::new(
                        ErrorCode::NotRunning,
                        "one-shot peer stopped while working",
                    ))
                }
            };
            if message.kind() == Kind::Response && message.id == Some(1) {
                if let Some(error) = message.error {
                    return Err(error);
                }
                return serde_json::from_value(message.result.unwrap_or_default())
                    .map_err(|e| LinkError::internal(e.to_string()));
            }
            if message.method.as_deref() == Some(method::JOB_PROGRESS) {
                if let Ok(p) = serde_json::from_value(message.params().clone()) {
                    progress(&p);
                }
            }
        }
    })();
    // Reap the one-shot even if it stalls after writing its response.
    let exit_deadline = Instant::now() + Duration::from_secs(1);
    while result.is_ok() && Instant::now() < exit_deadline {
        if child.try_wait().ok().flatten().is_some() {
            return result;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = child.kill();
    let _ = child.wait();
    result
}

pub fn file_content(path: &str) -> Result<Content, LinkError> {
    let path = Path::new(path)
        .canonicalize()
        .map_err(|e| LinkError::unsupported(e.to_string()))?;
    let meta = std::fs::metadata(&path).map_err(|e| LinkError::unsupported(e.to_string()))?;
    let mut content = Content::file(&path);
    content.kind = crate::link::link_type(&path);
    content.size = Some(if meta.is_file() { meta.len() } else { 0 });
    Ok(content)
}

pub fn page_content(locations: &Locations, png: &[u8]) -> Result<(Handoff, Content), LinkError> {
    if png.len() > CLIPBOARD_LIMIT as usize || !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(LinkError::unsupported("invalid PDF page image"));
    }
    let handoff = Handoff::create(locations, ids::LOOK)?;
    let content = handoff.file("page.png", png)?;
    Ok((handoff, content))
}

static CONSUMER: OnceLock<Arc<Consumer>> = OnceLock::new();
fn consumer() -> Arc<Consumer> {
    CONSUMER
        .get_or_init(|| Consumer::new(Locations::discover()))
        .clone()
}

pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let consumer = consumer();
        let notify = app.clone();
        if !consumer.watch(move || {
            let enabled = consumer_has_peers(&notify);
            let _ = notify.emit_to(crate::app::MAIN, "link-changed", enabled);
        }) {
            *consumer.last_error.lock().unwrap() = Some("Registry watch is unavailable.".into());
        }
    });
}

fn consumer_has_peers(app: &AppHandle) -> bool {
    consumer().has_peers(&app.state::<crate::app::AppState>().config())
}

pub fn refresh(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        consumer().registry.refresh();
        let _ = app.emit_to(crate::app::MAIN, "link-changed", consumer_has_peers(&app));
    });
}

#[tauri::command]
pub async fn link_available(app: AppHandle) -> Res<bool> {
    blocking(move || Ok(consumer_has_peers(&app))).await
}

#[tauri::command]
pub async fn link_actions(app: AppHandle, path: String, pdf_page: bool) -> Res<Vec<Offer>> {
    blocking(move || {
        let content = file_content(&path).map_err(|e| e.user_message("Arcade Look"))?;
        Ok(consumer().offers(
            &app.state::<crate::app::AppState>().config(),
            &content,
            pdf_page,
        ))
    })
    .await
}

#[tauri::command]
pub async fn link_invoke(
    app: AppHandle,
    app_id: String,
    action_id: String,
    path: String,
    request_id: String,
    pdf_png: Option<Vec<u8>>,
    progress: Channel<JobProgress>,
) -> Res<InvokeResult> {
    blocking(move || {
        let consumer = consumer();
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut jobs = consumer.jobs.lock().unwrap();
            if !jobs.is_empty() {
                return Err(LinkError::busy().user_message("Arcade Look"));
            }
            jobs.insert(request_id.clone(), cancel.clone());
        }
        let result = crate::util::catch(|| {
            let _ = progress.send(JobProgress {
                job: request_id.clone(),
                fraction: None,
                message: "Starting…".into(),
            });
            let config = app.state::<crate::app::AppState>().config();
            if !config.link_enabled || config.link_disabled_peers.contains(&app_id) {
                return Err(LinkError::denied(arcade_link::error::reason::DISABLED)
                    .user_message("Arcade Look"));
            }
            let mut content = file_content(&path).map_err(|e| e.user_message("Arcade Look"))?;
            let mut handoff = None;
            if let Some(png) = pdf_png {
                if app_id != ids::LENS || action_id != "lens.analyze" || content.kind != "file/pdf"
                {
                    return Err(LinkError::unsupported("a PDF page is only for Lens")
                        .user_message("Arcade Lens"));
                }
                let (h, page) = page_content(&consumer.locations, &png)
                    .map_err(|e| e.user_message("Arcade Lens"))?;
                content = page;
                handoff = Some(h);
            }
            let result = consumer
                .invoke(
                    &config,
                    (&app_id, &action_id),
                    content,
                    &mut |p| {
                        let _ = progress.send(p.clone());
                    },
                    &cancel,
                    JOB_TIMEOUT,
                )
                .map_err(|e| e.user_message(arcade_link::manifest::app_name(&app_id)));
            drop(handoff);
            result
        });
        consumer.jobs.lock().unwrap().remove(&request_id);
        if let Err(error) = &result {
            *consumer.last_error.lock().unwrap() = Some(error.clone());
        }
        result
    })
    .await
}

#[tauri::command]
pub fn link_cancel(request_id: String) {
    if let Some(consumer) = CONSUMER.get() {
        if let Some(cancel) = consumer.jobs.lock().unwrap().get(&request_id) {
            cancel.store(true, Ordering::SeqCst);
        }
    }
}

#[tauri::command]
pub async fn link_connected(app: AppHandle) -> Res<ConnectedApps> {
    blocking(move || {
        let consumer = consumer();
        let config = app.state::<crate::app::AppState>().config();
        let last_error = consumer
            .last_error
            .lock()
            .unwrap()
            .clone()
            .or_else(crate::link::last_error);
        Ok(ConnectedApps {
            peers: consumer.connected_peers(&config),
            registry: consumer.locations.registry.to_string_lossy().into_owned(),
            endpoint: if !config.link_enabled {
                "Connections off".into()
            } else if client::probe(&consumer.locations, ids::LOOK, &me()).is_some() {
                "Listening".into()
            } else {
                "Not listening".into()
            },
            last_error,
        })
    })
    .await
}

/// Get opens the installed manager; otherwise returns the canonical release
/// URL for the settings view to open through Look's existing URL guard.
#[tauri::command]
pub async fn link_get(app_id: String) -> Res<Option<String>> {
    blocking(move || {
        if !ids::APPS.contains(&app_id.as_str()) || app_id == ids::LOOK {
            return Err("Unknown Arcade app.".into());
        }
        let consumer = consumer();
        let registry = consumer.registry.snapshot();
        if let Some(manager) = registry.get(ids::TOOLS) {
            if let Ok(mut client) = Client::connect(&consumer.locations, ids::TOOLS, &me()) {
                if client.call(method::APP_ACTIVATE, json!({})).is_ok() {
                    return Ok(None);
                }
            }
            if client::spawn_detached(&manager.executable, &[]).is_ok() {
                return Ok(None);
            }
        }
        Ok(Some(arcade_link::manifest::releases_url(&app_id).into()))
    })
    .await
}

#[tauri::command]
pub async fn link_shortcut_owner(accelerator: String) -> Res<Option<String>> {
    blocking(move || Ok(consumer().shortcut_owner(&accelerator))).await
}

#[tauri::command]
pub fn link_shortcut_recording(recording: bool) {
    crate::integration::set_shortcut_recording(recording);
}

#[tauri::command]
pub async fn link_save_shortcut(app: AppHandle, accelerator: String) -> Res<Config> {
    let accelerator = blocking(move || {
        if !accelerator.is_empty() {
            accelerator
                .parse::<tauri_plugin_global_shortcut::Shortcut>()
                .map_err(|_| "This shortcut is not supported.".to_string())?;
        }
        Ok(accelerator)
    })
    .await?;
    crate::commands::set_config(
        app,
        serde_json::Map::from_iter([("globalShortcut".into(), json!(accelerator))]),
    )
    .await
}
