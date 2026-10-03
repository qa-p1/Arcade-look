//! Word-processing documents → HTML: DOCX, ODT, EPUB (RTF lives in rtf.rs).
//! The HTML is structural only (headings, emphasis, lists, tables, links, images) and is
//! sanitised again in the webview.

use crate::util::{data_uri, escape_html, image_mime_from_name, Res};
use crate::xml::{open_zip, rels, resolve_part, walk, zip_bytes, zip_text, Attrs, X};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

const MAX_HTML: usize = 6 << 20;
const MAX_IMAGE: u64 = 8 << 20;
const MAX_IMAGES_TOTAL: u64 = 48 << 20;

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DocOut {
    pub html: String,
    pub title: Option<String>,
    pub meta: Vec<(String, String)>,
    pub truncated: bool,
}

pub fn read(path: &Path, format: &str) -> Res<DocOut> {
    match format {
        "docx" => docx(path),
        "odt" => odt(path),
        "epub" => epub(path),
        "rtf" => crate::rtf::to_html(path),
        other => Err(format!("Unsupported document format: {other}")),
    }
}

struct ImageBudget {
    used: u64,
}

impl ImageBudget {
    fn take<R: std::io::Read + std::io::Seek>(
        &mut self,
        z: &mut zip::ZipArchive<R>,
        part: &str,
    ) -> Option<String> {
        if self.used > MAX_IMAGES_TOTAL {
            return None;
        }
        let bytes = zip_bytes(z, part, MAX_IMAGE)?;
        self.used += bytes.len() as u64;
        Some(data_uri(image_mime_from_name(part), &bytes))
    }
}

fn truthy(a: &Attrs) -> bool {
    !matches!(a.get("w:val"), Some("0" | "false" | "none" | "off"))
}

#[derive(Default, Clone, Copy)]
struct RunProps {
    b: bool,
    i: bool,
    u: bool,
    s: bool,
    sup: bool,
    sub: bool,
    mark: bool,
}

impl RunProps {
    fn wrap(&self, inner: &str) -> String {
        if inner.is_empty() {
            return String::new();
        }
        let mut s = inner.to_string();
        let tags: [(bool, &str); 7] = [
            (self.b, "strong"),
            (self.i, "em"),
            (self.u, "u"),
            (self.s, "s"),
            (self.sup, "sup"),
            (self.sub, "sub"),
            (self.mark, "mark"),
        ];
        for (on, t) in tags {
            if on {
                s = format!("<{t}>{s}</{t}>");
            }
        }
        s
    }
}

#[derive(Default)]
struct Para {
    style: Option<String>,
    outline: Option<u8>,
    list: Option<(String, u8)>,
    align: Option<String>,
    html: String,
}

/// Open lists, tracked as (ordered, level).
struct Lists {
    stack: Vec<bool>,
}

impl Lists {
    fn close_all(&mut self, out: &mut String) {
        while let Some(o) = self.stack.pop() {
            out.push_str(if o { "</ol>" } else { "</ul>" });
        }
    }
    fn item(&mut self, out: &mut String, level: usize, ordered: bool, html: &str) {
        while self.stack.len() > level + 1 {
            let o = self.stack.pop().unwrap();
            out.push_str(if o { "</ol>" } else { "</ul>" });
        }
        while self.stack.len() < level + 1 {
            out.push_str(if ordered { "<ol>" } else { "<ul>" });
            self.stack.push(ordered);
        }
        out.push_str("<li>");
        out.push_str(html);
        out.push_str("</li>");
    }
}

// ---------------------------------------------------------------- DOCX

