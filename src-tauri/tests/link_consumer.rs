//! Real consumer boundary runs. Execute this binary through Arcade-link's
//! isolated runner; the standalone suite keeps GUI/runtime checks ignored.
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use arcade_link::server::{Handler, InvokeContext, Reply, Server, ServerConfig};
use arcade_link::{
    ids, Action, Content, ErrorCode, InvokeRequest, InvokeResult, LinkError, Locations, Manifest,
    PeerInfo, Presence,
};
use arcade_look_lib::config::Config;
use arcade_look_lib::link_consumer::{file_content, me, page_content, Consumer};
use serde_json::{json, Value};

static NEXT: AtomicU64 = AtomicU64::new(0);
static ENVIRONMENT: Mutex<()> = Mutex::new(());
fn cli() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Arcade-link/target/debug/arcade-link")
}

struct Fixture {
    root: PathBuf,
    child: Option<Child>,
    app: String,
    previous_home: Option<std::ffi::OsString>,
    _environment: MutexGuard<'static, ()>,
}
impl Fixture {
    fn new(app: &str) -> Self {
        let environment = ENVIRONMENT.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(
            std::env::var("ARCADE_E2E_INNER").as_deref(),
            Ok("1"),
            "use the isolated e2e runner"
        );
        let root = std::env::temp_dir().join(format!(
            "look-consumer-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let previous_home = std::env::var_os("ARCADE_HOME");
        std::env::set_var("ARCADE_HOME", &root);
        Self {
            root,
            child: None,
            app: app.into(),
            previous_home,
            _environment: environment,
        }
    }
    fn locations(&self) -> Locations {
        Locations::under(&self.root)
    }
    fn start(&mut self, actions: Value, oneshot: bool) {
        self.stop();
        let path = self.root.join("fixture.json");
        std::fs::write(
            &path,
            json!({"id":self.app,"actions":actions,"oneshot":oneshot}).to_string(),
        )
        .unwrap();
        self.child = Some(
            Command::new(cli())
                .args(["mock", "--as", &self.app, "--actions"])
                .arg(&path)
                .env("ARCADE_HOME", &self.root)
                .env("ARCADE_MOCK_LOG", self.root.join("calls.jsonl"))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        wait(|| arcade_link::client::probe(&self.locations(), &self.app, &me()).is_some());
    }
    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    fn image(&self) -> Content {
        let path = self.root.join("input.png");
        image::RgbImage::new(8, 6).save(&path).unwrap();
        file_content(path.to_str().unwrap()).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = Command::new(cli())
            .args(["quit", &self.app, "--force"])
            .env("ARCADE_HOME", &self.root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        self.stop();
        let _ = std::fs::remove_dir_all(&self.root);
        match self.previous_home.take() {
            Some(home) => std::env::set_var("ARCADE_HOME", home),
            None => std::env::remove_var("ARCADE_HOME"),
        }
    }
}
fn wait(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "condition did not become true");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn action(behavior: Value) -> Value {
    json!({"id":"box:arcade.image.convert#webp","title":"Convert to WebP","preset":"webp",
        "accepts":["file/image"],"produces":["file/image"],"effects":["writes-files"],"featuredFor":["file/image"],"mock":behavior})
}

#[test]
#[ignore = "real peers: run with the isolated ecosystem runner"]
fn peers_first_later_missing_disabled_and_featured_limit() {
    let mut fixture = Fixture::new(ids::BOX);
    let content = fixture.image();
    let consumer = Consumer::new(fixture.locations());
    let config = Config::default();
    let connected = consumer.connected_peers(&config);
    assert_eq!(connected.len(), 6);
    assert!(connected
        .iter()
        .all(|peer| !peer.installed && peer.state == "Not installed"));
    assert!(!consumer.has_peers(&config));
    assert!(consumer.offers(&config, &content, false).is_empty());
    let (tx, rx) = mpsc::channel();
    assert!(consumer.watch(move || {
        let _ = tx.send(());
    }));
    let mut actions = vec![action(json!({}))];
    for n in 1..5 {
        let mut a = action(json!({}));
        a["id"] = json!(format!("box:arcade.image.convert#{n}"));
        a["preset"] = json!(n.to_string());
        actions.push(a);
    }
    actions.push(json!({"id":"box.open","title":"More","accepts":["*"],"interactive":true}));
    fixture.start(json!(actions), false);
    wait(|| {
        let _ = rx.recv_timeout(Duration::from_millis(10));
        consumer.offers(&config, &content, false).len() == 4
    });
    assert_eq!(
        Consumer::new(fixture.locations())
            .offers(&config, &content, false)
            .len(),
        4
    );
    assert!(consumer
        .connected_peers(&config)
        .iter()
        .find(|peer| peer.id == ids::BOX)
        .unwrap()
        .state
        .starts_with("Running · v"));
    let disabled = Config {
        link_disabled_peers: vec![ids::BOX.into()],
        ..config.clone()
    };
    assert!(consumer.offers(&disabled, &content, false).is_empty());
    assert!(
        !consumer
            .connected_peers(&disabled)
            .iter()
            .find(|peer| peer.id == ids::BOX)
            .unwrap()
            .enabled
    );
    let off = Config {
        link_enabled: false,
        ..config.clone()
    };
    assert!(consumer.offers(&off, &content, false).is_empty());
    assert_eq!(
        consumer
            .invoke(
                &disabled,
                (ids::BOX, "box:arcade.image.convert#webp"),
                content.clone(),
                &mut |_| {},
                &AtomicBool::new(false),
                Duration::from_secs(2)
            )
            .unwrap_err()
            .code,
        ErrorCode::Denied
    );
    let video = Content {
        kind: "file/video".into(),
        ..content
    };
    assert_eq!(consumer.offers(&config, &video, false).len(), 1); // only More
    fixture.stop();
    assert_eq!(
        consumer
            .connected_peers(&config)
            .iter()
            .find(|peer| peer.id == ids::BOX)
            .unwrap()
            .state,
        "Installed"
    );
    println!("missing peers: no entries; peers first/later: 3 featured + More; disabled: no IPC; wrong kind: presets hidden");
}

#[test]
#[ignore = "real peers: run with the isolated ecosystem runner"]
fn progress_outputs_cancel_timeout_and_crash() {
    let mut fixture = Fixture::new(ids::BOX);
    for (behavior, expected) in [
        (json!({"steps":3,"stepMs":20}), None),
        (json!({"steps":30,"stepMs":30}), Some(ErrorCode::Cancelled)),
        (json!({"steps":30,"stepMs":30}), Some(ErrorCode::Timeout)),
        (
            json!({"steps":30,"stepMs":30,"crashAfterMs":80}),
            Some(ErrorCode::NotRunning),
        ),
    ] {
        fixture.start(json!([action(behavior)]), false);
        let consumer = Consumer::new(fixture.locations());
        let input = fixture.image();
        let before = std::fs::read(input.path.as_ref().unwrap()).unwrap();
        let cancel = AtomicBool::new(false);
        let mut progress = 0;
        let result = consumer.invoke(
            &Config::default(),
            (ids::BOX, "box:arcade.image.convert#webp"),
            input.clone(),
            &mut |_| {
                progress += 1;
                if expected == Some(ErrorCode::Cancelled) {
                    cancel.store(true, Ordering::SeqCst);
                }
            },
            &cancel,
            if expected == Some(ErrorCode::Timeout) {
                Duration::from_millis(80)
            } else {
                Duration::from_secs(3)
            },
        );
        match expected {
            None => {
                let result = result.unwrap();
                assert_eq!(progress, 3);
                assert_eq!(result.outputs, vec![input.clone()]);
            }
            Some(code) => {
                let error = result.unwrap_err();
                assert_eq!(error.code, code);
                println!("{code:?}: {}", error.user_message("Arcade Box"));
            }
        }
        assert_eq!(std::fs::read(input.path.as_ref().unwrap()).unwrap(), before);
    }
    println!("3 progress updates + returned file; cancellation, bounded timeout, crash; source unchanged");
}

#[test]
#[ignore = "real peers: run with the isolated ecosystem runner"]
fn clipboard_size_unavailable_private_and_secret_guards() {
    let mut fixture = Fixture::new(ids::CLIPBOARD);
    fixture.start(json!([{"id":"clipboard.add","title":"Send","accepts":["file/*[]"],"maxBytes":16777216,"effects":["sends-to-device"]}]), false);
    let consumer = Consumer::new(fixture.locations());
    let mut content = fixture.image();
    content.size = Some(16777217);
    let offers = consumer.offers(&Config::default(), &content, false);
    assert_eq!(offers.len(), 1);
    assert!(offers[0].outbound);
    assert_eq!(
        offers[0].reason.as_deref(),
        Some("Too large to send to your devices (limit 16 MB).")
    );
    assert_eq!(
        consumer
            .invoke(
                &Config::default(),
                (ids::CLIPBOARD, "clipboard.add"),
                content,
                &mut |_| {},
                &AtomicBool::new(false),
                Duration::from_secs(2)
            )
            .unwrap_err()
            .code,
        ErrorCode::TooLarge
    );
    assert!(!fixture.root.join("calls.jsonl").exists());
    for (reason, message) in [
        ("private_mode", "Arcade Clipboard is in Private mode."),
        ("secret", "Not sent: this looks like a password or key."),
    ] {
        fixture.start(json!([{"id":"clipboard.add","title":"Send","accepts":["file/*[]"],"mock":{"error":"denied","reason":reason}}]), false);
        let consumer = Consumer::new(fixture.locations());
        let error = consumer
            .invoke(
                &Config::default(),
                (ids::CLIPBOARD, "clipboard.add"),
                fixture.image(),
                &mut |_| {},
                &AtomicBool::new(false),
                Duration::from_secs(2),
            )
            .unwrap_err();
        assert_eq!(error.user_message("Arcade Clipboard"), message);
        println!("{message}");
    }
    fixture.start(json!([{"id":"clipboard.add","title":"Send","accepts":["file/*[]"],"available":false,"reason":"No devices are paired"}]), false);
    let consumer = Consumer::new(fixture.locations());
    assert!(consumer
        .offers(&Config::default(), &fixture.image(), false)
        .is_empty());
    println!("16 MiB limit disables before IPC; unavailable hidden; owner rejects Private/secret requests verbatim");
}

#[test]
#[ignore = "real peers: run with the isolated ecosystem runner"]
fn stopped_peers_use_oneshot_and_interactive_launch() {
    let mut fixture = Fixture::new(ids::BOX);
    fixture.start(json!([action(json!({"steps":2,"stepMs":20})), {"id":"box.open","title":"More","accepts":["file/*"],"interactive":true}]), true);
    let consumer = Consumer::new(fixture.locations());
    fixture.stop();
    let result = consumer
        .invoke(
            &Config::default(),
            (ids::BOX, "box:arcade.image.convert#webp"),
            fixture.image(),
            &mut |_| {},
            &AtomicBool::new(false),
            Duration::from_secs(2),
        )
        .unwrap();
    assert_eq!(result.outputs[0].kind, "file/image");
    assert!(arcade_link::client::probe(&fixture.locations(), ids::BOX, &me()).is_none());
    let cancel = AtomicBool::new(false);
    let error = consumer
        .invoke(
            &Config::default(),
            (ids::BOX, "box:arcade.image.convert#webp"),
            fixture.image(),
            &mut |_| {
                cancel.store(true, Ordering::SeqCst);
            },
            &cancel,
            Duration::from_secs(2),
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Cancelled);
    let error = consumer
        .invoke(
            &Config::default(),
            (ids::BOX, "box:arcade.image.convert#webp"),
            fixture.image(),
            &mut |_| {},
            &AtomicBool::new(false),
            Duration::from_millis(5),
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Timeout);
    let result = consumer
        .invoke(
            &Config::default(),
            (ids::BOX, "box.open"),
            fixture.image(),
            &mut |_| {},
            &AtomicBool::new(false),
            Duration::from_secs(2),
        )
        .unwrap();
    assert_eq!(result.outputs[0].kind, "file/image");
    assert!(arcade_link::client::probe(&fixture.locations(), ids::BOX, &me()).is_some());
    println!("stopped headless peer: one-shot, no listener; stopped interactive peer: launched and connected");
}

#[test]
#[ignore = "real peers: run with the isolated ecosystem runner"]
fn wheel_gets_current_file_and_lens_handoffs_are_private_and_removed() {
    let mut fixture = Fixture::new(ids::WHEEL);
    fixture.start(
        json!([{"id":"wheel.add_action","title":"Add","accepts":["file/*"],"interactive":true}]),
        false,
    );
    let consumer = Consumer::new(fixture.locations());
    consumer
        .invoke(
            &Config::default(),
            (ids::WHEEL, "wheel.add_action"),
            fixture.image(),
            &mut |_| {},
            &AtomicBool::new(false),
            Duration::from_secs(2),
        )
        .unwrap();
    let call: Value = serde_json::from_str(
        std::fs::read_to_string(fixture.root.join("calls.jsonl"))
            .unwrap()
            .lines()
            .last()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(call["inputs"][0]["type"], "file/image");
    assert_eq!(call["context"]["source"], ids::LOOK);
    assert_eq!(call["context"]["interactive"], true);
    fixture.stop();
    fixture.app = ids::SHELF.into();
    fixture.start(
        json!([{"id":"shelf.add","title":"Add to Shelf","accepts":["file/*","text/plain"],"effects":["persists"]},
               {"id":"shelf.show","title":"Show Shelf","accepts":[]}]),
        false,
    );
    let consumer = Consumer::new(fixture.locations());
    let image = fixture.image();
    wait(|| {
        consumer
            .offers(&Config::default(), &image, false)
            .iter()
            .filter(|offer| offer.app == ids::SHELF)
            .map(|offer| (offer.action.as_str(), offer.title.as_str()))
            .eq([("shelf.add", "Add to Shelf")])
    });
    consumer
        .invoke(
            &Config::default(),
            (ids::SHELF, "shelf.add"),
            image.clone(),
            &mut |_| {},
            &AtomicBool::new(false),
            Duration::from_secs(2),
        )
        .unwrap();
    let call: Value = serde_json::from_str(
        std::fs::read_to_string(fixture.root.join("calls.jsonl"))
            .unwrap()
            .lines()
            .last()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(call["action"], "shelf.add");
    assert_eq!(call["inputs"][0]["type"], "file/image");
    assert_eq!(call["inputs"][0]["path"], image.path.as_deref().unwrap());
    fixture.stop();
    fixture.app = ids::LENS.into();
    fixture.start(json!([{"id":"lens.analyze","title":"Analyze","accepts":["file/image"],"interactive":true}]), false);
    let consumer = Consumer::new(fixture.locations());
    let png = std::fs::read(fixture.image().path.unwrap()).unwrap();
    let (handoff, input) = page_content(&fixture.locations(), &png).unwrap();
    let path = PathBuf::from(input.path.as_ref().unwrap());
    assert_eq!(input.owner.as_deref(), Some(ids::LOOK));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(handoff.dir())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
    consumer
        .invoke(
            &Config::default(),
            (ids::LENS, "lens.analyze"),
            input,
            &mut |_| {},
            &AtomicBool::new(false),
            Duration::from_secs(2),
        )
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), png);
    drop(handoff);
    assert!(!path.exists());
    assert!(page_content(&fixture.locations(), b"not a PNG").is_err());
    println!("Wheel receives file/image + interactive user-click context; Lens PNG handoff 0600/0700, read-only input, removed after completion");
}

struct Dynamic(Arc<Mutex<Vec<Action>>>);
impl Handler for Dynamic {
    fn describe(&self) -> Vec<Action> {
        self.0.lock().unwrap().clone()
    }
    fn invoke(&self, _: InvokeRequest, _: &InvokeContext) -> Result<Reply, LinkError> {
        Ok(Reply::Done(InvokeResult::default()))
    }
}
#[test]
#[ignore = "real peers: run with the isolated ecosystem runner"]
fn app_changed_refreshes_live_availability_without_manifest_rewrite() {
    let fixture = Fixture::new(ids::LENS);
    let actions = Arc::new(Mutex::new(vec![Action::new(
        "lens.analyze",
        "Analyze",
        "analyze",
    )
    .accepts(&["file/image"])]));
    let mut manifest = Manifest::new(ids::LENS, "1", cli().to_str().unwrap());
    manifest.actions = actions.lock().unwrap().clone();
    // A bare Server, so app.changed is sent without touching the manifest
    // (Presence::update notifies only when the manifest changes).
    arcade_link::manifest::write_manifest(&fixture.locations(), &manifest).unwrap();
    let server = Server::start(
        ServerConfig {
            app: PeerInfo {
                id: ids::LENS.into(),
                version: "1".into(),
            },
            locations: fixture.locations(),
        },
        Arc::new(Dynamic(actions.clone())),
    )
    .unwrap();
    let consumer = Consumer::new(fixture.locations());
    let (tx, rx) = mpsc::channel();
    assert!(consumer.watch(move || {
        let _ = tx.send(());
    }));
    let input = fixture.image();
    wait(|| {
        let _ = rx.recv_timeout(Duration::from_millis(10));
        consumer
            .offers(&Config::default(), &input, false)
            .first()
            .is_some_and(|o| o.reason.is_none())
    });
    // A direct server notification exercises describe, independent of disk discovery.
    actions.lock().unwrap()[0].available = false;
    actions.lock().unwrap()[0].reason = Some("Recognition is disabled".into());
    server.notify_changed();
    wait(|| {
        let _ = rx.recv_timeout(Duration::from_millis(10));
        consumer
            .offers(&Config::default(), &input, false)
            .is_empty()
    });
    let pdf = Content {
        kind: "file/pdf".into(),
        ..input
    };
    assert!(consumer.offers(&Config::default(), &pdf, false).is_empty());
    assert!(consumer.offers(&Config::default(), &pdf, true).is_empty());
    actions.lock().unwrap()[0].available = true;
    server.notify_changed();
    wait(|| {
        consumer
            .offers(&Config::default(), &pdf, true)
            .first()
            .is_some_and(|o| o.pdf_page)
    });
    drop(server);
    println!("app.changed -> live describe; PDF Lens action appears only with a page renderer");
}

#[test]
#[ignore = "real peers: run with the isolated ecosystem runner"]
fn saved_pipelines_are_cached_filtered_and_invoked_by_id() {
    let mut fixture = Fixture::new(ids::BOX);
    let pipelines = json!([
        {"id":"web","name":"Web image","version":1,"accepts":["file/image"],"effects":["writes-files"],"interactive":false},
        {"id":"capture","name":"Capture first","version":1,"accepts":["file/image"],"effects":["opens-ui"],"interactive":true},
        {"id":"video","name":"Video","version":1,"accepts":["file/video"],"effects":[],"interactive":false}
    ]);
    fixture.start(json!([
        {"id":"box.pipelines","title":"Pipelines","produces":["structured/pipelines"],"mock":{"result":{"outputs":[{"type":"structured/pipelines","data":pipelines}]}}},
        {"id":"box.pipeline.run","title":"Run","accepts":["file/*"],"effects":["writes-files"]}
    ]), false);
    let consumer = Consumer::new(fixture.locations());
    assert!(consumer.watch(|| {}));
    let content = fixture.image();
    let config = Config::default();
    wait(|| consumer.offers(&config, &content, false).len() == 1);
    let offer = consumer.offers(&config, &content, false).remove(0);
    assert_eq!(offer.title, "▶ Web image");
    assert_eq!(offer.pipeline.as_deref(), Some("web"));
    let calls = || std::fs::read_to_string(fixture.root.join("calls.jsonl")).unwrap();
    let before = calls();
    for _ in 0..50 {
        assert_eq!(consumer.offers(&config, &content, false).len(), 1);
    }
    assert_eq!(
        calls(),
        before,
        "opening the strip must never query pipelines"
    );
    let off = Config {
        link_disabled_peers: vec![ids::BOX.into()],
        ..config.clone()
    };
    assert!(consumer.offers(&off, &content, false).is_empty());
    consumer
        .invoke_with_pipeline(
            &config,
            (ids::BOX, "box.pipeline.run", Some("web")),
            content.clone(),
            &mut |_| {},
            &AtomicBool::new(false),
            Duration::from_secs(2),
        )
        .unwrap();
    let call: Value = serde_json::from_str(calls().lines().last().unwrap()).unwrap();
    assert_eq!(call["options"]["pipeline"], "web");
    for id in ["capture", "video", "deleted"] {
        assert_eq!(
            consumer
                .invoke_with_pipeline(
                    &config,
                    (ids::BOX, "box.pipeline.run", Some(id)),
                    content.clone(),
                    &mut |_| {},
                    &AtomicBool::new(false),
                    Duration::from_secs(2)
                )
                .unwrap_err()
                .code,
            ErrorCode::Unavailable
        );
    }
    println!("saved pipelines cached ahead of open; matching noninteractive entry only; options.pipeline preserved");
}

struct Pipelines(Vec<Action>);
impl Handler for Pipelines {
    fn describe(&self) -> Vec<Action> {
        self.0.clone()
    }
    fn invoke(&self, _: InvokeRequest, _: &InvokeContext) -> Result<Reply, LinkError> {
        let pipelines = json!([{"id":"web","name":"Web image","version":1,"accepts":["file/image"],"effects":["writes-files"],"interactive":false}]);
        Ok(Reply::Done(
            serde_json::from_value(
                json!({"outputs":[{"type":"structured/pipelines","data":pipelines}]}),
            )
            .unwrap(),
        ))
    }
}
#[test]
#[ignore = "real peers: run with the isolated ecosystem runner"]
fn pipelines_load_when_box_listens_after_its_manifest_appears() {
    let fixture = Fixture::new(ids::BOX);
    let mut manifest = Manifest::new(ids::BOX, "1", cli().to_str().unwrap());
    manifest.actions = vec![
        Action::new("box.pipelines", "Pipelines", "list").produces(&["structured/pipelines"]),
        Action::new("box.pipeline.run", "Run", "run").accepts(&["file/*"]),
    ];
    // Presence writes the manifest, then listens; Look can see the manifest
    // in between. Reproduce that gap deterministically.
    arcade_link::manifest::write_manifest(&fixture.locations(), &manifest).unwrap();
    let consumer = Consumer::new(fixture.locations());
    assert!(consumer.watch(|| {}));
    let content = fixture.image();
    let config = Config::default();
    std::thread::sleep(Duration::from_millis(300));
    assert!(consumer.offers(&config, &content, false).is_empty());
    let presence = Presence::start(
        fixture.locations(),
        manifest.clone(),
        Arc::new(Pipelines(manifest.actions.clone())),
    );
    assert!(presence.last_error().is_none());
    wait(|| {
        consumer
            .offers(&config, &content, false)
            .first()
            .is_some_and(|o| o.title == "▶ Web image")
    });
    presence.stop();
    println!("a pipeline fetch that ran before Box listened is retried once its endpoint appears");
}

#[test]
#[ignore = "real peers: run with the isolated ecosystem runner"]
fn get_hands_the_selected_app_to_tools_or_returns_releases() {
    let mut fixture = Fixture::new(ids::TOOLS);
    let consumer = Consumer::new(fixture.locations());
    assert_eq!(
        consumer.get(ids::BOX).unwrap().as_deref(),
        Some(arcade_link::manifest::releases_url(ids::BOX))
    );
    fixture.start(json!([{ "id":"tools.install", "title":"Install", "interactive":true, "effects":["opens-ui"] }]), false);
    consumer.registry.refresh();
    assert_eq!(consumer.get(ids::BOX).unwrap(), None);
    let calls = std::fs::read_to_string(fixture.root.join("calls.jsonl")).unwrap();
    let call: Value = serde_json::from_str(calls.lines().last().unwrap()).unwrap();
    assert_eq!(call["action"], "tools.install");
    assert_eq!(call["options"]["app"], ids::BOX);
    assert_eq!(call["context"]["interactive"], true);
    println!("Get passes options.app to tools.install; missing manager returns the canonical releases page");
}
