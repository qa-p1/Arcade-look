//! Command-line interface.

#[derive(Debug, Default, Clone)]
pub struct Args {
    pub paths: Vec<String>,
    pub service: bool,
    pub settings: bool,
    pub status: bool,
    pub show: bool,
    pub hide: bool,
    pub toggle: bool,
    pub quit: bool,
    pub install: bool,
    pub uninstall: bool,
    pub repair: bool,
    pub integration_status: bool,
    pub silent: bool,
    pub no_launch: bool,
    pub remove_data: bool,
    pub channel: Option<String>,
    /// Compatibility aliases for the original file-manager integration flags.
    pub install_integration: bool,
    pub uninstall_integration: bool,
    pub restart: bool,
    pub help: bool,
    pub version: bool,
    /// Print the Arcade Link manifest and exit.
    pub manifest: bool,
    /// Arcade Link one-shot mode: one request on stdin, no UI.
    pub invoke: bool,
}

pub const HELP: &str = "\
Arcade Look: press Space, see anything.

USAGE:
    arcade-look [OPTIONS] [PATH]

ARGS:
    PATH                     File or folder to preview (a file:// URI works too)

OPTIONS:
    --background             Start in the background without a window
    --service                Compatibility alias for --background
    --status                 Print running, version and Link state as one-line JSON
    --show                   Show the preview
    --hide                   Hide the preview
    --toggle                 Preview the selection or close the preview
    --settings               Open the settings
    --restart                Restart the running instance
    --install [--silent] [--no-launch] [--keep-download] [--channel C]
                             Install this AppImage or move this macOS app bundle
    --install-integration    Alias for --install
    --uninstall [--silent] [--remove-data]
                             Remove this installation using its receipt
    --uninstall-integration  Alias for --uninstall
    --repair                 Repair integration recorded in the install receipt
    --integration-status     Print receipt and integration status as JSON
    --quit                   Quit the running instance
    --arcade-manifest        Print the Arcade Link manifest (no side effects)
    --arcade-invoke          Run one Arcade Link request from stdin, without any UI
    -h, --help               Show this help
    -V, --version            Show the version

KEYS (in the preview window):
    Space / Esc  close         ←/→  previous / next file     Enter  open with default app
    I  info panel   F  fullscreen   +/-/0  zoom   ?  all shortcuts   Ctrl+Q  quit
";

/// Parse argv (including argv[0]). Unknown flags are ignored so that file managers which
/// append their own arguments never break us.
pub fn parse<S: AsRef<str>>(argv: &[S]) -> Args {
    let mut a = Args::default();
    let mut only_paths = false;
    let mut it = argv.iter().skip(1).map(|s| s.as_ref()).peekable();
    while let Some(arg) = it.next() {
        if only_paths || !arg.starts_with('-') || arg == "-" {
            a.paths.push(arg.to_string());
            continue;
        }
        match arg {
            "--" => only_paths = true,
            "--service" | "--background" | "--gapplication-service" => a.service = true,
            "--settings" => a.settings = true,
            "--status" => a.status = true,
            "--show" => a.show = true,
            "--hide" => a.hide = true,
            "--toggle" => a.toggle = true,
            "--quit" => a.quit = true,
            "--install" => a.install = true,
            "--install-integration" => a.install_integration = true,
            "--uninstall" => a.uninstall = true,
            "--uninstall-integration" => a.uninstall_integration = true,
            "--repair" => a.repair = true,
            "--integration-status" => a.integration_status = true,
            "--silent" => a.silent = true,
            "--no-launch" => a.no_launch = true,
            "--remove-data" => a.remove_data = true,
            "--restart" => a.restart = true,
            "--channel" => a.channel = it.next().map(str::to_string),
            "-h" | "--help" => a.help = true,
            "-V" | "--version" => a.version = true,
            "--arcade-manifest" => a.manifest = true,
            "--arcade-invoke" => a.invoke = true,
            _ => {}
        }
    }
    a
}

pub fn run_maintenance(args: &Args) {
    let result = if args.install || args.install_integration {
        crate::installer::install_cli(
            args.silent,
            args.no_launch,
            args.channel.as_deref().unwrap_or("stable"),
        )
    } else if args.uninstall || args.uninstall_integration {
        crate::installer::uninstall_cli(args.silent, args.remove_data)
    } else if args.repair {
        crate::installer::repair_cli()
    } else if args.integration_status {
        crate::installer::integration_status_cli()
    } else {
        Err("unsupported maintenance command".to_string())
    };
    match result {
        Ok(output) if !output.is_empty() => println!("{output}"),
        Ok(_) => {}
        Err(error) => {
            eprintln!("arcade-look: {error}");
            std::process::exit(1);
        }
    }
}

pub fn run_integration(install: bool) {
    let result = if install {
        crate::integration::install()
    } else {
        crate::integration::uninstall()
    };
    match result {
        Ok(message) => println!("{message}"),
        Err(error) => {
            eprintln!("arcade-look: {error}");
            std::process::exit(1);
        }
    }
}

pub fn restart_running() {
    let locations = arcade_link::Locations::discover();
    let peer = crate::link_consumer::me();
    if let Ok(mut client) =
        arcade_link::client::Client::connect(&locations, arcade_link::ids::LOOK, &peer)
    {
        if let Err(error) = client.restart(None) {
            eprintln!("arcade-look: restart failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    // No resident endpoint means there is nothing to replace; start the normal service.
    let exe = crate::integration::launcher_path();
    let mut command = std::process::Command::new(exe);
    // The service runs from its own mount; it mustn't keep this one alive.
    if let Err(error) = arcade_link::childenv::detach_from_image(&mut command)
        .arg("--service")
        .spawn()
    {
        eprintln!("arcade-look: could not start: {error}");
        std::process::exit(1);
    }
}

/// Read-only status: never starts GTK, Link, or a resident app.
pub fn print_status() {
    let locations = arcade_link::Locations::discover();
    let registry = arcade_link::Registry::load(&locations);
    let peer = arcade_link::PeerInfo {
        id: "arcade.look.status".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    };
    let running = match arcade_link::client::app_state(
        &locations,
        &registry,
        arcade_link::ids::LOOK,
        &peer,
    ) {
        arcade_link::AppState::Running { version } => Some(version),
        _ => None,
    };
    let link = registry
        .get(arcade_link::ids::LOOK)
        .map_or("unregistered", |manifest| {
            if manifest.settings.link_enabled {
                "on"
            } else {
                "off"
            }
        });
    println!(
        "{}",
        serde_json::json!({"app": arcade_link::ids::LOOK, "version": env!("CARGO_PKG_VERSION"), "running": running.is_some(), "runningVersion": running, "link": link})
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_runtime_and_install_options() {
        let a = parse(&[
            "x",
            "--service",
            "--install-integration",
            "--silent",
            "--channel",
            "nightly",
            "a.txt",
            "--weird",
            "--",
            "--literal",
        ]);
        assert!(a.service);
        assert!(a.install_integration);
        assert_eq!(a.channel.as_deref(), Some("nightly"));
        assert_eq!(a.paths, ["a.txt", "--literal"]);
        assert!(parse(&["x", "--repair"]).repair);
        assert!(parse(&["x", "-V"]).version);
    }
    #[test]
    fn standard_surface_flags_and_background_aliases_are_kept() {
        assert!(parse(&["look", "--status"]).status);
        assert!(parse(&["look", "--show"]).show);
        assert!(parse(&["look", "--hide"]).hide);
        assert!(parse(&["look", "--toggle"]).toggle);
        for flag in ["--service", "--background", "--gapplication-service"] {
            assert!(parse(&["look", flag]).service);
        }
        assert_eq!(parse(&["look", "--", "--show"]).paths, ["--show"]);
    }
}