fn docx(path: &Path) -> Res<DocOut> {
    let mut z = open_zip(path)?;
    let doc = zip_text(&mut z, "word/document.xml").ok_or("word/document.xml is missing")?;
    let rel = zip_text(&mut z, "word/_rels/document.xml.rels")
        .map(|s| rels(&s))
        .unwrap_or_default();

    // styleId → (lower-cased name, outline level)
    let mut styles: HashMap<String, (String, Option<u8>)> = HashMap::new();
    if let Some(s) = zip_text(&mut z, "word/styles.xml") {
        let mut cur: Option<String> = None;
        let _ = walk(&s, |ev| match ev {
            X::Open("w:style", a) => {
                cur = a.get("w:styleId").map(str::to_string);
                if let Some(id) = &cur {
                    styles.insert(id.clone(), (id.to_lowercase(), None));
                }
            }
            X::Open("w:name", a) => {
                if let (Some(id), Some(v)) = (&cur, a.get("w:val")) {
                    if let Some(e) = styles.get_mut(id) {
                        e.0 = v.to_lowercase();
                    }
                }
            }
            X::Open("w:outlineLvl", a) => {
                if let (Some(id), Some(v)) = (&cur, a.get("w:val").and_then(|v| v.parse().ok())) {
                    if let Some(e) = styles.get_mut(id) {
                        e.1 = Some(v);
                    }
                }
            }
            X::Close("w:style") => cur = None,
            _ => {}
        });
    }

    // numId → abstractNumId, (abstractNumId, ilvl) → ordered
    let mut num_abs: HashMap<String, String> = HashMap::new();
    let mut abs_fmt: HashMap<(String, u8), bool> = HashMap::new();
    if let Some(s) = zip_text(&mut z, "word/numbering.xml") {
        let (mut cur_abs, mut cur_num, mut cur_lvl): (Option<String>, Option<String>, u8) =
            (None, None, 0);
        let _ = walk(&s, |ev| match ev {
            X::Open("w:abstractNum", a) => cur_abs = a.get("w:abstractNumId").map(str::to_string),
            X::Open("w:lvl", a) => {
                cur_lvl = a.get("w:ilvl").and_then(|v| v.parse().ok()).unwrap_or(0)
            }
            X::Open("w:numFmt", a) => {
                if let Some(ab) = &cur_abs {
                    let fmt = a.get("w:val").unwrap_or("bullet");
                    abs_fmt.insert((ab.clone(), cur_lvl), !matches!(fmt, "bullet" | "none"));
                }
            }
            X::Close("w:abstractNum") => cur_abs = None,
            X::Open("w:num", a) => cur_num = a.get("w:numId").map(str::to_string),
            X::Open("w:abstractNumId", a) => {
                if let (Some(n), Some(v)) = (&cur_num, a.get("w:val")) {
                    num_abs.insert(n.clone(), v.to_string());
                }
            }
            X::Close("w:num") => cur_num = None,
            _ => {}
        });
    }
    let ordered = |num_id: &str, lvl: u8| -> bool {
        num_abs
            .get(num_id)
            .and_then(|a| abs_fmt.get(&(a.clone(), lvl)))
            .copied()
            .unwrap_or(false)
    };

    let mut images = ImageBudget { used: 0 };
    let mut pending_images: Vec<(usize, String, Option<u32>)> = Vec::new(); // placeholder idx, rid, width

    let mut out = String::with_capacity(doc.len() / 3);
    let mut lists = Lists { stack: Vec::new() };
    let mut paras: Vec<Para> = Vec::new();
    let mut run = RunProps::default();
    let mut run_text = String::new();
    let mut in_rpr = false;
    let mut in_ppr = false;
    let mut in_t = false;
    let mut skip_depth = 0usize;
    let mut extent_w: Option<u32> = None;
    let mut truncated = false;

    let flush_run = |paras: &mut Vec<Para>, run_text: &mut String, run: &RunProps| {
        if let Some(p) = paras.last_mut() {
            p.html.push_str(&run.wrap(run_text));
        }
        run_text.clear();
    };

    walk(&doc, |ev| {
        if truncated {
            return;
        }
        if skip_depth > 0 {
            match ev {
                X::Open(..) => skip_depth += 1,
                X::Close(_) => skip_depth -= 1,
                _ => {}
            }
            return;
        }
        match ev {
            X::Open(
                "w:delText"
                | "w:instrText"
                | "w:footnoteReference"
                | "w:commentReference"
                | "mc:Fallback"
                | "w:del",
                _,
            ) => {
                skip_depth = 1;
            }
            X::Open("w:p", _) => paras.push(Para::default()),
            X::Open("w:pPr", _) => in_ppr = true,
            X::Close("w:pPr") => in_ppr = false,
            X::Open("w:pStyle", a) if in_ppr => {
                if let Some(p) = paras.last_mut() {
                    p.style = a.get("w:val").map(str::to_string);
                }
            }
            X::Open("w:outlineLvl", a) if in_ppr => {
                if let Some(p) = paras.last_mut() {
                    p.outline = a.get("w:val").and_then(|v| v.parse().ok());
                }
            }
            X::Open("w:jc", a) if in_ppr => {
                if let Some(p) = paras.last_mut() {
                    p.align = a.get("w:val").map(|v| match v {
                        "both" | "distribute" => "justify".to_string(),
                        "start" => "left".into(),
                        "end" => "right".into(),
                        o => o.to_string(),
                    });
                }
            }
            X::Open("w:ilvl", a) if in_ppr => {
                if let Some(p) = paras.last_mut() {
                    let lvl = a.get("w:val").and_then(|v| v.parse().ok()).unwrap_or(0);
                    let id = p.list.as_ref().map(|l| l.0.clone()).unwrap_or_default();
                    p.list = Some((id, lvl));
                }
            }
            X::Open("w:numId", a) if in_ppr => {
                if let Some(p) = paras.last_mut() {
                    let id = a.get("w:val").unwrap_or("0").to_string();
                    let lvl = p.list.as_ref().map(|l| l.1).unwrap_or(0);
                    p.list = if id == "0" { None } else { Some((id, lvl)) };
                }
            }
            X::Open("w:r", _) => {
                run = RunProps::default();
                run_text.clear();
            }
            X::Open("w:rPr", _) if !in_ppr => in_rpr = true,
            X::Close("w:rPr") => in_rpr = false,
            X::Open("w:b", a) if in_rpr => run.b = truthy(a),
            X::Open("w:i", a) if in_rpr => run.i = truthy(a),
            X::Open("w:u", a) if in_rpr => run.u = truthy(a),
            X::Open("w:strike" | "w:dstrike", a) if in_rpr => run.s = truthy(a),
            X::Open("w:highlight", a) if in_rpr => run.mark = truthy(a),
            X::Open("w:vertAlign", a) if in_rpr => {
                run.sup = a.get("w:val") == Some("superscript");
                run.sub = a.get("w:val") == Some("subscript");
            }
            X::Open("w:t", _) => in_t = true,
            X::Close("w:t") => in_t = false,
            X::Text(t) if in_t => run_text.push_str(&escape_html(t)),
            X::Open("w:tab", _) if !in_ppr => run_text.push_str("<span class=\"tab\"></span>"),
            X::Open("w:br", a) => {
                if a.get("w:type") == Some("page") {
                    flush_run(&mut paras, &mut run_text, &run);
                    if let Some(p) = paras.last_mut() {
                        p.html.push_str("<hr class=\"page-break\">");
                    }
                } else {
                    run_text.push_str("<br>");
                }
            }
            X::Open("w:cr", _) => run_text.push_str("<br>"),
            X::Open("w:sym", a) => {
                if let Some(c) = a
                    .get("w:char")
                    .and_then(|c| u32::from_str_radix(c, 16).ok())
                    .and_then(char::from_u32)
                {
                    run_text.push(c);
                }
            }
            X::Close("w:r") => flush_run(&mut paras, &mut run_text, &run),
            X::Open("w:hyperlink", a) => {
                flush_run(&mut paras, &mut run_text, &run);
                let href = a
                    .get("r:id")
                    .and_then(|id| rel.get(id))
                    .map(|(t, _)| t.clone())
                    .or_else(|| a.get("w:anchor").map(|x| format!("#{x}")));
                if let Some(p) = paras.last_mut() {
                    match href {
                        Some(h) => p
                            .html
                            .push_str(&format!("<a href=\"{}\">", escape_html(&h))),
                        None => p.html.push_str("<a>"),
                    }
                }
            }
            X::Close("w:hyperlink") => {
                flush_run(&mut paras, &mut run_text, &run);
                if let Some(p) = paras.last_mut() {
                    p.html.push_str("</a>");
                }
            }
            X::Open("wp:extent", a) => {
                extent_w = a
                    .get("cx")
                    .and_then(|v| v.parse::<u64>().ok())
                    .map(|emu| (emu / 9525) as u32);
            }
            X::Open("a:blip", a) => {
                if let Some(rid) = a.get("r:embed").or_else(|| a.get("r:link")) {
                    flush_run(&mut paras, &mut run_text, &run);
                    if let Some(p) = paras.last_mut() {
                        let idx = pending_images.len();
                        p.html.push_str(&format!("\u{0}IMG{idx}\u{0}"));
                        pending_images.push((idx, rid.to_string(), extent_w.take()));
                    }
                }
            }
            X::Open("v:imagedata", a) => {
                if let Some(rid) = a.get("r:id") {
                    if let Some(p) = paras.last_mut() {
                        let idx = pending_images.len();
                        p.html.push_str(&format!("\u{0}IMG{idx}\u{0}"));
                        pending_images.push((idx, rid.to_string(), None));
                    }
                }
            }
            X::Close("w:p") => {
                flush_run(&mut paras, &mut run_text, &run);
                let Some(p) = paras.pop() else { return };
                let style = p.style.as_ref().and_then(|s| styles.get(s));
                let name = style
                    .map(|s| s.0.as_str())
                    .or(p.style.as_deref())
                    .unwrap_or("");
                let heading = if name == "title" {
                    Some(1)
                } else if name == "subtitle" {
                    Some(2)
                } else if let Some(n) = name
                    .strip_prefix("heading ")
                    .or_else(|| name.strip_prefix("heading"))
                {
                    n.trim().parse::<u8>().ok().map(|n| n.clamp(1, 6))
                } else {
                    p.outline
                        .or(style.and_then(|s| s.1))
                        .filter(|l| *l < 9)
                        .map(|l| (l + 1).min(6))
                };
                let align = p
                    .align
                    .as_ref()
                    .filter(|a| matches!(a.as_str(), "center" | "right" | "justify"))
                    .map(|a| format!(" align=\"{a}\""))
                    .unwrap_or_default();
                let target: &mut String = match paras.last_mut() {
                    // Nested paragraph (text box): inline into the parent.
                    Some(parent) => {
                        parent.html.push_str(&p.html);
                        parent.html.push_str("<br>");
                        return;
                    }
                    None => &mut out,
                };
                if let Some((num, lvl)) = &p.list {
                    lists.item(target, *lvl as usize, ordered(num, *lvl), &p.html);
                } else {
                    lists.close_all(target);
                    if let Some(h) = heading {
                        let cls = if name == "title" {
                            " class=\"doc-title\""
                        } else if name == "subtitle" {
                            " class=\"doc-subtitle\""
                        } else {
                            ""
                        };
                        target.push_str(&format!("<h{h}{cls}{align}>{}</h{h}>", p.html));
                    } else if name.contains("quote") {
                        target.push_str(&format!("<blockquote><p>{}</p></blockquote>", p.html));
                    } else if p.html.is_empty() {
                        target.push_str("<p class=\"blank\"></p>");
                    } else {
                        target.push_str(&format!("<p{align}>{}</p>", p.html));
                    }
                }
                if out.len() > MAX_HTML {
                    truncated = true;
                }
            }
            X::Open("w:tbl", _) => {
                let t = paras.last_mut().map(|p| &mut p.html).unwrap_or(&mut out);
                lists.close_all(t);
                t.push_str("<table>");
            }
            X::Close("w:tbl") => paras
                .last_mut()
                .map(|p| &mut p.html)
                .unwrap_or(&mut out)
                .push_str("</table>"),
            X::Open("w:tr", _) => paras
                .last_mut()
                .map(|p| &mut p.html)
                .unwrap_or(&mut out)
                .push_str("<tr>"),
            X::Close("w:tr") => paras
                .last_mut()
                .map(|p| &mut p.html)
                .unwrap_or(&mut out)
                .push_str("</tr>"),
            X::Open("w:gridSpan", a) => {
                // Patch the most recent <td> with a colspan.
                if let Some(n) = a.get("w:val").and_then(|v| v.parse::<u32>().ok()) {
                    let t = paras.last_mut().map(|p| &mut p.html).unwrap_or(&mut out);
                    if t.ends_with("<td>") {
                        t.truncate(t.len() - 4);
                        t.push_str(&format!("<td colspan=\"{n}\">"));
                    }
                }
            }
            X::Open("w:tc", _) => {
                let t = paras.last_mut().map(|p| &mut p.html).unwrap_or(&mut out);
                lists.close_all(t);
                t.push_str("<td>");
            }
            X::Close("w:tc") => {
                let t = paras.last_mut().map(|p| &mut p.html).unwrap_or(&mut out);
                lists.close_all(t);
                t.push_str("</td>");
            }
            _ => {}
        }
    })?;
    lists.close_all(&mut out);

    // Resolve images now that the zip is free again.
    for (idx, rid, width) in pending_images {
        let marker = format!("\u{0}IMG{idx}\u{0}");
        let img = rel
            .get(&rid)
            .filter(|(_, ext)| !ext)
            .and_then(|(t, _)| images.take(&mut z, &resolve_part("word", t)))
            .map(|src| match width {
                Some(w) if w > 0 => format!("<img src=\"{src}\" width=\"{w}\" alt=\"\">"),
                _ => format!("<img src=\"{src}\" alt=\"\">"),
            })
            .unwrap_or_default();
        out = out.replacen(&marker, &img, 1);
    }

    let (title, meta) = ooxml_core_props(&mut z);
    Ok(DocOut {
        html: out,
        title,
        meta,
        truncated,
    })
}

