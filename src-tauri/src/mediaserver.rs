//! Loopback HTTP server for <video>/<audio> on Linux.
//!
//! WebKitGTK's GStreamer pipeline doesn't stream reliably from custom URI schemes, but it
//! streams perfectly over HTTP. This minimal server binds to 127.0.0.1 on a random port and
//! only answers URLs carrying a random per-session token, so other local processes can't
//! use it to read files. It supports GET/HEAD and single byte ranges, streaming from disk.

use crate::util::path_from_url_segments;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::OnceLock;
use std::time::Duration;

static BASE: OnceLock<String> = OnceLock::new();

/// Base URL ("http://127.0.0.1:PORT/TOKEN/") once started.
pub fn base() -> Option<String> {
    BASE.get().cloned()
}

fn token() -> String {
    let mut b = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut b);
    }
    // Mix in time/pid so a missing /dev/urandom still yields something unguessable enough.
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let salt = crate::util::hash_key(&[&t.to_string(), &std::process::id().to_string()]);
    b.iter().map(|x| format!("{x:02x}")).collect::<String>() + &salt
}

pub fn start() {
    if BASE.get().is_some() {
        return;
    }
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return;
    };
    let Ok(addr) = listener.local_addr() else {
        return;
    };
    let tok = token();
    let _ = BASE.set(format!("http://127.0.0.1:{}/{tok}/", addr.port()));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let tok = tok.clone();
            std::thread::spawn(move || {
                let _ = serve(stream, &tok);
            });
        }
    });
}

fn serve(stream: TcpStream, tok: &str) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut out = stream;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let mut parts = line.split_whitespace();
        let method = parts.next().unwrap_or("").to_string();
        let target = parts.next().unwrap_or("").to_string();
        let mut range: Option<String> = None;
        let mut close = false;
        loop {
            let mut h = String::new();
            if reader.read_line(&mut h)? == 0 {
                return Ok(());
            }
            let h = h.trim_end();
            if h.is_empty() {
                break;
            }
            if let Some((k, v)) = h.split_once(':') {
                let k = k.trim().to_ascii_lowercase();
                if k == "range" {
                    range = Some(v.trim().to_string());
                } else if k == "connection" && v.trim().eq_ignore_ascii_case("close") {
                    close = true;
                }
            }
        }
        respond(&mut out, &method, &target, range.as_deref(), tok)?;
        if close {
            return Ok(());
        }
    }
}

fn status(out: &mut TcpStream, code: &str) -> std::io::Result<()> {
    write!(
        out,
        "HTTP/1.1 {code}\r\nContent-Length: 0\r\nAccess-Control-Allow-Origin: *\r\n\r\n"
    )
}

fn respond(
    out: &mut TcpStream,
    method: &str,
    target: &str,
    range: Option<&str>,
    tok: &str,
) -> std::io::Result<()> {
    if method != "GET" && method != "HEAD" {
        return status(out, "405 Method Not Allowed");
    }
    let path_part = target.split('?').next().unwrap_or("");
    let Some(rest) = path_part
        .strip_prefix('/')
        .and_then(|p| p.strip_prefix(tok))
        .and_then(|p| p.strip_prefix("/f/"))
    else {
        return status(out, "403 Forbidden");
    };
    let path = path_from_url_segments(rest);
    let Ok(mut f) = std::fs::File::open(&path) else {
        return status(out, "404 Not Found");
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mime = crate::protocol::mime_for(&path);
    let (start, end, partial) = match range.map(|r| crate::protocol::parse_range(r, len)) {
        Some(Some((s, e))) => (s, e, true),
        Some(None) => {
            return write!(out, "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{len}\r\nContent-Length: 0\r\n\r\n");
        }
        None => (0, len.saturating_sub(1), false),
    };
    let count = if len == 0 { 0 } else { end - start + 1 };
    let mut head = format!(
        "HTTP/1.1 {}\r\nContent-Type: {mime}\r\nContent-Length: {count}\r\nAccept-Ranges: bytes\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-cache\r\n",
        if partial { "206 Partial Content" } else { "200 OK" }
    );
    if partial {
        head.push_str(&format!("Content-Range: bytes {start}-{end}/{len}\r\n"));
    }
    head.push_str("\r\n");
    out.write_all(head.as_bytes())?;
    if method == "HEAD" || count == 0 {
        return out.flush();
    }
    f.seek(SeekFrom::Start(start))?;
    // Stream; a broken pipe (the player seeked away) just ends this response.
    std::io::copy(&mut (&mut f).take(count), out)?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn serves_ranges_with_token() {
        start();
        let base = base().unwrap();
        let p = std::env::temp_dir().join(format!("alook-ms-{}.bin", std::process::id()));
        std::fs::write(&p, (0u8..100).collect::<Vec<_>>()).unwrap();
        let url = format!("{base}f/{}", crate::util::url_segments_from_path(&p));
        let addr = base
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap()
            .to_string();
        let path = url
            .trim_start_matches("http://")
            .split_once('/')
            .unwrap()
            .1
            .to_string();

        let get = |path: &str, range: Option<&str>| -> String {
            let mut s = TcpStream::connect(&addr).unwrap();
            let r = range.map(|r| format!("Range: {r}\r\n")).unwrap_or_default();
            write!(
                s,
                "GET /{path} HTTP/1.1\r\nHost: x\r\n{r}Connection: close\r\n\r\n"
            )
            .unwrap();
            let mut buf = Vec::new();
            s.read_to_end(&mut buf).unwrap();
            String::from_utf8_lossy(&buf).to_string()
        };
        let r = get(&path, Some("bytes=10-19"));
        assert!(r.starts_with("HTTP/1.1 206"), "{r}");
        assert!(r.contains("Content-Range: bytes 10-19/100"));
        let full = get(&path, None);
        assert!(full.starts_with("HTTP/1.1 200"));
        let bad = get("wrongtoken/f/etc/passwd", None);
        assert!(bad.starts_with("HTTP/1.1 403"), "{bad}");
    }
}
