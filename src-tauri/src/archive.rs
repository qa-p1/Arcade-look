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
    let r = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())
        .ctx("Not a readable 7z archive")?;
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
    let mut ar = tar::Archive::new(decoder(compression, f)?);
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
    if l.truncated && started.elapsed() > BUDGET {
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
    let dir = util::temp_root().join("x").join(util::file_key(path));
    let rel = safe_rel_path(entry);
    let out = dir.join(&rel);
    if out.is_file() {
        return Ok(out);
    }
    std::fs::create_dir_all(out.parent().unwrap_or(&dir)).or_str()?;
    let tmp = out.with_extension("partial");
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
                let mut r =
                    sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())
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
                let mut ar = tar::Archive::new(decoder(comp, rd)?);
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
                let mut d = decoder(&fmt, rd)?;
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
            Err(e)
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
    fn garbage_is_an_error_not_a_panic() {
        let d = tmpdir("garbage");
        let p = d.join("bad.zip");
        std::fs::write(&p, b"PK\x03\x04garbagegarbage").unwrap();
        assert!(list(&p, "zip").is_err());
    }
}