/// docProps/core.xml + app.xml → (title, metadata rows).
pub fn ooxml_core_props<R: std::io::Read + std::io::Seek>(
    z: &mut zip::ZipArchive<R>,
) -> (Option<String>, Vec<(String, String)>) {
    let mut meta = Vec::new();
    let mut title = None;
    let collect = |xml: &str, fields: &[(&str, &str)]| {
        let mut cur: Option<&str> = None;
        let mut found: Vec<(String, String)> = Vec::new();
        let _ = walk(xml, |ev| match ev {
            X::Open(n, _) => {
                cur = fields
                    .iter()
                    .find(|(k, _)| *k == n)
                    .map(|(_, label)| *label)
            }
            X::Text(t) => {
                if let Some(label) = cur {
                    if !t.trim().is_empty() {
                        found.push((label.to_string(), t.trim().to_string()));
                    }
                }
            }
            X::Close(_) => cur = None,
        });
        found
    };
    if let Some(core) = zip_text(z, "docProps/core.xml") {
        for (k, v) in collect(
            &core,
            &[
                ("dc:title", "Title"),
                ("dc:subject", "Subject"),
                ("dc:creator", "Author"),
                ("cp:lastModifiedBy", "Last modified by"),
                ("dcterms:created", "Created"),
                ("dcterms:modified", "Modified"),
                ("cp:revision", "Revision"),
            ],
        ) {
            if k == "Title" {
                title = Some(v.clone());
            }
            meta.push((k, v));
        }
    }
    if let Some(app) = zip_text(z, "docProps/app.xml") {
        meta.extend(collect(
            &app,
            &[
                ("Pages", "Pages"),
                ("Words", "Words"),
                ("Slides", "Slides"),
                ("Application", "Application"),
                ("Company", "Company"),
            ],
        ));
    }
    (title, meta)
}

