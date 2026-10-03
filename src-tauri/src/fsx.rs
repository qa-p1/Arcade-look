//! Filesystem helpers: folder listings, budgeted recursive sizes, sibling navigation.

use crate::detect::extension_of;
use crate::util::{millis, natural_cmp, OrStr, Res};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const MAX_LIST: usize = 5000;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub dir: bool,
    pub size: Option<u64>,
    pub modified: Option<i64>,
    pub ext: String,
    pub kind: &'static str,
    pub hidden: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirListing {
    pub entries: Vec<DirEntry>,
    pub total: usize,
    pub truncated: bool,
}

pub fn is_hidden(name: &str, _path: &Path) -> bool {
    if name.starts_with('.') {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if let Ok(m) = std::fs::symlink_metadata(_path) {
            return m.file_attributes() & 0x2 != 0;
        }
    }
    false
}

pub fn list_dir(path: &Path) -> Res<DirListing> {
    let rd = std::fs::read_dir(path).ctx("Cannot read folder")?;
    let mut entries = Vec::new();
    let mut total = 0;
    for e in rd.flatten() {
        total += 1;
        if entries.len() >= MAX_LIST {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        let p = e.path();
        let meta = std::fs::metadata(&p).ok();
        let dir = meta.as_ref().is_some_and(|m| m.is_dir());
        let ext = if dir {
            String::new()
        } else {
            extension_of(&name)
        };
        entries.push(DirEntry {
            kind: if dir { "folder" } else { icon_hint(&ext) },
            ext,
            hidden: is_hidden(&name, &p),
            size: meta.as_ref().filter(|m| !m.is_dir()).map(|m| m.len()),
            modified: meta.and_then(|m| millis(m.modified())),
            path: p.to_string_lossy().to_string(),
            dir,
            name,
        });
    }
    entries.sort_by(|a, b| {
        b.dir
            .cmp(&a.dir)
            .then_with(|| natural_cmp(&a.name, &b.name))
    });
    Ok(DirListing {
        truncated: total > entries.len(),
        entries,
        total,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirSize {
    pub bytes: u64,
    pub files: u64,
    pub dirs: u64,
    pub complete: bool,
}

/// Recursive size with a time and entry budget. Symlinks are not followed.
pub fn dir_size(path: &Path, budget: Duration) -> DirSize {
    let started = Instant::now();
    let mut s = DirSize {
        bytes: 0,
        files: 0,
        dirs: 0,
        complete: true,
    };
    let mut stack: Vec<PathBuf> = vec![path.to_path_buf()];
    while let Some(d) = stack.pop() {
        if started.elapsed() > budget || s.files + s.dirs > 400_000 {
            s.complete = false;
            break;
        }
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                s.dirs += 1;
                stack.push(e.path());
            } else if ft.is_file() {
                s.files += 1;
                s.bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    s
}

/// The file `delta` steps away from `path` among its siblings (natural order, folders
/// first, hidden files only if the current one is hidden).
pub fn neighbor(path: &Path, delta: i64) -> Res<Option<String>> {
    let parent = path.parent().ok_or("no parent folder")?;
    let cur_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let show_hidden = is_hidden(&cur_name, path);
    let mut names: Vec<(bool, String)> = std::fs::read_dir(parent)
        .or_str()?
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            if !show_hidden && is_hidden(&n, &e.path()) {
                return None;
            }
            let dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            Some((dir, n))
        })
        .collect();
    names.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| natural_cmp(&a.1, &b.1)));
    let Some(idx) = names.iter().position(|(_, n)| *n == cur_name) else {
        return Ok(None);
    };
    let next = idx as i64 + delta;
    if next < 0 || next >= names.len() as i64 {
        return Ok(None);
    }
    Ok(Some(
        parent
            .join(&names[next as usize].1)
            .to_string_lossy()
            .to_string(),
    ))
}

/// Cheap kind hint for folder listings (no IO).
pub fn icon_hint(ext: &str) -> &'static str {
    kind_name(crate::detect::detect_from("", ext, b"\0").kind)
}

pub fn kind_name(k: crate::detect::Kind) -> &'static str {
    use crate::detect::Kind::*;
    match k {
        Folder => "folder",
        Image | ImageDecode | ImageRaw | ImagePsd | Svg => "image",
        Video => "video",
        Audio => "audio",
        Pdf => "pdf",
        Markdown | Text | Document | Epub => "doc",
        Code | Json | Notebook | Html => "code",
        Csv | Spreadsheet => "table",
        Presentation => "slides",
        Archive => "archive",
        Font => "font",
        Model => "model",
        Binary => "binary",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbors_and_sizes() {
        let d = std::env::temp_dir().join(format!("alook-fsx-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("sub")).unwrap();
        for n in ["a1.txt", "a10.txt", "a2.txt", ".hidden"] {
            std::fs::write(d.join(n), b"1234").unwrap();
        }
        std::fs::write(d.join("sub/x"), b"12").unwrap();
        let a2 = d.join("a2.txt");
        assert_eq!(
            neighbor(&a2, 1).unwrap().unwrap(),
            d.join("a10.txt").to_string_lossy()
        );
        assert_eq!(
            neighbor(&a2, -1).unwrap().unwrap(),
            d.join("a1.txt").to_string_lossy()
        );
        assert_eq!(neighbor(&d.join("a10.txt"), 1).unwrap(), None);
        // Folders come first.
        assert_eq!(
            neighbor(&d.join("a1.txt"), -1).unwrap().unwrap(),
            d.join("sub").to_string_lossy()
        );
        let s = dir_size(&d, Duration::from_secs(5));
        assert_eq!((s.bytes, s.files, s.dirs, s.complete), (18, 5, 1, true));
        let l = list_dir(&d).unwrap();
        assert_eq!(l.entries[0].name, "sub");
    }
}
