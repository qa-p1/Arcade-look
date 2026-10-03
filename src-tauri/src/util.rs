use base64::Engine;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub type Res<T> = Result<T, String>;

/// `ALOOK_DEBUG=1` prints lifecycle tracing to stderr.
pub fn debug_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("ALOOK_DEBUG").is_some_and(|v| v != "0"))
}

#[macro_export]
macro_rules! dbg_log {
    ($($t:tt)*) => {
        if $crate::util::debug_enabled() {
            eprintln!("[arcade-look] {}", format!($($t)*));
        }
    };
}

/// Convert any displayable error into the `String` errors our commands return.
pub trait OrStr<T> {
    fn or_str(self) -> Res<T>;
    fn ctx(self, what: &str) -> Res<T>;
}

impl<T, E: std::fmt::Display> OrStr<T> for Result<T, E> {
    fn or_str(self) -> Res<T> {
        self.map_err(|e| e.to_string())
    }
    fn ctx(self, what: &str) -> Res<T> {
        self.map_err(|e| format!("{what}: {e}"))
    }
}

/// Run blocking work off the async runtime, converting panics (e.g. inside a third-party
/// decoder fed a malicious file) into ordinary errors so the app never goes down.
pub async fn blocking<T, F>(f: F) -> Res<T>
where
    T: Send + 'static,
    F: FnOnce() -> Res<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || catch(f))
        .await
        .map_err(|e| e.to_string())?
}

pub fn catch<T, F: FnOnce() -> Res<T>>(f: F) -> Res<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(p) => {
            let msg = p
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".into());
            Err(format!("internal error while reading file: {msg}"))
        }
    }
}

pub fn millis(t: std::io::Result<SystemTime>) -> Option<i64> {
    let t = t.ok()?;
    Some(match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(e) => -(e.duration().as_millis() as i64),
    })
}

/// Milliseconds since the Unix epoch for a civil (proleptic Gregorian) date-time, UTC.
pub fn civil_to_millis(y: i64, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> i64 {
    // Howard Hinnant's days_from_civil.
    let (m, d) = (m.clamp(1, 12) as i64, d.clamp(1, 31) as i64);
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    ((days * 86400) + hh as i64 * 3600 + mm as i64 * 60 + ss as i64) * 1000
}

/// Natural, case-insensitive ordering: "file2" < "file10".
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let na: String =
                    std::iter::from_fn(|| ai.next_if(|c| c.is_ascii_digit())).collect();
                let nb: String =
                    std::iter::from_fn(|| bi.next_if(|c| c.is_ascii_digit())).collect();
                let ta = na.trim_start_matches('0');
                let tb = nb.trim_start_matches('0');
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                let (lx, ly) = (x.to_lowercase().next(), y.to_lowercase().next());
                if lx != ly {
                    return lx.cmp(&ly);
                }
                ai.next();
                bi.next();
            }
        }
    }
}