// ---------------------------------------------------------------- ODT

#[derive(Default, Clone, Copy)]
struct OdfStyle {
    run: RunProps,
    ordered: Option<bool>,
}

/// Parse ODF automatic/named styles relevant for rendering.
fn odf_styles(xml: &str, into: &mut HashMap<String, OdfStyle>) {
    let mut cur: Option<String> = None;
    let mut cur_list: Option<String> = None;
    let mut first_level_seen = false;
    let _ = walk(xml, |ev| match ev {
        X::Open("style:style", a) => cur = a.get("style:name").map(str::to_string),
        X::Close("style:style") => cur = None,
        X::Open("style:text-properties", a) => {
            if let Some(name) = &cur {
                let e = into.entry(name.clone()).or_default();
                if a.get("fo:font-weight")
                    .is_some_and(|w| w == "bold" || w.parse::<u32>().is_ok_and(|n| n >= 600))
                {
                    e.run.b = true;
                }
                if a.get("fo:font-style") == Some("italic") {
                    e.run.i = true;
                }
                if a.get("style:text-underline-style")
                    .is_some_and(|v| v != "none")
                {
                    e.run.u = true;
                }
                if a.get("style:text-line-through-style")
                    .is_some_and(|v| v != "none")
                {
                    e.run.s = true;
                }
                if let Some(pos) = a.get("style:text-position") {
                    e.run.sup = pos.starts_with("super") || pos.starts_with("33%");
                    e.run.sub = pos.starts_with("sub") || pos.starts_with("-33%");
                }
            }
        }
        X::Open("text:list-style", a) => {
            cur_list = a.get("style:name").map(str::to_string);
            first_level_seen = false;
        }
        X::Close("text:list-style") => cur_list = None,
        X::Open(n @ ("text:list-level-style-number" | "text:list-level-style-bullet"), _) => {
            if let Some(l) = &cur_list {
                if !first_level_seen {
                    first_level_seen = true;
                    into.entry(l.clone()).or_default().ordered = Some(n.ends_with("number"));
                }
            }
        }
        _ => {}
    });
}

