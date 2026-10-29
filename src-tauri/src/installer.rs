//! Look's policy around Arcade Link's receipts and install APIs: the
//! self-installing AppImage, the macOS move to Applications, and the
//! `--install`/`--uninstall`/`--repair`/`--integration-status` commands.
//!
//! Every operation takes an explicit Link environment, so tests and the e2e
//! runner can install into a temporary home.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use arcade_link::install::{self, AppInfo, Detection, Environment, Runtime, State};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use arcade_link::receipt::ManagedBy;
use arcade_link::receipt::{InstallMethod, Receipt};
use serde::{Deserialize, Serialize};

pub const ME: &str = arcade_link::ids::LOOK;
pub const NAME: &str = "Arcade Look";
pub const CLI_NAME: &str = "arcade-look";
pub const DESKTOP_ID: &str = "dev.arcade.Look";
const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn app_info() -> AppInfo {
    AppInfo {
        id: ME.into(),
        name: NAME.into(),
        filename: "Arcade-Look.AppImage".into(),
        desktop_id: DESKTOP_ID.into(),
        cli_name: CLI_NAME.into(),
        background_args: vec!["--service".into()],
        legacy_desktop_ids: vec!["arcade-look".into(), "app.arcadelook".into()],
        autostart_id: DESKTOP_ID.into(),
    }
}

/// How this copy was installed, for the manifest's `install` echo.
pub fn method() -> InstallMethod {
    match Environment::discover()
        .and_then(|e| Runtime::discover().and_then(|r| install::detect(&e, ME, &r)))
    {
        Ok(d) if d.state == State::DevBuild => InstallMethod::Dev,
        Ok(d) => d
            .receipt
            .as_ref()
            .filter(|_| d.state == State::InstalledHere)
            .map_or(
                if d.is_app_image {
                    InstallMethod::Appimage
                } else {
                    InstallMethod::Manual
                },
                |r| r.method,
            ),
        Err(_) => InstallMethod::Manual,
    }
}

