//! Memory items from `AUDIT/memory.md` (D12): the resident gives back what a closed window leaves behind, runs
//! from its unpacked copy, and trims itself while a window is open. Code refaults from the page cache in
//! microseconds, so nothing here changes how Look feels. No timer runs while Look has no window.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Manager};

/// OPEN1: the first trim after a page has loaded, then one every 20 s while any window is open.
const OPEN_TRIM_FIRST: Duration = Duration::from_secs(2);
const OPEN_TRIM_EVERY: Duration = Duration::from_secs(20);
/// W1: GTK and WebKit finish destroying a window first; the network process flushes its storage (the volume
/// the media player remembers is in it).
pub(crate) const RELEASE_AFTER_CLOSE: Duration = Duration::from_secs(3);
/// W0: startup has settled (Link, the tray and the shortcut are up).
pub(crate) const RELEASE_AFTER_STARTUP: Duration = Duration::from_secs(15);

static TRIMMING: AtomicBool = AtomicBool::new(false);

/// Whether these arguments leave a resident Look running. `--quit` and `--hide` end or hide what is already
/// running and never keep a process of their own, so they need no unpacked copy (U1).
pub(crate) fn starts_resident(args: &crate::cli::Args) -> bool {
    !(args.quit || args.hide)
}

/// OPEN1: started when a page finishes loading; one thread for all windows, ending with the last one.
pub(crate) fn trim_while_open(app: &AppHandle) {
    if TRIMMING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(OPEN_TRIM_FIRST);
        loop {
            if app.webview_windows().is_empty() {
                TRIMMING.store(false, Ordering::SeqCst);
                // A page that loaded just now saw the flag still set and left the trimming to this thread.
                if app.webview_windows().is_empty() || TRIMMING.swap(true, Ordering::SeqCst) {
                    return;
                }
            } else {
                arcade_link::runtime::trim();
            }
            std::thread::sleep(OPEN_TRIM_EVERY);
        }
    });
}

/// W1 + W3b, and W0 at startup: once no window is left, end WebKit's network process (WebKit starts a new one
/// with the next window), return freed heap and code pages, and let the kernel page out the idle heap (free
/// unless memory is short). A window opened in the meantime cancels it: with one open it only trims.
pub(crate) fn reclaim_after(app: &AppHandle, delay: Duration) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        if app.webview_windows().is_empty() {
            arcade_link::runtime::release_webkit();
            arcade_link::runtime::page_out_hidden();
        } else {
            arcade_link::runtime::trim();
        }
    });
}

/// W0 at startup: the release above, and (U1) the end of copies left by earlier versions. Each update unpacks a
/// new `look-<version>-<key>` folder of several hundred MB; only the one this process runs from is needed.
pub(crate) fn after_startup(app: &AppHandle) {
    reclaim_after(app, RELEASE_AFTER_STARTUP);
    #[cfg(target_os = "linux")]
    if let Some(appdir) = std::env::var_os("APPDIR")
        .filter(|_| std::env::var_os(arcade_link::runtime::UNPACKED_ENV).is_some())
    {
        std::thread::spawn(move || {
            std::thread::sleep(RELEASE_AFTER_STARTUP);
            let root = arcade_link::runtime::unpacked_root();
            for stale in stale_copies(&root, std::path::Path::new(&appdir)) {
                if !in_use(&stale) {
                    let _ = std::fs::remove_dir_all(stale);
                }
            }
        });
    }
}

/// Look's finished copies in `root` other than `keep`. A copy still being written (`.look-*.tmp-*`) is another
/// process's and is left alone.
#[cfg(target_os = "linux")]
fn stale_copies(root: &std::path::Path, keep: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| {
            entry.file_name().to_string_lossy().starts_with("look-")
                && entry.file_type().is_ok_and(|kind| kind.is_dir())
                && entry.path() != keep
        })
        .map(|entry| entry.path())
        .collect()
}

/// Whether any process runs a program from inside `dir` (a resident of an older version that is still going).
#[cfg(target_os = "linux")]
fn in_use(dir: &std::path::Path) -> bool {
    std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| {
            std::fs::read_link(entry.path().join("exe")).is_ok_and(|exe| exe.starts_with(dir))
        })
}