fn odf_length_px(v: &str) -> Option<u32> {
    let num_end = v.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))?;
    let n: f64 = v[..num_end].parse().ok()?;
    let px = match &v[num_end..] {
        "cm" => n * 96.0 / 2.54,
        "mm" => n * 96.0 / 25.4,
        "in" => n * 96.0,
        "pt" => n * 96.0 / 72.0,
        "px" => n,
        _ => return None,
    };
    Some(px.max(1.0) as u32)
}

pub(crate) fn odf_meta<R: std::io::Read + std::io::Seek>(
    z: &mut zip::ZipArchive<R>,
) -> (Option<String>, Vec<(String, String)>) {
    let mut meta = Vec::new();
    let mut title = None;
    if let Some(m) = zip_text(z, "meta.xml") {
        let fields = [
            ("dc:title", "Title"),
            ("dc:subject", "Subject"),
            ("meta:initial-creator", "Author"),
            ("dc:creator", "Last modified by"),
            ("meta:creation-date", "Created"),
            ("dc:date", "Modified"),
            ("meta:generator", "Application"),
        ];
        let mut cur: Option<&str> = None;
        let _ = walk(&m, |ev| match ev {
            X::Open("meta:document-statistic", a) => {
                for (k, label) in [("meta:page-count", "Pages"), ("meta:word-count", "Words")] {
                    if let Some(v) = a.get(k) {
                        meta.push((label.to_string(), v.to_string()));
                    }
                }
            }
            X::Open(n, _) => cur = fields.iter().find(|(k, _)| *k == n).map(|(_, l)| *l),
            X::Text(t) => {
                if let Some(l) = cur {
                    if l == "Title" {
                        title = Some(t.trim().to_string());
                    }
                    meta.push((l.to_string(), t.trim().to_string()));
                }
            }
            X::Close(_) => cur = None,
        });
    }
    (title, meta)
}

