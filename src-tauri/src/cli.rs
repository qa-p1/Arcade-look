//! Command-line interface.

#[derive(Debug, Default, Clone)]
pub struct Args {
    pub paths: Vec<String>,
    pub service: bool,
    pub quit: bool,
    pub install: bool,
    pub uninstall: bool,
    pub help: bool,
    pub version: bool,
}

pub const HELP: &str = "\
Arcade Look: press Space, see anything.

USAGE:
    arcade-look [OPTIONS] [PATH]

ARGS:
    PATH                     File or folder to preview (a file:// URI works too)

OPTIONS:
    --service                Start in the background without a window
                             (used by file manager integrations and autostart)
    --install-integration    Set up file manager integration for this user
    --uninstall-integration  Remove the file manager integration
    --quit                   Quit the running instance
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
    for arg in argv.iter().skip(1).map(|s| s.as_ref()) {
        if only_paths || !arg.starts_with('-') || arg == "-" {
            a.paths.push(arg.to_string());
            continue;
        }
        match arg {
            "--" => only_paths = true,
            "--service" | "--background" | "--gapplication-service" => a.service = true,
            "--quit" => a.quit = true,
            "--install-integration" => a.install = true,
            "--uninstall-integration" => a.uninstall = true,
            "-h" | "--help" => a.help = true,
            "-V" | "--version" => a.version = true,
            _ => {}
        }
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        let a = parse(&["x", "--service", "a.txt", "--weird", "--", "--literal"]);
        assert!(a.service);
        assert_eq!(a.paths, vec!["a.txt", "--literal"]);
        assert!(parse(&["x", "-V"]).version);
    }
}