/// Whether an installer owns this copy's launcher and login entries.
pub fn has_receipt() -> bool {
    Environment::discover()
        .ok()
        .and_then(|e| e.receipts().read(ME).ok().flatten())
        .is_some()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DialogState {
    Install,
    Update { from: String },
    OpenInstalled { version: String },
    MoveToApplications,
}

pub fn dialog_state(d: &Detection, version: &str) -> Option<DialogState> {
    match &d.state {
        State::DevBuild | State::InstalledHere => None,
        State::NotInstalled => Some(DialogState::Install),
        State::InstalledElsewhere { version: installed } => {
            let older = parse_version(installed)
                .zip(parse_version(version))
                .is_some_and(|(a, b)| a < b);
            if older {
                Some(DialogState::Update {
                    from: installed.clone(),
                })
            } else {
                Some(DialogState::OpenInstalled {
                    version: installed.clone(),
                })
            }
        }
    }
}

pub(crate) fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    Some((
        parts.next()??,
        parts.next()??,
        parts.next().flatten().unwrap_or(0),
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Options {
    pub channel: String,
    pub start_at_login: bool,
    pub remove_download: bool,
    pub install_cli: bool,
}

/// Whether the login entry will start Look. On Linux the desktop's startup
/// settings can switch an entry off without removing it.
fn login_entry_runs(path: &Path) -> bool {
    #[cfg(target_os = "linux")]
    {
        install::autostart_enabled(path)
    }
    #[cfg(not(target_os = "linux"))]
    {
        path.exists()
    }
}

impl Options {
    /// Look starts at login by default, like its first-run setup always did.
    pub fn defaults(environment: &Environment) -> Self {
        let receipt = environment.receipts().read(ME).ok().flatten();
        let start_at_login = receipt
            .as_ref()
            // Fresh installs start at login like every Arcade app; an existing
            // install reflects whether its entry will really run.
            .is_none_or(|r| {
                r.integration
                    .autostart
                    .as_ref()
                    .is_some_and(|p| login_entry_runs(p))
            });
        let install_cli = environment
            .path
            .as_ref()
            .is_some_and(|p| std::env::split_paths(p).any(|p| p == environment.bin));
        Self {
            channel: "stable".into(),
            start_at_login,
            remove_download: true,
            install_cli,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileIdentity {
    path: PathBuf,
    size: u64,
    modified: u128,
}

fn identity(path: &Path) -> io::Result<FileIdentity> {
    let meta = fs::metadata(path)?;
    Ok(FileIdentity {
        path: path.into(),
        size: meta.len(),
        modified: meta
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
    })
}

fn dismissed_file() -> PathBuf {
    crate::config::dir().join("install-dismissed.json")
}

pub fn dismissed(path: &Path) -> bool {
    identity(path).ok().is_some_and(|id| {
        fs::read(dismissed_file())
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<FileIdentity>>(&b).ok())
            .is_some_and(|v| v.contains(&id))
    })
}

/// "Don't ask again for this file": remembers this exact download.
pub fn dismiss(path: &Path) -> io::Result<()> {
    let id = identity(path)?;
    let file = dismissed_file();
    let mut ids: Vec<FileIdentity> = fs::read(&file)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    ids.retain(|old| old.path != id.path);
    if ids.len() >= 128 {
        ids.remove(0);
    }
    ids.push(id);
    arcade_link::paths::write_atomic(
        &file,
        &serde_json::to_vec(&ids).map_err(io::Error::other)?,
        true,
    )
}

/// Only real AppImages and macOS bundles outside Applications prompt. The
/// argument check runs first, so background and Link starts cost nothing.
pub fn automatic_prompt(
    args: &[String],
    environment: &Environment,
    runtime: &Runtime,
) -> io::Result<Option<(Detection, DialogState)>> {
    const SKIP: &[&str] = &[
        "--service",
        "--background",
        "--gapplication-service",
        "--arcade-invoke",
        "--arcade-manifest",
        "--version",
        "-V",
        "--install",
        "--uninstall",
        "--install-integration",
        "--uninstall-integration",
        "--repair",
        "--integration-status",
        "--install-dialog",
        "--quit",
        "--restart",
        "--help",
        "-h",
    ];
    if std::env::var("ARCADE_NO_INSTALL_PROMPT").as_deref() == Ok("1")
        || args.iter().skip(1).any(|a| SKIP.contains(&a.as_str()))
    {
        return Ok(None);
    }
    #[cfg(target_os = "linux")]
    {
        if !runtime.is_appimage() {
            return Ok(None);
        }
        let detection = install::detect(environment, ME, runtime)?;
        if !install::should_prompt(&detection, args, false, dismissed(&detection.running_from)) {
            return Ok(None);
        }
        Ok(dialog_state(&detection, VERSION).map(|s| (detection, s)))
    }
    #[cfg(target_os = "macos")]
    {
        use arcade_link::install::platform::macos;
        let Some(bundle) = macos::bundle_for(&runtime.executable) else {
            return Ok(None);
        };
        let mut detection = install::detect(environment, ME, runtime)?;
        if macos::in_applications(environment, &bundle) {
            if detection.state != State::DevBuild {
                if detection.receipt.is_none() {
                    environment.receipts().write(&macos_receipt(
                        environment,
                        &bundle,
                        None,
                        None,
                        "stable",
                    )?)?;
                } else {
                    refresh_bundle_version(environment, &bundle)?;
                }
            }
            return Ok(None);
        }
        let needs_move = macos::is_translocated(&bundle)
            || bundle.starts_with(environment.home.join("Downloads"))
            || bundle.starts_with("/Volumes")
            || fs::metadata(&bundle)?.permissions().readonly();
        if !needs_move || dismissed(&bundle) {
            return Ok(None);
        }
        let Some(state) = dialog_state(&detection, VERSION) else {
            return Ok(None);
        };
        let state = if state == DialogState::Install {
            DialogState::MoveToApplications
        } else {
            state
        };
        detection.running_from = bundle;
        Ok(Some((detection, state)))
    }
    #[cfg(windows)]
    {
        let _ = (environment, runtime);
        Ok(None)
    }
}

#[cfg(target_os = "linux")]
pub fn install_copy(
    environment: &Environment,
    runtime: &Runtime,
    options: &Options,
) -> io::Result<Receipt> {
    if !runtime.is_appimage() {
        // A second install window, after the first one installed (and removed) this download.
        if runtime.appimage.as_ref().is_some_and(|p| !p.exists()) {
            if let Some(receipt) = environment
                .receipts()
                .read(ME)?
                .filter(|r| r.version == VERSION && r.path.is_file())
            {
                return Ok(receipt);
            }
            return Err(io::Error::other(
                "This downloaded file is gone (it may have been installed from another window). Open NAME from the app launcher.".replace("NAME", NAME),
            ));
        }
        return Err(io::Error::other(
            "--install requires a real AppImage, not an ordinary or development binary",
        ));
    }
    struct Stage(PathBuf);
    impl Drop for Stage {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let stage = Stage(
        environment
            .data
            .join("arcade")
            .join(".install-icons")
            .join(arcade_link::endpoint::new_token()?),
    );
    arcade_link::paths::ensure_private_dir(&stage.0)?;
    let mut icons = Vec::new();
    for (size, png) in [
        (32u16, include_bytes!("../icons/32x32.png").as_slice()),
        (64, include_bytes!("../icons/64x64.png").as_slice()),
        (128, include_bytes!("../icons/128x128.png").as_slice()),
        (256, include_bytes!("../icons/128x128@2x.png").as_slice()),
        (512, include_bytes!("../icons/icon.png").as_slice()),
    ] {
        let source = stage.0.join(format!("{size}.png"));
        fs::write(&source, png)?;
        icons.push(install::Icon {
            source,
            size: install::IconSize::Pixels(size),
        });
    }
    let existing = environment.receipts().read(ME)?;
    let mut receipt = install::install(
        environment,
        install::InstallOptions {
            app: &app_info(),
            runtime,
            version: VERSION,
            channel: &options.channel,
            managed_by: existing
                .as_ref()
                .map_or(ManagedBy::SelfManaged, |r| r.managed_by),
            icons: &icons,
            start_at_login: options.start_at_login,
            remove_download: options.remove_download,
            refresh_caches: true,
        },
    )?;
    if !options.start_at_login {
        if let Some(path) = receipt.integration.autostart.take() {
            remove_file(&path)?;
            environment.receipts().write(&receipt)?;
        }
    }
    // The launcher entry keeps Look's MIME types and Settings action, and the
    // file-manager hooks (GNOME Files Space, Dolphin, Nemo) follow the copy.
    crate::integration::linux::write_installed_entries(environment, &receipt)
        .map_err(io::Error::other)?;
    Ok(receipt)
}

#[cfg(target_os = "macos")]
pub fn install_copy(
    environment: &Environment,
    runtime: &Runtime,
    options: &Options,
) -> io::Result<Receipt> {
    use arcade_link::install::platform::macos;
    let detection = install::detect(environment, ME, runtime)?;
    if detection.state == State::DevBuild {
        return Err(io::Error::other("development builds cannot be installed"));
    }
    let source = macos::bundle_for(&runtime.executable)
        .ok_or_else(|| io::Error::other("--install requires an app bundle"))?;
    let target = bundle_destination(environment, detection.receipt.as_ref(), "Arcade Look.app")?;
    let root = target.parent().unwrap().to_path_buf();
    fs::create_dir_all(&root)?;
    if fs::canonicalize(&source).ok() != fs::canonicalize(&target).ok() {
        let stage = BundleStage(root.join(format!(
            ".arcade-look-{}",
            arcade_link::endpoint::new_token()?
        )));
        let staged = macos::move_to_applications(environment, &source, Some(&stage.0))?;
        if target.exists() && detection.receipt.as_ref().is_none_or(|r| r.path != target) {
            return Err(io::Error::other(
                "Applications already contains an Arcade Look bundle without its receipt",
            ));
        }
        stage.replace(&staged, &target)?;
        drop(stage);
    }
    let executable = target.join("Contents/MacOS/arcade-look");
    let cli = if options.install_cli {
        Some(macos::install_cli(environment, CLI_NAME, &executable)?)
    } else {
        None
    };
    let autostart = detection
        .receipt
        .as_ref()
        .and_then(|r| r.integration.autostart.clone());
    let receipt = macos_receipt(environment, &target, cli, autostart, &options.channel)?;
    environment.receipts().write(&receipt)?;
    Ok(receipt)
}

#[cfg(windows)]
pub fn install_copy(
    _environment: &Environment,
    _runtime: &Runtime,
    _options: &Options,
) -> io::Result<Receipt> {
    Err(io::Error::other(
        "Use the Arcade Look per-user installer on Windows",
    ))
}

#[cfg(target_os = "macos")]
fn macos_receipt(
    environment: &Environment,
    bundle: &Path,
    cli: Option<PathBuf>,
    autostart: Option<PathBuf>,
    channel: &str,
) -> io::Result<Receipt> {
    let old = environment.receipts().read(ME)?;
    let now = arcade_link::manifest::now_rfc3339();
    Ok(Receipt {
        schema: 1,
        id: ME.into(),
        version: VERSION.into(),
        channel: channel.into(),
        method: InstallMethod::MacosBundle,
        managed_by: old
            .as_ref()
            .map_or(ManagedBy::SelfManaged, |r| r.managed_by),
        path: bundle.into(),
        integration: arcade_link::receipt::Integration {
            cli,
            autostart,
            ..Default::default()
        },
        previous: None,
        installed_at: old
            .as_ref()
            .map_or_else(|| now.clone(), |r| r.installed_at.clone()),
        updated_at: now,
    })
}

pub fn launch_path(receipt: &Receipt) -> PathBuf {
    if receipt.method == InstallMethod::MacosBundle {
        receipt.path.join("Contents/MacOS/arcade-look")
    } else {
        receipt.path.clone()
    }
}

/// A manually replaced bundle keeps its ownership and integration metadata.
#[cfg(any(target_os = "macos", test))]
fn refresh_bundle_version(environment: &Environment, bundle: &Path) -> io::Result<()> {
    if let Some(mut receipt) = environment.receipts().read(ME)? {
        if receipt.method == InstallMethod::MacosBundle
            && receipt.path == bundle
            && receipt.version != VERSION
        {
            receipt.version = VERSION.into();
            receipt.updated_at = arcade_link::manifest::now_rfc3339();
            environment.receipts().write(&receipt)?;
        }
    }
    Ok(())
}

/// Link compares CLI targets to the bundle path, while this link targets its executable.
#[cfg(any(target_os = "macos", all(test, unix)))]
fn bundle_cli(environment: &Environment) -> io::Result<Option<PathBuf>> {
    let Some(receipt) = environment.receipts().read(ME)? else {
        return Ok(None);
    };
    Ok(receipt.integration.cli.filter(|cli| {
        fs::read_link(cli).is_ok_and(|target| {
            target.starts_with(&receipt.path)
                && !target
                    .components()
                    .any(|c| c == std::path::Component::ParentDir)
        })
    }))
}

pub fn repair(environment: &Environment, runtime: &Runtime) -> io::Result<Receipt> {
    let detection = install::detect(environment, ME, runtime)?;
    #[cfg(target_os = "linux")]
    {
        let moved = (detection.state != State::DevBuild && detection.is_app_image)
            .then_some(detection.running_from.as_path());
        let receipt = install::repair(environment, &app_info(), moved)?;
        crate::integration::linux::write_installed_entries(environment, &receipt)
            .map_err(io::Error::other)?;
        Ok(receipt)
    }
    #[cfg(target_os = "macos")]
    {
        use arcade_link::install::platform::macos;
        let mut receipt = detection
            .receipt
            .ok_or_else(|| io::Error::other("no Arcade Look receipt"))?;
        if let Some(bundle) = macos::bundle_for(&runtime.executable) {
            if detection.state != State::DevBuild {
                receipt.path = bundle;
            }
        }
        let executable = launch_path(&receipt);
        if !executable.is_file() {
            return Err(io::Error::other(
                "installed Arcade Look executable is missing",
            ));
        }
        if receipt.integration.cli.is_some() {
            receipt.integration.cli = Some(macos::install_cli(environment, CLI_NAME, &executable)?);
        }
        receipt.updated_at = arcade_link::manifest::now_rfc3339();
        environment.receipts().write(&receipt)?;
        Ok(receipt)
    }
    #[cfg(windows)]
    {
        let _ = (detection, runtime);
        Err(io::Error::other(
            "Repair Arcade Look with its Windows installer",
        ))
    }
}

pub fn integration_status(environment: &Environment) -> io::Result<install::IntegrationStatus> {
    install::integration_status(environment, ME, &std::env::var("SHELL").unwrap_or_default())
}

/// Quits the running copy, then removes what the receipt lists.
fn quit_running_instance() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe)
            .arg("--quit")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

/// Before the freshly installed copy starts: an older one still running would
/// keep the single instance and the new copy would only show it. Asks over the
/// Link first (and waits for it to go), else with the app's own `--quit`.
pub fn stop_running_copy(environment: &Environment) {
    let quit = install::quit_running(
        &environment.locations,
        ME,
        std::time::Duration::from_secs(10),
    );
    if !matches!(quit, Ok(true)) {
        quit_running_instance();
    }
}

/// Tauri's app identifier (tauri.conf.json).
const IDENTIFIER: &str = "app.arcadelook";

pub fn uninstall(environment: &Environment, remove_data: bool) -> io::Result<bool> {
    quit_running_instance();
    #[cfg(target_os = "linux")]
    crate::integration::linux::remove_installed_entries(environment);
    let cache = dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
    // The WebView's storage and cache live under the app identifier.
    #[allow(unused_mut)]
    let mut data_folders = vec![
        crate::config::dir(),
        cache.join("arcade-look"),
        cache.join(IDENTIFIER),
    ];
    let dirs = arcade_link::paths::app_dirs(ME);
    data_folders.extend([dirs.config, dirs.data, dirs.cache]);
    data_folders.extend(
        arcade_link::paths::legacy_dirs(ME)
            .into_iter()
            .map(|legacy| legacy.path),
    );
    data_folders.extend(dirs::data_local_dir().map(|d| d.join(IDENTIFIER)));
    #[cfg(target_os = "macos")]
    data_folders.extend(dirs::home_dir().map(|h| h.join("Library/WebKit").join(IDENTIFIER)));
    #[cfg(target_os = "macos")]
    let cli = bundle_cli(environment)?;
    let removed = install::uninstall(
        environment,
        ME,
        install::UninstallOptions {
            remove_data,
            data_folders: &data_folders,
        },
    )?;
    #[cfg(target_os = "macos")]
    if let Some(cli) = cli {
        remove_file(&cli)?;
    }
    Ok(removed)
}

pub fn tools_installed(environment: &Environment) -> bool {
    arcade_link::Registry::load(&environment.locations)
        .get(arcade_link::ids::TOOLS)
        .is_some_and(|m| m.executable_exists() && m.settings.link_enabled)
}

/// Updates of a Tools-managed copy go through Tools (`tools.install`).
pub fn handoff_to_tools(environment: &Environment) -> Result<(), arcade_link::LinkError> {
    let registry = arcade_link::Registry::load(&environment.locations);
    let tools = registry
        .get(arcade_link::ids::TOOLS)
        .ok_or_else(|| arcade_link::LinkError::unavailable("Arcade Tools isn't installed"))?;
    let action = tools
        .action("tools.install")
        .filter(|a| a.available)
        .ok_or_else(|| {
            arcade_link::LinkError::unavailable("This version of Arcade Tools cannot install Look")
        })?;
    let request =
        arcade_link::InvokeRequest::new(action.id.split('#').next().unwrap_or(&action.id), ME)
            .input(arcade_link::Content::text("text/plain", ME))
            .options(serde_json::json!({ "app": ME }));
    let me = arcade_link::PeerInfo {
        id: ME.into(),
        version: VERSION.into(),
    };
    arcade_link::client::invoke_action(
        &environment.locations,
        &me,
        tools,
        &request,
        Default::default(),
    )
    .map(|_| ())
}

/// The original arguments minus the install flags, for the relaunch.
pub fn launch_args(args: &[String]) -> Vec<std::ffi::OsString> {
    let mut result = Vec::new();
    let mut it = args.iter().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--channel" => {
                it.next();
            }
            "--install" | "--install-integration" | "--silent" | "--no-launch" => {}
            _ => result.push(arg.into()),
        }
    }
    result
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn remove_file(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn json(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|e| e.to_string())
}

/// `--install [--silent] [--no-launch] [--keep-download] [--channel C]`.
pub fn install_cli(silent: bool, no_launch: bool, channel: &str) -> Result<String, String> {
    let environment = Environment::discover().map_err(|e| e.to_string())?;
    let runtime = Runtime::discover().map_err(|e| e.to_string())?;
    let mut options = Options::defaults(&environment);
    options.channel = channel.into();
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--keep-download") {
        options.remove_download = false;
    }
    let (output, launch) = if silent {
        let receipt = install_copy(&environment, &runtime, &options).map_err(|e| e.to_string())?;
        if let Some(hint) = integration_status(&environment)
            .map_err(|e| e.to_string())?
            .path_hint
        {
            eprintln!("arcade-look: {hint}");
        }
        (json(&receipt)?, Some(launch_path(&receipt)))
    } else {
        let detection = install::detect(&environment, ME, &runtime).map_err(|e| e.to_string())?;
        if detection.state == State::DevBuild {
            return Err(arcade_link::install::strings::get().errors["devBuild"].clone());
        }
        let state = dialog_state(&detection, VERSION).unwrap_or(DialogState::Install);
        match crate::installer_ui::run(environment.clone(), runtime, detection, state, options)? {
            crate::installer_ui::Outcome::Launch(path) => (String::new(), Some(path)),
            _ => (String::new(), None),
        }
    };
    if let Some(path) = launch.filter(|_| !no_launch) {
        // The installed copy replaces an older one still running.
        stop_running_copy(&environment);
        install::reexec(&path, &launch_args(&args)).map_err(|e| e.to_string())?;
    }
    Ok(output)
}

/// `--uninstall [--silent] [--remove-data]`.
pub fn uninstall_cli(silent: bool, remove_data: bool) -> Result<String, String> {
    let environment = Environment::discover().map_err(|e| e.to_string())?;
    if silent {
        let removed = uninstall(&environment, remove_data).map_err(|e| e.to_string())?;
        return Ok(serde_json::json!({ "uninstalled": removed }).to_string());
    }
    crate::installer_ui::run_uninstall(environment, remove_data)?;
    Ok(String::new())
}

pub fn repair_cli() -> Result<String, String> {
    let environment = Environment::discover().map_err(|e| e.to_string())?;
    let runtime = Runtime::discover().map_err(|e| e.to_string())?;
    json(&repair(&environment, &runtime).map_err(|e| e.to_string())?)
}

pub fn integration_status_cli() -> Result<String, String> {
    let environment = Environment::discover().map_err(|e| e.to_string())?;
    json(&integration_status(&environment).map_err(|e| e.to_string())?)
}

/// Before the first window: offer to install a downloaded copy. `true` when
/// this process is done (relaunched the installed copy, or handed to Tools).
pub fn first_run_prompt(args: &[String]) -> bool {
    let (Ok(environment), Ok(runtime)) = (Environment::discover(), Runtime::discover()) else {
        return false;
    };
    let (detection, state) = match automatic_prompt(args, &environment, &runtime) {
        Ok(Some(prompt)) => prompt,
        Ok(None) => return false,
        Err(e) => {
            eprintln!("arcade-look: install detection: {e}");
            return false;
        }
    };
    let options = Options::defaults(&environment);
    match crate::installer_ui::run(environment, runtime, detection, state, options) {
        Ok(crate::installer_ui::Outcome::Launch(path)) => match install::reexec(
            &path,
            &args.iter().skip(1).map(Into::into).collect::<Vec<_>>(),
        ) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("arcade-look: relaunch: {e}");
                false
            }
        },
        Ok(crate::installer_ui::Outcome::Exit) => true,
        Ok(crate::installer_ui::Outcome::Continue) => false,
        Err(e) => {
            eprintln!("arcade-look: install dialog: {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle_receipt(environment: &Environment) -> Receipt {
        Receipt {
            schema: 1,
            id: ME.into(),
            version: "0.1.0".into(),
            channel: "nightly".into(),
            method: InstallMethod::MacosBundle,
            managed_by: arcade_link::receipt::ManagedBy::Tools,
            path: environment.home.join("Applications/Arcade Look.app"),
            integration: arcade_link::receipt::Integration {
                cli: Some(environment.bin.join("cli")),
                autostart: Some(environment.home.join("Library/LaunchAgents/app.plist")),
                ..Default::default()
            },
            previous: Some(arcade_link::receipt::Previous {
                version: "0.0.9".into(),
                path: environment.home.join("previous/Arcade Look.app"),
            }),
            installed_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn manual_bundle_replacement_updates_only_the_owned_receipt_version() {
        let root = std::env::temp_dir().join(format!(
            "bundle-receipt-{}",
            arcade_link::endpoint::new_token().unwrap()
        ));
        let environment = Environment::under(&root);
        let original = bundle_receipt(&environment);
        refresh_bundle_version(&environment, &original.path).unwrap();
        assert!(environment.receipts().read(ME).unwrap().is_none());
        environment.receipts().write(&original).unwrap();
        refresh_bundle_version(&environment, &root.join("Other.app")).unwrap();
        assert_eq!(
            environment.receipts().read(ME).unwrap(),
            Some(original.clone())
        );
        refresh_bundle_version(&environment, &original.path).unwrap();
        let refreshed = environment.receipts().read(ME).unwrap().unwrap();
        assert_eq!(refreshed.version, VERSION);
        assert!(refreshed.updated_at > original.updated_at);
        let mut expected = original.clone();
        expected.version = refreshed.version.clone();
        expected.updated_at = refreshed.updated_at.clone();
        assert_eq!(refreshed, expected);
        refresh_bundle_version(&environment, &original.path).unwrap();
        assert_eq!(environment.receipts().read(ME).unwrap(), Some(refreshed));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn bundle_cli_cleanup_selects_only_links_into_the_receipted_bundle() {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!(
            "bundle-cli-{}",
            arcade_link::endpoint::new_token().unwrap()
        ));
        let environment = Environment::under(&root);
        let mut receipt = bundle_receipt(&environment);
        assert!(bundle_cli(&environment).unwrap().is_none());
        fs::create_dir_all(&environment.bin).unwrap();
        environment.receipts().write(&receipt).unwrap();
        let cli = receipt.integration.cli.clone().unwrap();
        fs::write(&cli, b"unrelated regular file").unwrap();
        assert!(bundle_cli(&environment).unwrap().is_none());
        fs::remove_file(&cli).unwrap();
        // Component-wise prefix matching must leave similarly named bundles alone.
        symlink(
            root.join("Applications/Arcade Look.app-other/Contents/MacOS/app"),
            &cli,
        )
        .unwrap();
        assert!(bundle_cli(&environment).unwrap().is_none());
        fs::remove_file(&cli).unwrap();
        symlink(receipt.path.join("../Other.app/executable"), &cli).unwrap();
        assert!(bundle_cli(&environment).unwrap().is_none());
        fs::remove_file(&cli).unwrap();
        // read_link also recognizes a dangling link after a manual bundle removal.
        symlink(receipt.path.join("Contents/MacOS/app"), &cli).unwrap();
        assert_eq!(bundle_cli(&environment).unwrap(), Some(cli.clone()));
        fs::remove_file(&cli).unwrap();
        assert!(bundle_cli(&environment).unwrap().is_none());
        receipt.integration.cli = None;
        environment.receipts().write(&receipt).unwrap();
        assert!(bundle_cli(&environment).unwrap().is_none());
        fs::remove_dir_all(root).unwrap();
    }

    fn detection(state: State) -> Detection {
        Detection {
            state,
            running_from: PathBuf::from("/test/Arcade-Look.AppImage"),
            is_app_image: true,
            receipt: None,
            installed_path: None,
        }
    }

    #[test]
    fn dialog_state_covers_new_update_open_and_dev_copies() {
        assert_eq!(
            dialog_state(&detection(State::NotInstalled), "0.2.0"),
            Some(DialogState::Install)
        );
        assert_eq!(
            dialog_state(
                &detection(State::InstalledElsewhere {
                    version: "0.1.2".into()
                }),
                "0.2.0"
            ),
            Some(DialogState::Update {
                from: "0.1.2".into()
            })
        );
        assert_eq!(
            dialog_state(
                &detection(State::InstalledElsewhere {
                    version: "0.2.0".into()
                }),
                "0.2.0"
            ),
            Some(DialogState::OpenInstalled {
                version: "0.2.0".into()
            })
        );
        assert_eq!(
            dialog_state(&detection(State::InstalledHere), "0.2.0"),
            None
        );
        assert_eq!(dialog_state(&detection(State::DevBuild), "0.2.0"), None);
    }

    #[test]
    fn relaunch_drops_install_flags() {
        let args: Vec<String> = [
            "look",
            "--install",
            "--silent",
            "--channel",
            "nightly",
            "a.txt",
        ]
        .map(String::from)
        .into();
        assert_eq!(launch_args(&args), vec![std::ffi::OsString::from("a.txt")]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn appimage_install_repair_and_uninstall_use_a_temp_environment() {
        let root = std::env::temp_dir().join(format!(
            "look-install-{}",
            arcade_link::endpoint::new_token().unwrap()
        ));
        let env = Environment::under(&root.join("home"));
        let appdir = root.join("mount");
        let exe = appdir.join("usr/bin/arcade-look");
        let image = root.join("Arcade-Look.AppImage");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, b"test binary").unwrap();
        fs::write(&image, b"test appimage").unwrap();
        let runtime = Runtime {
            executable: exe,
            appimage: Some(image),
            appdir: Some(appdir),
            dev_build: false,
        };
        let opts = Options {
            channel: "stable".into(),
            start_at_login: true,
            remove_download: false,
            install_cli: false,
        };
        let receipt = install_copy(&env, &runtime, &opts).unwrap();
        assert!(receipt.path.is_file());
        let desktop =
            fs::read_to_string(receipt.integration.desktop_entry.as_ref().unwrap()).unwrap();
        assert!(desktop.contains("MimeType=") && desktop.contains("X-Arcade-Id=arcade.look"));
        assert!(receipt.integration.autostart.as_ref().unwrap().is_file());
        let repaired = install::repair(&env, &app_info(), None).unwrap();
        assert_eq!(repaired.path, receipt.path);
        assert!(install::uninstall(
            &env,
            ME,
            install::UninstallOptions {
                remove_data: false,
                data_folders: &[]
            }
        )
        .unwrap());
        assert!(env.receipts().read(ME).unwrap().is_none());
        assert!(!receipt.path.exists());
        fs::remove_dir_all(root).unwrap();
    }
}

// Keep an adopted native installation in the Applications folder the user chose.
#[cfg(any(target_os = "macos", all(test, unix)))]
fn bundle_destination(
    environment: &Environment,
    receipt: Option<&Receipt>,
    name: &str,
) -> io::Result<PathBuf> {
    if let Some(receipt) = receipt.filter(|r| r.method == InstallMethod::MacosBundle) {
        receipt.validate_native_paths()?;
        if receipt.path.file_name().is_some_and(|n| n == name)
            && receipt.path.parent().is_some_and(|p| {
                p == environment.home.join("Applications") || p == Path::new("/Applications")
            })
        {
            return Ok(receipt.path.clone());
        }
        return Err(io::Error::other(
            "the recorded bundle is outside its Applications folder",
        ));
    }
    Ok(environment.home.join("Applications").join(name))
}

#[cfg(any(target_os = "macos", all(test, unix)))]
struct BundleStage(PathBuf);
#[cfg(any(target_os = "macos", all(test, unix)))]
impl BundleStage {
    fn replace(&self, staged: &Path, target: &Path) -> io::Result<()> {
        let backup = self.0.join("previous.app");
        let had_old = target.exists();
        if had_old {
            fs::rename(target, &backup)?;
        }
        if let Err(error) = fs::rename(staged, target) {
            if had_old {
                fs::rename(&backup, target)?;
            }
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(any(target_os = "macos", all(test, unix)))]
impl Drop for BundleStage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(all(test, unix))]
mod bundle_destination_tests {
    use super::*;

    #[test]
    fn failed_bundle_replacement_restores_the_old_app_and_removes_staging() {
        let root = std::env::temp_dir().join(format!(
            "arcade-bundle-failure-{}",
            arcade_link::endpoint::new_token().unwrap()
        ));
        let target = root.join("Installed.app");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("sentinel"), b"old app").unwrap();
        let stage_path = root.join(".arcade-stage");
        {
            let stage = BundleStage(stage_path.clone());
            fs::create_dir_all(&stage.0).unwrap();
            assert!(stage
                .replace(&stage.0.join("missing.app"), &target)
                .is_err());
            assert_eq!(fs::read(target.join("sentinel")).unwrap(), b"old app");
        }
        assert!(!stage_path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_updates_keep_the_adopted_applications_folder() {
        let environment =
            Environment::under(&std::env::temp_dir().join("arcade-bundle-destination"));
        let name = "Arcade Look.app";
        let default = environment.home.join("Applications").join(name);
        assert_eq!(
            bundle_destination(&environment, None, name).unwrap(),
            default
        );
        let mut receipt = Receipt {
            schema: 1,
            id: "arcade.look".into(),
            version: "0.2.0".into(),
            channel: "stable".into(),
            method: InstallMethod::MacosBundle,
            managed_by: ManagedBy::Tools,
            path: Path::new("/Applications").join(name),
            integration: Default::default(),
            previous: None,
            installed_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        };
        assert_eq!(
            bundle_destination(&environment, Some(&receipt), name).unwrap(),
            receipt.path
        );
        receipt.path = default.clone();
        assert_eq!(
            bundle_destination(&environment, Some(&receipt), name).unwrap(),
            default
        );
        receipt.path = environment.home.join("Downloads").join(name);
        assert!(bundle_destination(&environment, Some(&receipt), name).is_err());
        receipt.path = Path::new("/Applications/Other.app").into();
        assert!(bundle_destination(&environment, Some(&receipt), name).is_err());
    }
}
