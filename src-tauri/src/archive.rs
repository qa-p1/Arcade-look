//! Archive listing and single-entry extraction. Every listing is bounded by an entry cap
//! and a time budget so multi-GB archives never hang the UI.

use crate::util::{self, OrStr, Res};
use serde::Serialize;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const MAX_ENTRIES: usize = 50_000;
const BUDGET: Duration = Duration::from_millis(2500);
/// Never extract more than this for a preview.
const MAX_EXTRACT: u64 = 1 << 30;
/// Reaching one entry may mean decompressing everything before it.
const EXTRACT_BUDGET: Duration = Duration::from_secs(30);

/// Fails reads (and seeks) once `deadline` has passed, so no single member
/// (a huge one, or a decompression bomb before the wanted entry) can hold a
/// listing or extraction past its budget.
struct Deadline<R> {
    inner: R,
    deadline: Instant,
}

impl<R> Deadline<R> {
    fn new(inner: R, budget: Duration) -> Self {
        Self {
            inner,
            deadline: Instant::now() + budget,
        }
    }
    fn check(&self) -> std::io::Result<()> {
        if Instant::now() > self.deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "time budget exceeded",
            ));
        }
        Ok(())
    }
}

impl<R: Read> Read for Deadline<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.check()?;
        self.inner.read(buf)
    }
}

impl<R: Seek> Seek for Deadline<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.check()?;
        self.inner.seek(pos)
    }
}

fn timed_out(error: &dyn std::fmt::Display) -> bool {
    error.to_string().contains("time budget exceeded")
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub path: String,
    pub size: Option<u64>,
    pub packed: Option<u64>,
    pub dir: bool,
    pub modified: Option<i64>,
    pub encrypted: bool,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub format: String,
    pub entries: Vec<Entry>,
    pub total_size: u64,
    pub packed_size: u64,
    pub truncated: bool,
    pub note: Option<String>,
}

/// Resolve the effective container format, trusting magic bytes over the extension.
pub fn sniff_format(path: &Path, hint: &str) -> String {
    let mut head = [0u8; 300];
    let n = File::open(path)
        .and_then(|mut f| f.read(&mut head))
        .unwrap_or(0);
    let h = &head[..n];
    let compressed = if h.starts_with(&[0x1f, 0x8b]) {
        Some("gz")
    } else if h.starts_with(b"BZh") {
        Some("bz2")
    } else if h.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0]) {
        Some("xz")
    } else if h.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        Some("zst")
    } else {
        None
    };
    if let Some(c) = compressed {
        // "tar.gz" stays "tar.gz"; "gz" stays single-stream (we peek inside lazily).
        return if hint.starts_with("tar.") {
            format!("tar.{c}")
        } else {
            c.to_string()
        };
    }
    if h.starts_with(b"PK\x03\x04") || h.starts_with(b"PK\x05\x06") {
        return "zip".into();
    }
    if h.starts_with(&[b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C]) {
        return "7z".into();
    }
    if n > 262 && &h[257..262] == b"ustar" {
        return "tar".into();
    }
    if hint == "xz" && !h.is_empty() {
        return "lzma".into(); // legacy .lzma (LZMA-alone)
    }
    hint.to_string()
}

fn decoder<'a>(fmt: &str, r: impl Read + 'a) -> Res<Box<dyn Read + 'a>> {
    Ok(match fmt {
        "gz" => Box::new(flate2::read::MultiGzDecoder::new(r)),
        "bz2" => Box::new(bzip2::read::MultiBzDecoder::new(r)),
        "xz" => Box::new(lzma_rust2::XzReader::new(r, true)),
        "lzma" => Box::new(lzma_rust2::LzmaReader::new_mem_limit(r, 1024 * 1024, None).or_str()?),
        "zst" => Box::new(ruzstd::decoding::StreamingDecoder::new(r).or_str()?),
        "" => Box::new(r),
        other => return Err(format!("unsupported compression: {other}")),
    })
}

pub fn list(path: &Path, hint: &str) -> Res<Listing> {
    let fmt = sniff_format(path, hint);
    let mut listing = match fmt.as_str() {
        "zip" => list_zip(path)?,
        "7z" => list_7z(path)?,
        "tar" => list_tar(path, "")?,
        f if f.starts_with("tar.") => list_tar(path, &f[4..])?,
        "gz" | "bz2" | "xz" | "lzma" | "zst" => list_single(path, &fmt)?,
        other => return Err(format!("Unsupported archive format: {other}")),
    };
    listing.format = fmt;
    Ok(listing)
}

