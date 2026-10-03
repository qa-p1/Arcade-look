//! Presentations (PPTX, ODP) → structured slides: title, text blocks, images, tables, notes.

use crate::util::{data_uri, image_mime_from_name, Res};
use crate::xml::{open_zip, rels, resolve_part, walk, zip_bytes, zip_text, X};
use serde::Serialize;
use std::path::Path;

const MAX_SLIDES: usize = 400;
const MAX_IMAGE: u64 = 8 << 20;
const MAX_IMAGES_TOTAL: u64 = 64 << 20;

#[derive(Serialize, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Item {
    Text { text: String, level: u8 },
    Image { src: String },
    Table { rows: Vec<Vec<String>> },
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Slide {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub items: Vec<Item>,
    pub notes: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Deck {
    pub slides: Vec<Slide>,
    pub aspect: f64,
    pub title: Option<String>,
    pub meta: Vec<(String, String)>,
    pub truncated: bool,
}

pub fn read(path: &Path, format: &str) -> Res<Deck> {
    match format {
        "pptx" => pptx(path),
        "odp" => odp(path),
        other => Err(format!("Unsupported presentation format: {other}")),
    }
}

fn pptx(path: &Path) -> Res<Deck> {
    let mut z = open_zip(path)?;
    let pres = zip_text(&mut z, "ppt/presentation.xml").ok_or("ppt/presentation.xml missing")?;
    let pres_rels = zip_text(&mut z, "ppt/_rels/presentation.xml.rels")
        .map(|s| rels(&s))
        .unwrap_or_default();
    let mut order: Vec<String> = Vec::new();
    let mut aspect = 16.0 / 9.0;
    walk(&pres, |ev| match ev {
        X::Open("p:sldId", a) => {
            if let Some(t) = a.get("r:id").and_then(|id| pres_rels.get(id)) {
                order.push(resolve_part("ppt", &t.0));
            }
        }
        X::Open("p:sldSz", a) => {
            let cx: f64 = a.get("cx").and_then(|v| v.parse().ok()).unwrap_or(0.0);
            let cy: f64 = a.get("cy").and_then(|v| v.parse().ok()).unwrap_or(0.0);
            if cx > 0.0 && cy > 0.0 {
                aspect = cx / cy;
            }
        }
        _ => {}
    })?;
    let truncated = order.len() > MAX_SLIDES;
    order.truncate(MAX_SLIDES);

    let mut used = 0u64;
    let mut slides = Vec::with_capacity(order.len());
    for part in order {
        let Some(xml) = zip_text(&mut z, &part) else {
            continue;
        };
        let (dir, file) = part.rsplit_once('/').unwrap_or(("", &part));
        let rel_map = zip_text(&mut z, &format!("{dir}/_rels/{file}.rels"))
            .map(|s| rels(&s))
            .unwrap_or_default();
        let mut slide = parse_pptx_slide(&xml);
        // Resolve images.
        for item in slide.items.iter_mut() {
            if let Item::Image { src } = item {
                let rid = std::mem::take(src);
                if let Some((t, false)) = rel_map.get(&rid) {
                    let p = resolve_part(dir, t);
                    if used < MAX_IMAGES_TOTAL {
                        if let Some(b) = zip_bytes(&mut z, &p, MAX_IMAGE) {
                            used += b.len() as u64;
                            *src = data_uri(image_mime_from_name(&p), &b);
                        }
                    }
                }
            }
        }
        slide
            .items
            .retain(|i| !matches!(i, Item::Image { src } if src.is_empty()));
        // Speaker notes.
        if let Some((t, _)) = rel_map.values().find(|(t, _)| t.contains("notesSlide")) {
            if let Some(nx) = zip_text(&mut z, &resolve_part(dir, t)) {
                slide.notes = notes_text(&nx);
            }
        }
        slides.push(slide);
    }
    let (title, meta) = crate::office::ooxml_core_props(&mut z);
    Ok(Deck {
        slides,
        aspect,
        title,
        meta,
        truncated,
    })
}

fn parse_pptx_slide(xml: &str) -> Slide {
    let mut s = Slide::default();
    let mut ph: Option<String> = None;
    let mut para = String::new();
    let mut level = 0u8;
    let mut in_t = false;
    let mut shape_paras: Vec<(String, u8)> = Vec::new();
    let mut table: Option<Vec<Vec<String>>> = None;
    let mut cell: Option<String> = None;
    let _ = walk(xml, |ev| match ev {
        X::Open("p:sp", _) => {
            ph = None;
            shape_paras.clear();
        }
        X::Open("p:ph", a) => ph = Some(a.get("type").unwrap_or("body").to_string()),
        X::Open("a:p", _) => {
            para.clear();
            level = 0;
        }
        X::Open("a:pPr", a) => level = a.get("lvl").and_then(|v| v.parse().ok()).unwrap_or(0),
        X::Open("a:t", _) => in_t = true,
        X::Close("a:t") => in_t = false,
        X::Text(t) if in_t => para.push_str(t),
        X::Open("a:br", _) => para.push('\n'),
        X::Close("a:p") => {
            if let Some(c) = cell.as_mut() {
                if !c.is_empty() {
                    c.push('\n');
                }
                c.push_str(&para);
            } else if !para.trim().is_empty() {
                shape_paras.push((para.clone(), level));
            }
        }
        X::Close("p:sp") => {
            let text = shape_paras
                .iter()
                .map(|(t, _)| t.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            match ph.as_deref() {
                Some("title" | "ctrTitle") if s.title.is_none() => s.title = Some(text),
                Some("subTitle") if s.subtitle.is_none() => s.subtitle = Some(text),
                Some("sldNum" | "dt" | "ftr") => {}
                _ => {
                    for (t, l) in shape_paras.drain(..) {
                        s.items.push(Item::Text { text: t, level: l });
                    }
                }
            }
            shape_paras.clear();
        }
        X::Open("a:blip", a) => {
            if let Some(r) = a.get("r:embed") {
                s.items.push(Item::Image { src: r.to_string() });
            }
        }
        X::Open("a:tbl", _) => table = Some(Vec::new()),
        X::Open("a:tr", _) => {
            if let Some(t) = table.as_mut() {
                t.push(Vec::new());
            }
        }
        X::Open("a:tc", _) => cell = Some(String::new()),
        X::Close("a:tc") => {
            if let (Some(t), Some(c)) = (table.as_mut(), cell.take()) {
                if let Some(r) = t.last_mut() {
                    r.push(c);
                }
            }
        }
        X::Close("a:tbl") => {
            if let Some(rows) = table.take() {
                s.items.push(Item::Table { rows });
            }
        }
        _ => {}
    });
    s
}

fn notes_text(xml: &str) -> Option<String> {
    let mut out = Vec::new();
    let mut in_body = false;
    let mut in_t = false;
    let mut para = String::new();
    let _ = walk(xml, |ev| match ev {
        X::Open("p:ph", a) => in_body = a.get("type") == Some("body"),
        X::Open("p:sp", _) => in_body = false,
        X::Open("a:t", _) => in_t = true,
        X::Close("a:t") => in_t = false,
        X::Text(t) if in_t && in_body => para.push_str(t),
        X::Close("a:p") => {
            if !para.trim().is_empty() {
                out.push(std::mem::take(&mut para));
            }
            para.clear();
        }
        _ => {}
    });
    (!out.is_empty()).then(|| out.join("\n"))
}

fn odp(path: &Path) -> Res<Deck> {
    let mut z = open_zip(path)?;
    let content = zip_text(&mut z, "content.xml").ok_or("content.xml missing")?;
    let mut slides: Vec<Slide> = Vec::new();
    let mut class: Option<String> = None;
    let mut in_notes = false;
    let mut para = String::new();
    let mut in_p = 0usize;
    let mut list_depth = 0u8;
    let mut frame_paras: Vec<(String, u8)> = Vec::new();
    let mut table: Option<Vec<Vec<String>>> = None;
    let mut cell: Option<String> = None;
    let mut images: Vec<(usize, usize, String)> = Vec::new();
    walk(&content, |ev| match ev {
        X::Open("draw:page", _) => {
            if slides.len() < MAX_SLIDES {
                slides.push(Slide::default());
            }
        }
        X::Open("presentation:notes", _) => in_notes = true,
        X::Close("presentation:notes") => in_notes = false,
        _ if slides.is_empty() => {}
        X::Open("draw:frame", a) => {
            class = a.get("presentation:class").map(str::to_string);
            frame_paras.clear();
        }
        X::Open("text:list", _) => list_depth += 1,
        X::Close("text:list") => list_depth = list_depth.saturating_sub(1),
        X::Open("text:p" | "text:h", _) => {
            in_p += 1;
            para.clear();
        }
        X::Text(t) if in_p > 0 => para.push_str(t),
        X::Open("text:s", _) if in_p > 0 => para.push(' '),
        X::Open("text:line-break", _) if in_p > 0 => para.push('\n'),
        X::Close("text:p" | "text:h") => {
            in_p = in_p.saturating_sub(1);
            if let Some(c) = cell.as_mut() {
                if !c.is_empty() {
                    c.push('\n');
                }
                c.push_str(&para);
            } else if in_notes {
                if let Some(s) = slides.last_mut() {
                    if !para.trim().is_empty() {
                        let n = s.notes.get_or_insert_with(String::new);
                        if !n.is_empty() {
                            n.push('\n');
                        }
                        n.push_str(&para);
                    }
                }
            } else if !para.trim().is_empty() {
                frame_paras.push((para.clone(), list_depth.saturating_sub(1)));
            }
            para.clear();
        }
        X::Open("draw:image", a) if !in_notes => {
            let si = slides.len() - 1;
            if let (Some(h), Some(s)) = (a.get("xlink:href"), slides.last_mut()) {
                if !h.contains("://") {
                    images.push((si, s.items.len(), h.trim_start_matches("./").to_string()));
                    s.items.push(Item::Image { src: String::new() });
                }
            }
        }
        X::Open("table:table", _) => table = Some(Vec::new()),
        X::Open("table:table-row", _) => {
            if let Some(t) = table.as_mut() {
                t.push(Vec::new());
            }
        }
        X::Open("table:table-cell", _) => cell = Some(String::new()),
        X::Close("table:table-cell") => {
            if let (Some(t), Some(c)) = (table.as_mut(), cell.take()) {
                if let Some(r) = t.last_mut() {
                    r.push(c);
                }
            }
        }
        X::Close("table:table") => {
            if let (Some(rows), Some(s)) = (table.take(), slides.last_mut()) {
                s.items.push(Item::Table { rows });
            }
        }
        X::Close("draw:frame") if !in_notes => {
            if let Some(s) = slides.last_mut() {
                let text = frame_paras
                    .iter()
                    .map(|(t, _)| t.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                match class.as_deref() {
                    Some("title") if s.title.is_none() && !text.is_empty() => s.title = Some(text),
                    Some("subtitle") if s.subtitle.is_none() && !text.is_empty() => {
                        s.subtitle = Some(text)
                    }
                    Some("page-number" | "date-time" | "footer") => {}
                    _ => {
                        for (t, l) in frame_paras.drain(..) {
                            s.items.push(Item::Text { text: t, level: l });
                        }
                    }
                }
            }
            frame_paras.clear();
            class = None;
        }
        _ => {}
    })?;
    let mut used = 0u64;
    for (si, ii, part) in images {
        if used > MAX_IMAGES_TOTAL {
            break;
        }
        if let Some(b) = zip_bytes(&mut z, &part, MAX_IMAGE) {
            used += b.len() as u64;
            if let Some(Item::Image { src }) = slides.get_mut(si).and_then(|s| s.items.get_mut(ii))
            {
                *src = data_uri(image_mime_from_name(&part), &b);
            }
        }
    }
    for s in slides.iter_mut() {
        s.items
            .retain(|i| !matches!(i, Item::Image { src } if src.is_empty()));
    }
    let (title, meta) = crate::office::odf_meta(&mut z);
    Ok(Deck {
        slides,
        aspect: 16.0 / 9.0,
        title,
        meta,
        truncated: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::office::tests::make_zip;

    #[test]
    fn pptx_basic() {
        let d = std::env::temp_dir().join(format!("alook-slides-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("t.pptx");
        make_zip(&p, &[
            ("ppt/presentation.xml", br#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst><p:sldSz cx="12192000" cy="6858000"/></p:presentation>"#),
            ("ppt/_rels/presentation.xml.rels", br#"<Relationships><Relationship Id="rId2" Target="slides/slide1.xml"/></Relationships>"#),
            ("ppt/slides/slide1.xml", br#"<p:sld xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree>
<p:sp><p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>Hello</a:t></a:r></a:p></p:txBody></p:sp>
<p:sp><p:txBody><a:p><a:r><a:t>Point one</a:t></a:r></a:p><a:p><a:pPr lvl="1"/><a:r><a:t>Sub</a:t></a:r></a:p></p:txBody></p:sp>
</p:spTree></p:cSld></p:sld>"#),
        ]);
        let deck = read(&p, "pptx").unwrap();
        assert_eq!(deck.slides.len(), 1);
        assert_eq!(deck.slides[0].title.as_deref(), Some("Hello"));
        assert_eq!(deck.slides[0].items.len(), 2);
        assert!((deck.aspect - 16.0 / 9.0).abs() < 0.01);
    }
}
