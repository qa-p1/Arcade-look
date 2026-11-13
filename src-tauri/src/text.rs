//! Reading text files: capped reads, BOM handling, legacy-encoding detection.

use crate::util::{OrStr, Res};
use serde::Serialize;
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextData {
    pub text: String,
    pub encoding: String,
    pub truncated: bool,
    pub size: u64,
    pub lines: usize,
}

pub fn read_capped(path: &Path, max: u64) -> Res<(Vec<u8>, u64)> {
    let f = File::open(path).ctx("Cannot open file")?;
    let size = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mut buf = Vec::with_capacity(size.min(max) as usize);
    f.take(max).read_to_end(&mut buf).or_str()?;
    Ok((buf, size))
}

pub fn read_text(path: &Path, max: u64) -> Res<TextData> {
    let (bytes, size) = read_capped(path, max)?;
    let truncated = (bytes.len() as u64) < size;
    let (text, encoding) = decode(&bytes);
    // An empty file has no lines (LK-4: it said "1 line").
    let lines = text.lines().count();
    Ok(TextData {
        text,
        encoding,
        truncated,
        size,
        lines,
    })
}

/// Decode bytes to a String, detecting UTF-8/UTF-16 BOMs and guessing legacy encodings.
pub fn decode(bytes: &[u8]) -> (String, String) {
    if let Some((enc, bom_len)) = encoding_rs::Encoding::for_bom(bytes) {
        let (cow, _) = enc.decode_without_bom_handling(&bytes[bom_len..]);
        return (cow.into_owned(), enc.name().to_string());
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => return (s.to_string(), "UTF-8".into()),
        Err(e) if e.error_len().is_none() => {
            // Truncated in the middle of a multi-byte sequence: drop the partial char.
            let s = std::str::from_utf8(&bytes[..e.valid_up_to()]).unwrap_or_default();
            return (s.to_string(), "UTF-8".into());
        }
        Err(_) => {}
    }
    let mut det = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    det.feed(
        &bytes[..bytes.len().min(64 * 1024)],
        bytes.len() <= 64 * 1024,
    );
    let enc = det.guess(None, chardetng::Utf8Detection::Allow);
    let (cow, _, _) = enc.decode(bytes);
    (cow.into_owned(), enc.name().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LK-4: an empty file has no lines; one without a final newline has one.
    #[test]
    fn line_counts() {
        let dir = std::env::temp_dir().join(format!("look-lines-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let count = |text: &str| {
            let p = dir.join("t.txt");
            std::fs::write(&p, text).unwrap();
            read_text(&p, 1 << 20).unwrap().lines
        };
        assert_eq!(count(""), 0);
        assert_eq!(count("one"), 1);
        assert_eq!(count("one\ntwo\n"), 2);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn decodes_variants() {
        assert_eq!(decode(b"hello").0, "hello");
        assert_eq!(decode(b"\xEF\xBB\xBFhi").0, "hi");
        let utf16le = [0xFF, 0xFE, b'h', 0, b'i', 0];
        let (s, enc) = decode(&utf16le);
        assert_eq!(s, "hi");
        assert_eq!(enc, "UTF-16LE");
        // Latin-1/Windows-1252 "café"
        let (s, _) = decode(b"caf\xe9 au lait, cr\xe8me br\xfbl\xe9e");
        assert!(s.starts_with("café"));
        // Truncated UTF-8
        let b = "añb".as_bytes();
        assert_eq!(decode(&b[..2]).0, "a");
    }
}