fn odt(path: &Path) -> Res<DocOut> {
    let mut z = open_zip(path)?;
    let content = zip_text(&mut z, "content.xml").ok_or("content.xml is missing")?;
    let mut styles = HashMap::new();
    if let Some(s) = zip_text(&mut z, "styles.xml") {
        odf_styles(&s, &mut styles);
    }
    odf_styles(&content, &mut styles);

    let mut out = String::new();
    let mut in_body = false;
    let mut skip = 0usize;
    // Stack of open element closers so spans/links nest correctly.
    let mut closers: Vec<&'static str> = Vec::new();
    let mut list_ordered: Vec<bool> = Vec::new();
    let mut images: Vec<(String, Option<u32>)> = Vec::new();
    let mut frame_w: Option<u32> = None;
    let mut truncated = false;

    walk(&content, |ev| {
        if truncated {
            return;
        }
        if skip > 0 {
            match ev {
                X::Open(..) => skip += 1,
                X::Close(_) => skip -= 1,
                _ => {}
            }
            return;
        }
        match ev {
            X::Open("office:text", _) => in_body = true,
            X::Close("office:text") => in_body = false,
            _ if !in_body => {}
            X::Open(
                "text:note-body"
                | "office:annotation"
                | "text:tracked-changes"
                | "text:sequence-decls"
                | "office:forms"
                | "text:table-of-content-source"
                | "svg:desc"
                | "svg:title",
                _,
            ) => skip = 1,
            X::Open("text:h", a) => {
                let lvl: u8 = a
                    .get("text:outline-level")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1)
                    .clamp(1, 6);
                out.push_str(&format!("<h{lvl}>"));
                closers
                    .push(["</h1>", "</h2>", "</h3>", "</h4>", "</h5>", "</h6>"][lvl as usize - 1]);
            }
            X::Open("text:p", a) => {
                let st = a
                    .get("text:style-name")
                    .and_then(|s| styles.get(s))
                    .copied()
                    .unwrap_or_default();
                out.push_str("<p>");
                let w = st.run.wrap("\u{1}");
                let (open, close) = w.split_once('\u{1}').unwrap_or(("", ""));
                out.push_str(open);
                closers.push(leak_closer(&format!("{close}</p>")));
            }
            X::Open("text:span", a) => {
                let st = a
                    .get("text:style-name")
                    .and_then(|s| styles.get(s))
                    .copied()
                    .unwrap_or_default();
                let w = st.run.wrap("\u{1}");
                let (open, close) = w.split_once('\u{1}').unwrap_or(("", ""));
                out.push_str(open);
                closers.push(leak_closer(close));
            }
            X::Open("text:a", a) => {
                let href = a.get("xlink:href").unwrap_or("");
                out.push_str(&format!("<a href=\"{}\">", escape_html(href)));
                closers.push("</a>");
            }
            X::Open("text:list", a) => {
                let ordered = a
                    .get("text:style-name")
                    .and_then(|s| styles.get(s))
                    .and_then(|s| s.ordered)
                    .or(list_ordered.last().copied())
                    .unwrap_or(false);
                list_ordered.push(ordered);
                out.push_str(if ordered { "<ol>" } else { "<ul>" });
                closers.push(if ordered { "</ol>" } else { "</ul>" });
            }
            X::Open("text:list-item" | "text:list-header", _) => {
                out.push_str("<li>");
                closers.push("</li>");
            }
            X::Open("table:table", _) => {
                out.push_str("<table>");
                closers.push("</table>");
            }
            X::Open("table:table-row", _) => {
                out.push_str("<tr>");
                closers.push("</tr>");
            }
            X::Open("table:table-cell", a) => {
                let span = a
                    .get("table:number-columns-spanned")
                    .and_then(|v| v.parse::<u32>().ok())
                    .filter(|n| *n > 1);
                match span {
                    Some(n) => out.push_str(&format!("<td colspan=\"{n}\">")),
                    None => out.push_str("<td>"),
                }
                closers.push("</td>");
            }
            X::Open("draw:frame", a) => {
                frame_w = a.get("svg:width").and_then(odf_length_px);
                closers.push("");
            }
            X::Open("draw:image", a) => {
                if let Some(h) = a.get("xlink:href") {
                    if !h.contains("://") {
                        let idx = images.len();
                        out.push_str(&format!("\u{0}IMG{idx}\u{0}"));
                        images.push((h.trim_start_matches("./").to_string(), frame_w));
                    }
                }
                closers.push("");
            }
            X::Open("text:s", a) => {
                let n: usize = a
                    .get("text:c")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1)
                    .min(200);
                out.push_str(&" ".repeat(n));
                closers.push("");
            }
            X::Open("text:tab", _) => {
                out.push_str("<span class=\"tab\"></span>");
                closers.push("");
            }
            X::Open("text:line-break", _) => {
                out.push_str("<br>");
                closers.push("");
            }
            X::Open("text:soft-page-break", _) => closers.push(""),
            X::Open(_, _) => closers.push(""),
            X::Close("draw:frame") => {
                frame_w = None;
                if let Some(c) = closers.pop() {
                    out.push_str(c);
                }
            }
            X::Close(n) => {
                if n == "text:list" {
                    list_ordered.pop();
                }
                if let Some(c) = closers.pop() {
                    out.push_str(c);
                }
                if out.len() > MAX_HTML {
                    truncated = true;
                }
            }
            X::Text(t) => out.push_str(&escape_html(t)),
        }
    })?;

    let mut budget = ImageBudget { used: 0 };
    for (idx, (part, w)) in images.into_iter().enumerate() {
        let marker = format!("\u{0}IMG{idx}\u{0}");
        let img = budget
            .take(&mut z, &part)
            .map(|src| match w {
                Some(w) => format!("<img src=\"{src}\" width=\"{w}\" alt=\"\">"),
                None => format!("<img src=\"{src}\" alt=\"\">"),
            })
            .unwrap_or_default();
        out = out.replacen(&marker, &img, 1);
    }
    let (title, meta) = odf_meta(&mut z);
    Ok(DocOut {
        html: out,
        title,
        meta,
        truncated,
    })
}

/// Closers are tiny strings built from a handful of fixed tag combinations; intern them so
/// the closer stack can hold `&'static str` without allocating per element.
fn leak_closer(s: &str) -> &'static str {
    use std::sync::{Mutex, OnceLock};
    static POOL: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let pool = POOL.get_or_init(|| Mutex::new(HashMap::new()));
    let mut g = pool.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(v) = g.get(s) {
        return v;
    }
    let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
    g.insert(s.to_string(), leaked);
    leaked
}

// ---------------------------------------------------------------- EPUB

