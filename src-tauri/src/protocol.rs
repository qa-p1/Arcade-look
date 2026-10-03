//! `alook://` protocol: streams local files to the webview with HTTP Range support
//! (so a 20 GB video seeks instantly and is never read whole), plus derived resources.
//!
//! Routes (path segments percent-encoded, see `util::url_segments_from_path`):
//!   /f/<path>                 raw file (Range aware)
//!   /img/<kind>/<max>/<path>  decoded image (kind: decode | raw | psd) as PNG/JPEG
//!   /cover/<path>             embedded audio cover art
//!   /plugin/<id>/<file>       script plugin assets

use crate::util::path_from_url_segments;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use tauri::http::{header, Request, Response, StatusCode};

/// Largest slice served for one ranged request.
const CHUNK: u64 = 4 << 20;
/// Largest file served whole to a request without a Range header.
const FULL_MAX: u64 = 1 << 30;

type Resp = Response<Vec<u8>>;

pub fn handle(req: &Request<Vec<u8>>) -> Resp {
    let resp = route(req);
    crate::dbg_log!(
        "{} {} range={:?} -> {} {:?} len={}",
        req.method(),
        req.uri().path(),
        req.headers().get(header::RANGE),
        resp.status(),
        resp.headers().get(header::CONTENT_RANGE),
        resp.body().len()
    );
    resp
}

fn route(req: &Request<Vec<u8>>) -> Resp {
    let path = req.uri().path();
    let path = path.strip_prefix('/').unwrap_or(path);
    let (route, rest) = path.split_once('/').unwrap_or((path, ""));
    match route {
        "f" => serve_file(req, &path_from_url_segments(rest)),
        "img" => {
            let mut it = rest.splitn(3, '/');
            let kind = it.next().unwrap_or("decode").to_string();
            let max: u32 = it.next().and_then(|m| m.parse().ok()).unwrap_or(4096);
            let p = path_from_url_segments(it.next().unwrap_or(""));
            match crate::util::catch(|| crate::imaging::render(&p, &kind, max)) {
                Ok((mime, bytes)) => ok(&mime, bytes),
                Err(e) => error(StatusCode::UNPROCESSABLE_ENTITY, &e),
            }
        }
        "cover" => {
            match crate::util::catch(|| crate::media::cover(&path_from_url_segments(rest))) {
                Ok((mime, bytes)) => ok(&mime, bytes),
                Err(e) => error(StatusCode::NOT_FOUND, &e),
            }
        }
        "plugin" => {
            let (id, file) = rest.split_once('/').unwrap_or((rest, ""));
            let id = percent_encoding::percent_decode_str(id)
                .decode_utf8_lossy()
                .to_string();
            let file = percent_encoding::percent_decode_str(file)
                .decode_utf8_lossy()
                .to_string();
            if id.contains("..") || file.split(['/', '\\']).any(|s| s == "..") {
                return error(StatusCode::FORBIDDEN, "forbidden");
            }
            let p = crate::config::plugins_dir().join(id).join(file);
            serve_file(req, &p)
        }
        _ => error(StatusCode::NOT_FOUND, "unknown route"),
    }
}

fn base(status: StatusCode, mime: &str) -> tauri::http::response::Builder {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            "Accept-Ranges, Content-Range, Content-Length",
        )
        .header(header::CACHE_CONTROL, "no-cache")
}

fn ok(mime: &str, body: Vec<u8>) -> Resp {
    base(StatusCode::OK, mime).body(body).unwrap_or_default()
}

fn error(status: StatusCode, msg: &str) -> Resp {
    base(status, "text/plain; charset=utf-8")
        .body(msg.as_bytes().to_vec())
        .unwrap_or_default()
}