fn empty(fmt: &str) -> Listing {
    Listing {
        format: fmt.into(),
        entries: Vec::new(),
        total_size: 0,
        packed_size: 0,
        truncated: false,
        note: None,
    }
}

fn list_zip(path: &Path) -> Res<Listing> {
    let f = File::open(path).or_str()?;
    let mut z = zip::ZipArchive::new(BufReader::new(f)).ctx("Not a readable ZIP archive")?;
    let mut l = empty("zip");
    let started = Instant::now();
    for i in 0..z.len() {
        if l.entries.len() >= MAX_ENTRIES || started.elapsed() > BUDGET {
            l.truncated = true;
            break;
        }
        let Ok(e) = z.by_index_raw(i) else { continue };
        let modified = e.last_modified().map(|d| {
            util::civil_to_millis(
                d.year() as i64,
                d.month() as u32,
                d.day() as u32,
                d.hour() as u32,
                d.minute() as u32,
                d.second() as u32,
            )
        });
        l.total_size += e.size();
        l.packed_size += e.compressed_size();
        l.entries.push(Entry {
            path: e.name().to_string(),
            size: Some(e.size()),
            packed: Some(e.compressed_size()),
            dir: e.is_dir(),
            modified,
            encrypted: e.encrypted(),
        });
    }
    Ok(l)
}

fn list_7z(path: &Path) -> Res<Listing> {
    // Opening reads (and may decompress) the whole header: same budget.
    let file = Deadline::new(BufReader::new(File::open(path).or_str()?), BUDGET);
    let r = match sevenz_rust2::ArchiveReader::new(file, sevenz_rust2::Password::empty()) {
        Ok(r) => r,
        Err(e) if timed_out(&e) => {
            return Err("Listing stopped after 2.5 s: this 7z archive's index is too large.".into())
        }
        Err(e) => return Err(format!("Not a readable 7z archive: {e}")),
    };
    let mut l = empty("7z");
    l.packed_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    for e in &r.archive().files {
        if l.entries.len() >= MAX_ENTRIES {
            l.truncated = true;
            break;
        }
        if e.is_anti_item() {
            continue;
        }
        let modified = e
            .has_last_modified_date
            .then(|| nt_to_millis(u64::from(e.last_modified_date)));
        l.total_size += e.size();
        l.entries.push(Entry {
            path: e.name().to_string(),
            size: Some(e.size()),
            packed: None,
            dir: e.is_directory(),
            modified,
            encrypted: false,
        });
    }
    Ok(l)
}

fn nt_to_millis(nt: u64) -> i64 {
    (nt / 10_000) as i64 - 11_644_473_600_000
}

fn list_tar(path: &Path, compression: &str) -> Res<Listing> {
    let f = BufReader::with_capacity(256 * 1024, File::open(path).or_str()?);
    // Moving to the next header decompresses the member before it.
    let mut ar = tar::Archive::new(Deadline::new(decoder(compression, f)?, BUDGET));
    let mut l = empty("tar");
    l.packed_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let started = Instant::now();
    let entries = ar.entries().ctx("Not a readable tar archive")?;
    for e in entries {
        if l.entries.len() >= MAX_ENTRIES || started.elapsed() > BUDGET {
            l.truncated = true;
            break;
        }
        let e = match e {
            Ok(e) => e,
            Err(err) if timed_out(&err) => {
                l.truncated = true;
                break;
            }
            Err(err) => {
                l.note = Some(format!(
                    "Archive is damaged after {} entries: {err}",
                    l.entries.len()
                ));
                break;
            }
        };
        let h = e.header();
        let dir = h.entry_type().is_dir();
        let size = h.size().unwrap_or(0);
        let name = e
            .path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "?".into());
        // Skip PAX/GNU metadata records.
        if matches!(
            h.entry_type(),
            tar::EntryType::XGlobalHeader | tar::EntryType::XHeader | tar::EntryType::GNULongName
        ) {
            continue;
        }
        l.total_size += size;
        l.entries.push(Entry {
            path: name,
            size: Some(size),
            packed: None,
            dir,
            modified: h.mtime().ok().map(|s| s as i64 * 1000),
            encrypted: false,
        });
    }
    if l.truncated && started.elapsed() >= BUDGET {
        l.note = Some(
            "Listing stopped after 2.5 s: the archive is large and compressed as a single stream."
                .into(),
        );
    }
    Ok(l)
}