fn epub(path: &Path) -> Res<DocOut> {
    let mut z = open_zip(path)?;
    let container =
        zip_text(&mut z, "META-INF/container.xml").ok_or("Not an EPUB (container.xml missing)")?;
    let mut opf_path = None;
    walk(&container, |ev| {
        if let X::Open("rootfile", a) = ev {
            if opf_path.is_none() {
                opf_path = a.get("full-path").map(str::to_string);
            }
        }
    })?;
    let opf_path = opf_path.ok_or("EPUB rootfile not found")?;
    let opf = zip_text(&mut z, &opf_path).ok_or("EPUB package document missing")?;
    let opf_dir = opf_path
        .rsplit_once('/')
        .map(|(d, _)| d.to_string())
        .unwrap_or_default();

    let mut manifest: HashMap<String, (String, String, String)> = HashMap::new(); // id → (href, media-type, properties)
    let mut spine: Vec<String> = Vec::new();
    let mut meta: Vec<(String, String)> = Vec::new();
    let mut title = None;
    let mut cover_id: Option<String> = None;
    let mut cur_meta: Option<&str> = None;
    walk(&opf, |ev| match ev {
        X::Open("item" | "opf:item", a) => {
            if let (Some(id), Some(href)) = (a.get("id"), a.get("href")) {
                manifest.insert(
                    id.to_string(),
                    (
                        percent_encoding::percent_decode_str(href)
                            .decode_utf8_lossy()
                            .to_string(),
                        a.get("media-type").unwrap_or("").to_string(),
                        a.get("properties").unwrap_or("").to_string(),
                    ),
                );
            }
        }
        X::Open("itemref" | "opf:itemref", a) => {
            if let Some(id) = a.get("idref") {
                spine.push(id.to_string());
            }
        }
        X::Open("meta" | "opf:meta", a) => {
            if a.get("name") == Some("cover") {
                cover_id = a.get("content").map(str::to_string);
            }
        }
        X::Open(n, _) => {
            cur_meta = match n {
                "dc:title" => Some("Title"),
                "dc:creator" => Some("Author"),
                "dc:publisher" => Some("Publisher"),
                "dc:date" => Some("Date"),
                "dc:language" => Some("Language"),
                _ => None,
            }
        }
        X::Text(t) => {
            if let Some(k) = cur_meta {
                if k == "Title" && title.is_none() {
                    title = Some(t.trim().to_string());
                }
                meta.push((k.to_string(), t.trim().to_string()));
            }
        }
        X::Close(_) => cur_meta = None,
    })?;
    meta.push(("Chapters".into(), spine.len().to_string()));

    let mut budget = ImageBudget { used: 0 };
    let mut out = String::new();
    let cover = manifest
        .values()
        .find(|(_, _, p)| p.split_whitespace().any(|x| x == "cover-image"))
        .map(|(h, _, _)| h.clone())
        .or_else(|| {
            cover_id
                .and_then(|id| manifest.get(&id))
                .map(|(h, _, _)| h.clone())
        });
    if let Some(c) = cover {
        if let Some(src) = budget.take(&mut z, &resolve_part(&opf_dir, &c)) {
            out.push_str(&format!(
                "<figure class=\"epub-cover\"><img src=\"{src}\" alt=\"Cover\"></figure>"
            ));
        }
    }

    let mut truncated = false;
    for id in &spine {
        let Some((href, media, _)) = manifest.get(id) else {
            continue;
        };
        if !media.contains("html") && !href.ends_with("html") && !href.ends_with(".htm") {
            continue;
        }
        if out.len() > MAX_HTML {
            truncated = true;
            break;
        }
        let part = resolve_part(&opf_dir, href);
        let part_dir = part
            .rsplit_once('/')
            .map(|(d, _)| d.to_string())
            .unwrap_or_default();
        let Some(xhtml) = zip_text(&mut z, &part) else {
            continue;
        };
        out.push_str("<section class=\"chapter\">");
        xhtml_body_to_html(&xhtml, &mut out, |src| {
            budget.take(&mut z, &resolve_part(&part_dir, src))
        });
        out.push_str("</section>");
    }
    Ok(DocOut {
        html: out,
        title,
        meta,
        truncated,
    })
}

/// Re-serialise the <body> of an XHTML chapter, keeping structure and inlining images.
fn xhtml_body_to_html(
    xhtml: &str,
    out: &mut String,
    mut image: impl FnMut(&str) -> Option<String>,
) {
    let mut in_body = false;
    let mut skip = 0usize;
    let _ = walk(xhtml, |ev| {
        if skip > 0 {
            match ev {
                X::Open(..) => skip += 1,
                X::Close(_) => skip -= 1,
                _ => {}
            }
            return;
        }
        match ev {
            X::Open("body", _) => in_body = true,
            X::Close("body") => in_body = false,
            _ if !in_body => {}
            X::Open(n, _) if matches!(local(n), "script" | "style" | "head") => skip = 1,
            X::Open(n, a) => {
                let tag = local(n);
                match tag {
                    "img" | "image" => {
                        let src = a.get("src").or_else(|| a.get("xlink:href")).unwrap_or("");
                        if let Some(uri) = image(src) {
                            out.push_str(&format!(
                                "<img src=\"{uri}\" alt=\"{}\">",
                                escape_html(a.get("alt").unwrap_or(""))
                            ));
                        }
                    }
                    "svg" => {}
                    _ => {
                        out.push('<');
                        out.push_str(tag);
                        for k in ["href", "colspan", "rowspan", "id"] {
                            if let Some(v) = a.get(k) {
                                out.push_str(&format!(" {k}=\"{}\"", escape_html(v)));
                            }
                        }
                        out.push('>');
                    }
                }
            }
            X::Close(n) => {
                let tag = local(n);
                if !matches!(tag, "img" | "image" | "svg" | "br" | "hr") {
                    out.push_str(&format!("</{tag}>"));
                }
            }
            X::Text(t) => out.push_str(&escape_html(t)),
        }
    });
}