pub fn data_uri(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

pub fn image_mime_from_name(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    match ext {
        "png" => "image/png",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "emf" => "image/emf",
        "wmf" => "image/wmf",
        _ => "image/jpeg",
    }
}

/// Root for files we extract (archive entries, plugin output). Cleaned on startup.
pub fn temp_root() -> PathBuf {
    std::env::temp_dir().join("arcade-look")
}

/// Remove temp sub-directories older than a day. Best effort.
pub fn clean_temp() {
    let Ok(rd) = std::fs::read_dir(temp_root()) else {
        return;
    };
    for e in rd.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| m.elapsed().ok())
            .is_some_and(|age| age.as_secs() > 24 * 3600);
        if old {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// Stable short hash for cache keys (FNV-1a; not cryptographic).
pub fn hash_key(parts: &[&str]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for p in parts {
        for b in p.bytes().chain(std::iter::once(0)) {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    format!("{h:016x}")
}

/// Cache key for a file's current content: path + size + mtime.
pub fn file_key(path: &Path) -> String {
    let meta = std::fs::metadata(path).ok();
    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0).to_string();
    let mtime = meta
        .and_then(|m| millis(m.modified()))
        .unwrap_or(0)
        .to_string();
    hash_key(&[&path.to_string_lossy(), &size, &mtime])
}

/// Turn a user-supplied argument (path, `file://` URI, relative path) into an absolute path.
pub fn normalize_arg(arg: &str, cwd: Option<&Path>) -> Option<PathBuf> {
    let arg = arg.trim();
    if arg.is_empty() {
        return None;
    }
    let p = if let Some(rest) = arg.strip_prefix("file://") {
        // file:///home/x or file://localhost/home/x
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        let decoded = percent_encoding::percent_decode_str(rest)
            .decode_utf8_lossy()
            .to_string();
        #[cfg(windows)]
        let decoded = decoded.trim_start_matches('/').replace('/', "\\");
        PathBuf::from(decoded)
    } else {
        PathBuf::from(arg)
    };
    let abs = if p.is_absolute() {
        p
    } else {
        match cwd {
            Some(c) => c.join(p),
            None => std::env::current_dir().ok()?.join(p),
        }
    };
    Some(clean_path(&abs))
}

/// Lexically remove `.` and `..` components (no symlink resolution, no IO).
pub fn clean_path(p: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Decode the path part of an `alook://localhost/<route>/<path>` URL (see `src/lib/urls.ts`).
pub fn path_from_url_segments(encoded: &str) -> PathBuf {
    let decoded: Vec<String> = encoded
        .split('/')
        .map(|seg| {
            percent_encoding::percent_decode_str(seg)
                .decode_utf8_lossy()
                .to_string()
        })
        .collect();
    let joined = decoded.join("/");
    #[cfg(windows)]
    {
        PathBuf::from(joined.replace('/', "\\"))
    }
    #[cfg(not(windows))]
    {
        PathBuf::from(format!("/{joined}"))
    }
}

/// Inverse of `path_from_url_segments`, used by the backend when it emits URLs
/// (e.g. relative images in Markdown).
pub fn url_segments_from_path(path: &Path) -> String {
    const SEG: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~');
    let s = path.to_string_lossy().replace('\\', "/");
    let s = s.strip_prefix('/').unwrap_or(&s);
    s.split('/')
        .map(|seg| percent_encoding::utf8_percent_encode(seg, SEG).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// Base URL of our custom protocol as seen by the webview on this platform.
pub fn protocol_base() -> &'static str {
    if cfg!(windows) {
        "http://alook.localhost/"
    } else {
        "alook://localhost/"
    }
}

pub fn file_url(path: &Path) -> String {
    format!("{}f/{}", protocol_base(), url_segments_from_path(path))
}

pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural() {
        let mut v = vec!["file10", "File2", "file1", "a", "file02b"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["a", "file1", "File2", "file02b", "file10"]);
    }

    #[test]
    fn url_round_trip() {
        #[cfg(not(windows))]
        {
            let p = Path::new("/home/u/My Files/naïve #1?.txt");
            let enc = url_segments_from_path(p);
            assert!(!enc.contains(' ') && !enc.contains('#') && !enc.contains('?'));
            assert_eq!(path_from_url_segments(&enc), p);
        }
    }

    #[test]
    fn normalize() {
        let cwd = Path::new("/tmp/x");
        #[cfg(not(windows))]
        {
            assert_eq!(
                normalize_arg("a/../b.txt", Some(cwd)).unwrap(),
                Path::new("/tmp/x/b.txt")
            );
            assert_eq!(
                normalize_arg("file:///etc/my%20file", None).unwrap(),
                Path::new("/etc/my file")
            );
        }
        assert!(normalize_arg("  ", Some(cwd)).is_none());
    }

    #[test]
    fn panics_become_errors() {
        let r: Res<()> = catch(|| panic!("boom"));
        assert!(r.unwrap_err().contains("boom"));
    }
}