/// gz/bz2/xz/zst holding a single file: show that file as the only entry.
fn list_single(path: &Path, fmt: &str) -> Res<Listing> {
    let mut l = empty(fmt);
    let packed = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    l.packed_size = packed;
    let size = if fmt == "gz" { gzip_isize(path) } else { None };
    l.total_size = size.unwrap_or(0);
    l.entries.push(Entry {
        path: single_inner_name(path, fmt),
        size,
        packed: Some(packed),
        dir: false,
        modified: std::fs::metadata(path)
            .ok()
            .and_then(|m| util::millis(m.modified())),
        encrypted: false,
    });
    Ok(l)
}

fn single_inner_name(path: &Path, fmt: &str) -> String {
    if fmt == "gz" {
        if let Ok(f) = File::open(path) {
            let d = flate2::read::GzDecoder::new(f);
            if let Some(name) = d.header().and_then(|h| h.filename()) {
                let n = String::from_utf8_lossy(name).to_string();
                if let Some(base) = Path::new(&n).file_name() {
                    return base.to_string_lossy().to_string();
                }
            }
        }
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".svgz") {
        return format!("{}.svg", &name[..name.len() - 5]);
    }
    match name.rfind('.') {
        Some(i) if i > 0 => name[..i].to_string(),
        _ => format!("{name}.out"),
    }
}

/// gzip stores the uncompressed size mod 2^32 in the trailer.
fn gzip_isize(path: &Path) -> Option<u64> {
    let mut f = File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    if len < 18 {
        return None;
    }
    f.seek(SeekFrom::End(-4)).ok()?;
    let mut b = [0u8; 4];
    f.read_exact(&mut b).ok()?;
    let isize = u32::from_le_bytes(b) as u64;
    // Only trust it when it is plausible (files > 4 GiB wrap around).
    (len < 1 << 32).then_some(isize)
}