fn local(n: &str) -> &str {
    n.rsplit(':').next().unwrap_or(n)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    pub fn make_zip(path: &Path, files: &[(&str, &[u8])]) {
        let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        for (n, d) in files {
            z.start_file(*n, o).unwrap();
            z.write_all(d).unwrap();
        }
        z.finish().unwrap();
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("alook-office-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d.join(name)
    }

    #[test]
    fn docx_basic() {
        let p = tmp("t.docx");
        let doc = br#"<?xml version="1.0"?><w:document xmlns:w="w" xmlns:r="r"><w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Intro &amp; more</w:t></w:r></w:p>
<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Bold</w:t></w:r><w:r><w:t xml:space="preserve"> plain</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Item</w:t></w:r></w:p>
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
<w:p><w:hyperlink r:id="rId9"><w:r><w:t>link</w:t></w:r></w:hyperlink></w:p>
</w:body></w:document>"#;
        let styles = br#"<w:styles xmlns:w="w"><w:style w:styleId="Heading1"><w:name w:val="heading 1"/></w:style></w:styles>"#;
        let rels = br#"<Relationships><Relationship Id="rId9" Target="https://example.com" TargetMode="External"/></Relationships>"#;
        let core = br#"<cp:coreProperties xmlns:cp="c" xmlns:dc="d"><dc:title>My Doc</dc:title></cp:coreProperties>"#;
        make_zip(
            &p,
            &[
                ("word/document.xml", doc),
                ("word/styles.xml", styles),
                ("word/_rels/document.xml.rels", rels),
                ("docProps/core.xml", core),
            ],
        );
        let d = read(&p, "docx").unwrap();
        assert!(d.html.contains("<h1>Intro &amp; more</h1>"), "{}", d.html);
        assert!(d.html.contains("<strong>Bold</strong> plain"), "{}", d.html);
        assert!(d.html.contains("<ul><li>Item</li></ul>"), "{}", d.html);
        assert!(
            d.html
                .contains("<table><tr><td><p>Cell</p></td></tr></table>"),
            "{}",
            d.html
        );
        assert!(
            d.html.contains("<a href=\"https://example.com\">link</a>"),
            "{}",
            d.html
        );
        assert_eq!(d.title.as_deref(), Some("My Doc"));
    }

    #[test]
    fn odt_basic() {
        let p = tmp("t.odt");
        let content = br#"<office:document-content xmlns:office="o" xmlns:text="t" xmlns:style="s" xmlns:fo="f">
<office:automatic-styles><style:style style:name="T1"><style:text-properties fo:font-weight="bold"/></style:style></office:automatic-styles>
<office:body><office:text>
<text:h text:outline-level="2">Title</text:h>
<text:p>Hello <text:span text:style-name="T1">world</text:span><text:s text:c="2"/>!</text:p>
<text:list><text:list-item><text:p>one</text:p></text:list-item></text:list>
</office:text></office:body></office:document-content>"#;
        make_zip(&p, &[("content.xml", content)]);
        let d = read(&p, "odt").unwrap();
        assert!(d.html.contains("<h2>Title</h2>"), "{}", d.html);
        assert!(
            d.html.contains("<p>Hello <strong>world</strong>  !</p>"),
            "{}",
            d.html
        );
        assert!(
            d.html.contains("<ul><li><p>one</p></li></ul>"),
            "{}",
            d.html
        );
    }

    #[test]
    fn epub_basic() {
        let p = tmp("t.epub");
        make_zip(&p, &[
            ("META-INF/container.xml", br#"<container><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>"#),
            ("OEBPS/content.opf", br#"<package><metadata><dc:title>Book</dc:title></metadata><manifest><item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#),
            ("OEBPS/ch1.xhtml", br#"<html><head><style>x</style></head><body><h1>Chapter 1</h1><p>It was a <em>dark</em> night.</p><script>alert(1)</script></body></html>"#),
        ]);
        let d = read(&p, "epub").unwrap();
        assert_eq!(d.title.as_deref(), Some("Book"));
        assert!(
            d.html
                .contains("<h1>Chapter 1</h1><p>It was a <em>dark</em> night.</p>"),
            "{}",
            d.html
        );
        assert!(!d.html.contains("alert"));
    }

    #[test]
    fn not_a_zip_is_error() {
        let p = tmp("bad.docx");
        std::fs::write(&p, b"nope").unwrap();
        assert!(read(&p, "docx").is_err());
    }
}