/// OPEN1: WebKit's helper processes inherit `libarcade-trim.so` (vendored from Link, built by
/// `scripts/prepare-trimmer.py`), which trims them 2 s after start and every 20 s; it does nothing in any other
/// process. Programs Look starts never get it: Link's child environment drops entries inside `$APPDIR`. Only
/// the unpacked copy sets it (or a process outside an AppImage): the launcher's mount is gone once the copy runs.
/// Call it while Look still has one thread.
#[cfg(target_os = "linux")]
pub fn preload_webkit_trimmer() {
    let unpacked = std::env::var_os(arcade_link::runtime::UNPACKED_ENV).is_some();
    if !unpacked && std::env::var_os("APPIMAGE").is_some() {
        return;
    }
    let Some(dir) = std::env::var_os("APPDIR").filter(|d| !d.is_empty()) else {
        return;
    };
    let old = std::env::var_os("LD_PRELOAD");
    if let Some(value) = trimmer_preload(std::path::Path::new(&dir), old.as_deref()) {
        // Look has one thread here (before the first runtime, GTK or WebKit); nothing reads the environment
        // concurrently.
        std::env::set_var("LD_PRELOAD", value);
    }
}

/// The `LD_PRELOAD` value that adds `<appdir>/usr/lib/libarcade-trim.so` to `old`, if the image carries it.
#[cfg(target_os = "linux")]
fn trimmer_preload(
    appdir: &std::path::Path,
    old: Option<&std::ffi::OsStr>,
) -> Option<std::ffi::OsString> {
    let library = appdir.join("usr/lib/libarcade-trim.so");
    if !library.is_file() {
        return None;
    }
    let mut value = library.into_os_string();
    if let Some(old) = old.filter(|v| !v.is_empty()) {
        value.push(":");
        value.push(old);
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> crate::cli::Args {
        let argv: Vec<String> = std::iter::once("arcade-look")
            .chain(list.iter().copied())
            .map(String::from)
            .collect();
        crate::cli::parse(&argv)
    }

    #[test]
    fn only_quit_and_hide_skip_the_unpacked_copy() {
        for list in [
            &[][..],
            &["--background"],
            &["--service"],
            &["--show"],
            &["--toggle"],
            &["--settings"],
            &["note.md"],
        ] {
            assert!(starts_resident(&args(list)), "{list:?} keeps a resident");
        }
        assert!(!starts_resident(&args(&["--quit"])));
        assert!(!starts_resident(&args(&["--hide"])));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn only_older_finished_copies_of_look_are_stale() {
        let root = std::env::temp_dir().join(format!("look-stale-{}", std::process::id()));
        let keep = root.join("look-1.0.0-0123456789abcdef");
        for name in [
            "look-1.0.0-0123456789abcdef",
            "look-0.4.3-fedcba9876543210",
            "tools-1.0.0-0123456789abcdef",
            ".look-1.0.0-0123456789abcdef.tmp-7",
        ] {
            std::fs::create_dir_all(root.join(name)).unwrap();
        }
        std::fs::write(root.join("look-notes.txt"), b"x").unwrap();
        assert_eq!(
            stale_copies(&root, &keep),
            vec![root.join("look-0.4.3-fedcba9876543210")]
        );
        // A program running from inside a folder marks it in use; this test binary's own folder is.
        let own = std::env::current_exe().unwrap();
        assert!(in_use(own.parent().unwrap()));
        assert!(!in_use(&root.join("look-0.4.3-fedcba9876543210")));
        assert!(stale_copies(&root.join("missing"), &keep).is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_trimmer_is_preloaded_only_when_the_image_carries_it() {
        let root = std::env::temp_dir().join(format!("look-trim-{}", std::process::id()));
        let lib = root.join("usr/lib");
        std::fs::create_dir_all(&lib).unwrap();
        assert_eq!(trimmer_preload(&root, None), None);
        std::fs::write(lib.join("libarcade-trim.so"), b"x").unwrap();
        let own = lib.join("libarcade-trim.so").into_os_string();
        assert_eq!(trimmer_preload(&root, None), Some(own.clone()));
        assert_eq!(
            trimmer_preload(&root, Some(std::ffi::OsStr::new(""))),
            Some(own.clone())
        );
        let mut both = own;
        both.push(":/usr/lib/libother.so");
        assert_eq!(
            trimmer_preload(&root, Some(std::ffi::OsStr::new("/usr/lib/libother.so"))),
            Some(both)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
