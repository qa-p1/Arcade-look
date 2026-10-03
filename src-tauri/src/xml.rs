//! Tiny, forgiving XML event walker over quick-xml for the OOXML/ODF/EPUB parsers.
//! Element names are reported as qualified names ("w:p"); entity references are resolved
//! into the surrounding text.

use crate::util::{OrStr, Res};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::io::Read;

pub struct Attrs(pub Vec<(String, String)>);

impl Attrs {
    /// Look up by qualified name ("r:id") or, failing that, by local name ("id").
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == name)
            .or_else(|| {
                let local = name.rsplit(':').next().unwrap_or(name);
                self.0.iter().find(|(k, _)| k.rsplit(':').next() == Some(local))
            })
            .map(|(_, v)| v.as_str())
    }
}

pub enum X<'a> {
    Open(&'a str, &'a Attrs),
    Close(&'a str),
    Text(&'a str),
}

fn attrs_of(e: &BytesStart) -> Attrs {
    Attrs(
        e.attributes()
            .flatten()
            .map(|a| {
                let k = String::from_utf8_lossy(a.key.as_ref()).to_string();
                let v = a
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .map(|c| c.to_string())
                    .unwrap_or_else(|_| String::from_utf8_lossy(&a.value).to_string());
                (k, v)
            })
            .collect(),
    )
}

fn resolve_entity(name: &str) -> Option<String> {
    Some(match name {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" => "'".into(),
        "nbsp" => "\u{a0}".into(),
        _ => {
            let n = name.strip_prefix('#')?;
            let code = if let Some(h) = n.strip_prefix('x').or_else(|| n.strip_prefix('X')) {
                u32::from_str_radix(h, 16).ok()?
            } else {
                n.parse().ok()?
            };
            char::from_u32(code)?.to_string()
        }
    })
}

/// Walk `xml`, calling `f` for every open/close/text event. Malformed trailing content
/// stops the walk without failing: partial documents still preview.
pub fn walk(xml: &str, mut f: impl FnMut(X)) -> Res<()> {
    let mut r = Reader::from_str(xml);
    r.config_mut().check_end_names = false;
    let mut text = String::new();
    let flush = |text: &mut String, f: &mut dyn FnMut(X)| {
        if !text.is_empty() {
            f(X::Text(text));
            text.clear();
        }
    };
    let mut seen_any = false;
    loop {
        let ev = match r.read_event() {
            Ok(ev) => ev,
            Err(e) => {
                if seen_any {
                    break;
                }
                return Err(format!("Malformed XML: {e}"));
            }
        };
        seen_any = true;
        match ev {
            Event::Start(e) => {
                flush(&mut text, &mut f);
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let a = attrs_of(&e);
                f(X::Open(&name, &a));
            }
            Event::Empty(e) => {
                flush(&mut text, &mut f);
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let a = attrs_of(&e);
                f(X::Open(&name, &a));
                f(X::Close(&name));
            }
            Event::End(e) => {
                flush(&mut text, &mut f);
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                f(X::Close(&name));
            }
            Event::Text(t) => {
                if let Ok(s) = t.decode() {
                    text.push_str(&s);
                }
            }
            Event::CData(t) => {
                if let Ok(s) = t.decode() {
                    text.push_str(&s);
                }
            }
            Event::GeneralRef(r) => {
                if let Ok(name) = r.decode() {
                    if let Some(s) = resolve_entity(&name) {
                        text.push_str(&s);
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    flush(&mut text, &mut f);
    Ok(())
}

/// Read a zip member as UTF-8 text (capped).
pub fn zip_text<R: Read + std::io::Seek>(z: &mut zip::ZipArchive<R>, name: &str) -> Option<String> {
    let mut f = z.by_name(name).ok()?;
    let mut s = String::new();
    (&mut f).take(96 << 20).read_to_string(&mut s).ok()?;
    Some(s)
}

pub fn zip_bytes<R: Read + std::io::Seek>(
    z: &mut zip::ZipArchive<R>,
    name: &str,
    max: u64,
) -> Option<Vec<u8>> {
    let mut f = z.by_name(name).ok()?;
    if f.size() > max {
        return None;
    }
    let mut v = Vec::with_capacity(f.size() as usize);
    (&mut f).take(max).read_to_end(&mut v).ok()?;
    Some(v)
}

pub fn open_zip(path: &std::path::Path) -> Res<zip::ZipArchive<std::io::BufReader<std::fs::File>>> {
    let f = std::fs::File::open(path).or_str()?;
    zip::ZipArchive::new(std::io::BufReader::new(f)).ctx("Not a valid Office/ZIP container")
}

/// Parse an OPC relationships part into (Id → (Target, external)).
pub fn rels(xml: &str) -> std::collections::HashMap<String, (String, bool)> {
    let mut m = std::collections::HashMap::new();
    let _ = walk(xml, |ev| {
        if let X::Open("Relationship", a) = ev {
            if let (Some(id), Some(t)) = (a.get("Id"), a.get("Target")) {
                let ext = a.get("TargetMode") == Some("External");
                m.insert(id.to_string(), (t.to_string(), ext));
            }
        }
    });
    m
}

/// Resolve an OPC relative target against the directory of the part that references it.
pub fn resolve_part(base_dir: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') {
        return abs.to_string();
    }
    let mut parts: Vec<&str> = base_dir.split('/').filter(|s| !s.is_empty()).collect();
    for seg in target.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            s => parts.push(s),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_and_resolves_entities() {
        let mut out = Vec::new();
        walk(r#"<a x="1 &amp; 2"><b/>t &lt;3 &#x263A;</a>"#, |e| {
            out.push(match e {
                X::Open(n, a) => format!("+{n}{}", a.get("x").unwrap_or("")),
                X::Close(n) => format!("-{n}"),
                X::Text(t) => format!("'{t}'"),
            })
        })
        .unwrap();
        assert_eq!(out, vec!["+a1 & 2", "+b", "-b", "'t <3 ☺'", "-a"]);
    }

    #[test]
    fn resolves_parts() {
        assert_eq!(resolve_part("ppt/slides", "../media/image1.png"), "ppt/media/image1.png");
        assert_eq!(resolve_part("word", "media/a.png"), "word/media/a.png");
        assert_eq!(resolve_part("word", "/word/media/a.png"), "word/media/a.png");
    }
}