/// Extract one entry into the temp cache and return its path. Cached by archive content.
pub fn extract(path: &Path, hint: &str, entry: &str) -> Res<PathBuf> {
    let fmt = sniff_format(path, hint);
    // One folder per exact entry name: names that sanitize alike
    // (`../a.txt` and `a.txt`, `a:b` and `a?b`, or differ only in case)
    // never share a cached file. The file keeps the entry's own name.
    let dir = util::temp_root()
        .join("x")
        .join(util::file_key(path))
        .join(util::hash_key(&[entry]));
    let name = safe_rel_path(entry);
    let out = dir.join(name.file_name().unwrap_or(name.as_os_str()));
    if out.is_file() {
        return Ok(out);
    }
    std::fs::create_dir_all(&dir).or_str()?;
    // Each extraction writes its own partial file; the finished one is
    // renamed into place, so concurrent previews never share a half-file.
    static PARTIALS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let tmp = dir.join(format!(
        ".{}-{}.partial",
        std::process::id(),
        PARTIALS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let result = (|| -> Res<()> {
        let mut w = std::fs::File::create(&tmp).or_str()?;
        match fmt.as_str() {
            "zip" => {
                let f = File::open(path).or_str()?;
                let mut z = zip::ZipArchive::new(BufReader::new(f)).or_str()?;
                let mut e = z.by_name(entry).ctx("Entry not found")?;
                if e.encrypted() {
                    return Err("This entry is password-protected.".into());
                }
                copy_capped(&mut e, &mut w)?;
            }
            "7z" => {
                let file =
                    Deadline::new(BufReader::new(File::open(path).or_str()?), EXTRACT_BUDGET);
                let mut r = sevenz_rust2::ArchiveReader::new(file, sevenz_rust2::Password::empty())
                    .or_str()?;
                let mut found = false;
                let mut err = None;
                r.for_each_entries(|e, rd| {
                    if e.name() == entry {
                        found = true;
                        if let Err(x) = copy_capped(rd, &mut w) {
                            err = Some(x);
                        }
                        return Ok(false);
                    }
                    std::io::copy(rd, &mut std::io::sink())?;
                    Ok(true)
                })
                .or_str()?;
                if let Some(e) = err {
                    return Err(e);
                }
                if !found {
                    return Err("Entry not found".into());
                }
            }
            f if f == "tar" || f.starts_with("tar.") => {
                let comp = f.strip_prefix("tar.").unwrap_or("");
                let rd = BufReader::with_capacity(256 * 1024, File::open(path).or_str()?);
                let mut ar = tar::Archive::new(Deadline::new(decoder(comp, rd)?, EXTRACT_BUDGET));
                let mut found = false;
                for e in ar.entries().or_str()? {
                    let mut e = e.or_str()?;
                    let name = e
                        .path()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if name == entry {
                        copy_capped(&mut e, &mut w)?;
                        found = true;
                        break;
                    }
                }
                if !found {
                    return Err("Entry not found".into());
                }
            }
            "gz" | "bz2" | "xz" | "lzma" | "zst" => {
                let rd = BufReader::new(File::open(path).or_str()?);
                let mut d = Deadline::new(decoder(&fmt, rd)?, EXTRACT_BUDGET);
                copy_capped(&mut d, &mut w)?;
            }
            other => return Err(format!("Unsupported archive format: {other}")),
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            std::fs::rename(&tmp, &out).or_str()?;
            Ok(out)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(if timed_out(&e) {
                format!(
                    "Gave up after {} s: reaching this entry means decompressing too much of the archive.",
                    EXTRACT_BUDGET.as_secs()
                )
            } else {
                e
            })
        }
    }
}

fn copy_capped(r: &mut dyn Read, w: &mut dyn std::io::Write) -> Res<()> {
    let n = std::io::copy(&mut r.take(MAX_EXTRACT + 1), w).or_str()?;
    if n > MAX_EXTRACT {
        return Err("Entry is larger than 1 GiB; open the archive in a dedicated tool.".into());
    }
    Ok(())
}

/// Strip absolute prefixes and `..` so entries can't escape the extraction dir (zip-slip).
fn safe_rel_path(entry: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for part in entry.split(['/', '\\']) {
        match part {
            "" | "." | ".." => {}
            p => {
                let cleaned: String = p
                    .chars()
                    .map(|c| {
                        if matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                            '_'
                        } else {
                            c
                        }
                    })
                    .collect();
                out.push(cleaned);
            }
        }
    }
    if out.as_os_str().is_empty() {
        out.push("entry");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("alook-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn zip_list_and_extract() {
        let d = tmpdir("zip");
        let p = d.join("a.zip");
        {
            let mut z = zip::ZipWriter::new(File::create(&p).unwrap());
            let o = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            z.add_directory("docs/", o).unwrap();
            z.start_file("docs/readme.md", o).unwrap();
            z.write_all(b"# hi").unwrap();
            z.finish().unwrap();
        }
        let l = list(&p, "zip").unwrap();
        assert_eq!(l.entries.len(), 2);
        assert!(l
            .entries
            .iter()
            .any(|e| e.path == "docs/readme.md" && e.size == Some(4)));
        let out = extract(&p, "zip", "docs/readme.md").unwrap();
        assert_eq!(std::fs::read(out).unwrap(), b"# hi");
    }

    #[test]
    fn tar_gz_and_single_gz() {
        let d = tmpdir("tgz");
        let p = d.join("a.tar.gz");
        {
            let enc = flate2::write::GzEncoder::new(
                File::create(&p).unwrap(),
                flate2::Compression::fast(),
            );
            let mut b = tar::Builder::new(enc);
            let data = b"hello tar";
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, "dir/x.txt", &data[..]).unwrap();
            b.into_inner().unwrap().finish().unwrap();
        }
        let l = list(&p, "tar.gz").unwrap();
        assert_eq!(l.entries[0].path, "dir/x.txt");
        assert_eq!(
            std::fs::read(extract(&p, "tar.gz", "dir/x.txt").unwrap()).unwrap(),
            b"hello tar"
        );

        let g = d.join("log.txt.gz");
        {
            let mut enc = flate2::write::GzEncoder::new(
                File::create(&g).unwrap(),
                flate2::Compression::fast(),
            );
            enc.write_all(b"line1\nline2\n").unwrap();
            enc.finish().unwrap();
        }
        let l = list(&g, "gz").unwrap();
        assert_eq!(l.entries[0].path, "log.txt");
        assert_eq!(l.entries[0].size, Some(12));
        let out = extract(&g, "gz", "log.txt").unwrap();
        assert_eq!(std::fs::read(out).unwrap(), b"line1\nline2\n");
    }

    #[test]
    fn zip_slip_is_neutralised() {
        assert_eq!(
            safe_rel_path("../../etc/passwd"),
            PathBuf::from("etc").join("passwd")
        );
        assert_eq!(safe_rel_path("/abs/x"), PathBuf::from("abs").join("x"));
    }

    #[test]
    fn entries_that_sanitize_alike_keep_their_own_contents() {
        let d = tmpdir("alias");
        let p = d.join("a.zip");
        {
            let mut z = zip::ZipWriter::new(File::create(&p).unwrap());
            let o = zip::write::SimpleFileOptions::default();
            for (name, body) in [
                ("../same.txt", "UP"),
                ("same.txt", "DOWN"),
                ("a:b.txt", "COLON"),
                ("a?b.txt", "QUESTION"),
            ] {
                z.start_file(name, o).unwrap();
                z.write_all(body.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        for (name, body) in [
            ("../same.txt", "UP"),
            ("same.txt", "DOWN"),
            ("a:b.txt", "COLON"),
            ("a?b.txt", "QUESTION"),
        ] {
            let out = extract(&p, "zip", name).unwrap();
            assert_eq!(std::fs::read_to_string(&out).unwrap(), body, "{name}");
            assert!(out.extension().is_some_and(|e| e == "txt"));
        }
    }

    #[test]
    fn concurrent_extractions_do_not_share_a_partial_file() {
        let d = tmpdir("race");
        let p = d.join("a.zip");
        let body = vec![b'x'; 4 << 20];
        {
            let mut z = zip::ZipWriter::new(File::create(&p).unwrap());
            let o = zip::write::SimpleFileOptions::default();
            for name in ["chapter.txt", "chapter.md"] {
                z.start_file(name, o).unwrap();
                z.write_all(&body).unwrap();
            }
            z.finish().unwrap();
        }
        std::thread::scope(|scope| {
            let runs: Vec<_> = (0..8)
                .map(|i| {
                    let p = &p;
                    scope.spawn(move || {
                        extract(
                            p,
                            "zip",
                            if i % 2 == 0 {
                                "chapter.txt"
                            } else {
                                "chapter.md"
                            },
                        )
                    })
                })
                .collect();
            for run in runs {
                assert_eq!(
                    std::fs::metadata(run.join().unwrap().unwrap())
                        .unwrap()
                        .len(),
                    body.len() as u64
                );
            }
        });
    }

    #[test]
    fn a_slow_member_cannot_hold_a_tar_listing_past_its_budget() {
        // Stands in for a member that takes long to decompress.
        struct Slow(std::io::Cursor<Vec<u8>>);
        impl Read for Slow {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                std::thread::sleep(Duration::from_millis(20));
                let n = buf.len().min(512);
                self.0.read(&mut buf[..n])
            }
        }
        let mut tar = tar::Builder::new(Vec::new());
        for name in ["big.bin", "after.txt"] {
            let data = vec![0u8; if name == "big.bin" { 200 * 1024 } else { 1 }];
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_cksum();
            tar.append_data(&mut header, name, data.as_slice()).unwrap();
        }
        let bytes = tar.into_inner().unwrap();
        let started = Instant::now();
        let mut ar = tar::Archive::new(Deadline::new(
            Slow(std::io::Cursor::new(bytes)),
            Duration::from_millis(300),
        ));
        let mut seen = 0;
        let mut stopped = false;
        for e in ar.entries().unwrap() {
            match e {
                Ok(_) => seen += 1,
                Err(err) => {
                    stopped = timed_out(&err);
                    break;
                }
            }
        }
        assert!(stopped && seen == 1, "seen {seen}, stopped {stopped}");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        let d = tmpdir("garbage");
        let p = d.join("bad.zip");
        std::fs::write(&p, b"PK\x03\x04garbagegarbage").unwrap();
        assert!(list(&p, "zip").is_err());
    }
}