pub fn mime_for(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = crate::detect::extension_of(&name);
    match ext.as_str() {
        "js" | "mjs" => return "text/javascript".into(),
        "css" => return "text/css".into(),
        "wasm" => return "application/wasm".into(),
        "gltf" => return "model/gltf+json".into(),
        "bin" => return "application/octet-stream".into(),
        _ => {}
    }
    let d = crate::detect::detect_from(&name, &ext, &[]);
    if d.mime == "text/plain" {
        "text/plain; charset=utf-8".into()
    } else {
        d.mime
    }
}

/// Parse a single `bytes=a-b` / `bytes=a-` / `bytes=-n` range.
pub fn parse_range(h: &str, len: u64) -> Option<(u64, u64)> {
    let spec = h.trim().strip_prefix("bytes=")?;
    let first = spec.split(',').next()?.trim();
    let (a, b) = first.split_once('-')?;
    let (start, end) = if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        (len.saturating_sub(n), len.checked_sub(1)?)
    } else {
        let s: u64 = a.parse().ok()?;
        let e = if b.is_empty() {
            len.checked_sub(1)?
        } else {
            b.parse::<u64>().ok()?.min(len.checked_sub(1)?)
        };
        (s, e)
    };
    (start <= end && start < len).then_some((start, end))
}

fn serve_file(req: &Request<Vec<u8>>, path: &Path) -> Resp {
    let mut f = match File::open(path) {
        Ok(f) => f,
        Err(e) => return error(StatusCode::NOT_FOUND, &e.to_string()),
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mime = mime_for(path);
    let head_only = req.method() == tauri::http::Method::HEAD;

    let range = req
        .headers()
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(|h| parse_range(h, len));

    let (start, end, partial) = match range {
        Some(Some((s, e))) => (s, e.min(s + CHUNK - 1), true),
        Some(None) => {
            return base(StatusCode::RANGE_NOT_SATISFIABLE, &mime)
                .header(header::CONTENT_RANGE, format!("bytes */{len}"))
                .body(Vec::new())
                .unwrap_or_default();
        }
        None if len <= FULL_MAX => (0, len.saturating_sub(1), false),
        None => (0, CHUNK - 1, true),
    };
    let count = if len == 0 { 0 } else { end - start + 1 };
    let mut body = Vec::new();
    if !head_only && count > 0 {
        body.reserve(count as usize);
        if f.seek(SeekFrom::Start(start)).is_err()
            || (&mut f).take(count).read_to_end(&mut body).is_err()
        {
            return error(StatusCode::INTERNAL_SERVER_ERROR, "read failed");
        }
    }
    let mut b = base(
        if partial {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        },
        &mime,
    )
    .header(header::ACCEPT_RANGES, "bytes")
    .header(header::CONTENT_LENGTH, count.to_string());
    if partial {
        b = b.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"));
    }
    b.body(body).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-", 100), Some((0, 99)));
        assert_eq!(parse_range("bytes=10-19", 100), Some((10, 19)));
        assert_eq!(parse_range("bytes=-10", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=50-500", 100), Some((50, 99)));
        assert_eq!(parse_range("bytes=100-", 100), None);
        assert_eq!(parse_range("bytes=0-", 0), None);
        assert_eq!(parse_range("items=0-1", 100), None);
    }

    #[cfg(unix)]
    #[test]
    fn serves_ranges() {
        let p = std::env::temp_dir().join(format!("alook-proto-{}.bin", std::process::id()));
        std::fs::write(&p, (0u8..=255).collect::<Vec<_>>()).unwrap();
        let url = format!(
            "alook://localhost/f/{}",
            crate::util::url_segments_from_path(&p)
        );
        let req = Request::builder()
            .uri(&url)
            .header("Range", "bytes=16-31")
            .body(Vec::new())
            .unwrap();
        let r = handle(&req);
        assert_eq!(r.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(r.body(), &(16u8..=31).collect::<Vec<_>>());
        assert_eq!(r.headers()[header::CONTENT_RANGE], "bytes 16-31/256");
        let full = handle(&Request::builder().uri(&url).body(Vec::new()).unwrap());
        assert_eq!(full.status(), StatusCode::OK);
        assert_eq!(full.body().len(), 256);
    }
}
