//! Minimal RTF → HTML: paragraphs, bold/italic/underline, unicode and code-page escapes.
//! Destinations that aren't body text (font tables, pictures, metadata) are skipped.

use crate::office::DocOut;
use crate::text::read_capped;
use crate::util::{escape_html, Res};
use std::path::Path;

#[derive(Clone, Copy, Default)]
struct State {
    skip: bool,
    b: bool,
    i: bool,
    u: bool,
    uc: u32,
}

pub fn to_html(path: &Path) -> Res<DocOut> {
    let (bytes, size) = read_capped(path, 32 << 20)?;
    if !bytes.starts_with(b"{\\rtf") {
        return Err("Not an RTF document".into());
    }
    let html = convert(&bytes);
    Ok(DocOut {
        html,
        title: None,
        meta: Vec::new(),
        truncated: (bytes.len() as u64) < size,
    })
}

const SKIP_DESTS: &[&str] = &[
    "fonttbl",
    "colortbl",
    "stylesheet",
    "info",
    "pict",
    "header",
    "footer",
    "headerl",
    "headerr",
    "footerl",
    "footerr",
    "headerf",
    "footerf",
    "xmlnstbl",
    "listtable",
    "listoverridetable",
    "rsidtbl",
    "generator",
    "themedata",
    "colorschememapping",
    "latentstyles",
    "datastore",
    "object",
    "fldinst",
    "filetbl",
    "revtbl",
    "footnote",
    "bkmkstart",
    "bkmkend",
    "pgdsctbl",
    "mmathPr",
    "wgrffmtfilter",
    "nonshppict",
];

#[allow(unused_assignments)]
fn convert(b: &[u8]) -> String {
    let mut out = String::with_capacity(b.len() / 2);
    let mut para = String::new();
    let mut stack: Vec<State> = Vec::new();
    let mut st = State {
        uc: 1,
        ..Default::default()
    };
    let mut run = String::new();
    let mut run_fmt = (false, false, false);
    let mut pending_skip = 0u32; // chars to skip after \uN
                                 // `\ansicpg` names the code page of `\'xx` escapes.
    let mut codepage: &'static encoding_rs::Encoding = encoding_rs::WINDOWS_1252;
    // A character outside the BMP arrives as two `\uN` surrogates.
    let mut high_surrogate: Option<u32> = None;
    let mut i = 0;

    let flush_run = |run: &mut String, para: &mut String, fmt: (bool, bool, bool)| {
        if run.is_empty() {
            return;
        }
        let mut s = escape_html(run);
        if fmt.0 {
            s = format!("<strong>{s}</strong>");
        }
        if fmt.1 {
            s = format!("<em>{s}</em>");
        }
        if fmt.2 {
            s = format!("<u>{s}</u>");
        }
        para.push_str(&s);
        run.clear();
    };
    let end_para = |para: &mut String, out: &mut String| {
        if para.trim().is_empty() {
            out.push_str("<p class=\"blank\"></p>");
        } else {
            out.push_str("<p>");
            out.push_str(para);
            out.push_str("</p>");
        }
        para.clear();
    };

    macro_rules! emit_char {
        ($c:expr) => {{
            if !st.skip {
                if pending_skip > 0 {
                    pending_skip -= 1;
                } else {
                    let fmt = (st.b, st.i, st.u);
                    if fmt != run_fmt {
                        flush_run(&mut run, &mut para, run_fmt);
                        run_fmt = fmt;
                    }
                    run.push($c);
                }
            }
        }};
    }

    while i < b.len() {
        let c = b[i];
        match c {
            b'{' => {
                stack.push(st);
                i += 1;
                // "{\*\dest" marks an ignorable destination.
                if b.get(i..i + 2) == Some(b"\\*") {
                    st.skip = true;
                }
            }
            b'}' => {
                flush_run(&mut run, &mut para, run_fmt);
                st = stack.pop().unwrap_or(st);
                run_fmt = (st.b, st.i, st.u);
                i += 1;
            }
            b'\\' => {
                i += 1;
                let Some(&n) = b.get(i) else { break };
                if n.is_ascii_alphabetic() {
                    let start = i;
                    while i < b.len() && b[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let word = std::str::from_utf8(&b[start..i]).unwrap_or("");
                    let pstart = i;
                    if i < b.len() && (b[i] == b'-' || b[i].is_ascii_digit()) {
                        i += 1;
                        while i < b.len() && b[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    let param: Option<i32> = std::str::from_utf8(&b[pstart..i])
                        .ok()
                        .and_then(|s| s.parse().ok());
                    if i < b.len() && b[i] == b' ' {
                        i += 1;
                    }
                    if SKIP_DESTS.contains(&word) {
                        st.skip = true;
                        continue;
                    }
                    if st.skip {
                        continue;
                    }
                    match word {
                        "par" | "sect" => {
                            flush_run(&mut run, &mut para, run_fmt);
                            end_para(&mut para, &mut out);
                        }
                        "line" => {
                            flush_run(&mut run, &mut para, run_fmt);
                            para.push_str("<br>");
                        }
                        "page" => {
                            flush_run(&mut run, &mut para, run_fmt);
                            end_para(&mut para, &mut out);
                            out.push_str("<hr class=\"page-break\">");
                        }
                        "tab" => emit_char!('\t'),
                        "cell" => emit_char!('\t'),
                        "row" => {
                            flush_run(&mut run, &mut para, run_fmt);
                            end_para(&mut para, &mut out);
                        }
                        "b" => st.b = param != Some(0),
                        "i" => st.i = param != Some(0),
                        "ul" => st.u = param != Some(0),
                        "ulnone" => st.u = false,
                        "plain" => {
                            st.b = false;
                            st.i = false;
                            st.u = false;
                        }
                        "ansicpg" => codepage = param.and_then(code_page).unwrap_or(codepage),
                        "uc" => st.uc = param.unwrap_or(1).max(0) as u32,
                        "u" => {
                            if let Some(p) = param {
                                let code = if p < 0 { (p + 65536) as u32 } else { p as u32 };
                                match (code, high_surrogate.take()) {
                                    (0xD800..=0xDBFF, _) => high_surrogate = Some(code),
                                    (0xDC00..=0xDFFF, Some(high)) => {
                                        let full =
                                            0x10000 + ((high - 0xD800) << 10) + (code - 0xDC00);
                                        if let Some(ch) = char::from_u32(full) {
                                            emit_char!(ch);
                                        }
                                    }
                                    _ => {
                                        if let Some(ch) = char::from_u32(code) {
                                            emit_char!(ch);
                                        }
                                    }
                                }
                                pending_skip = st.uc;
                            }
                        }
                        "emdash" => emit_char!('—'),
                        "endash" => emit_char!('–'),
                        "bullet" => emit_char!('•'),
                        "lquote" => emit_char!('‘'),
                        "rquote" => emit_char!('’'),
                        "ldblquote" => emit_char!('“'),
                        "rdblquote" => emit_char!('”'),
                        "~" => emit_char!('\u{a0}'),
                        _ => {}
                    }
                } else {
                    i += 1;
                    match n {
                        b'\'' => {
                            // Collect the whole run of `\'xx` bytes: in double-byte
                            // code pages one character spans two escapes.
                            let mut bytes = Vec::new();
                            loop {
                                let hex = b
                                    .get(i..i + 2)
                                    .and_then(|h| std::str::from_utf8(h).ok())
                                    .and_then(|h| u8::from_str_radix(h, 16).ok());
                                i += 2;
                                if let Some(byte) = hex {
                                    // A `\uN` fallback counts each escaped byte.
                                    if pending_skip > 0 && !st.skip {
                                        pending_skip -= 1;
                                    } else {
                                        bytes.push(byte);
                                    }
                                }
                                if b.get(i..i + 2) != Some(b"\\'") {
                                    break;
                                }
                                i += 2;
                            }
                            let (s, _, _) = codepage.decode(&bytes);
                            for ch in s.chars() {
                                emit_char!(ch);
                            }
                        }
                        b'~' => emit_char!('\u{a0}'),
                        b'-' | b'_' => {}
                        b'\n' | b'\r' => {
                            flush_run(&mut run, &mut para, run_fmt);
                            end_para(&mut para, &mut out);
                        }
                        other => emit_char!(other as char),
                    }
                }
            }
            b'\r' | b'\n' => i += 1,
            _ => {
                // Plain text: consume a run of bytes (RTF body text is 7-bit ASCII).
                emit_char!(c as char);
                i += 1;
            }
        }
    }
    flush_run(&mut run, &mut para, run_fmt);
    if !para.is_empty() {
        end_para(&mut para, &mut out);
    }
    out
}

/// The encoding of a Windows code page number, as `\ansicpg` gives it.
fn code_page(number: i32) -> Option<&'static encoding_rs::Encoding> {
    let label = match number {
        874 | 1250..=1258 => format!("windows-{number}"),
        932 => "shift_jis".into(),
        936 => "gbk".into(),
        949 => "euc-kr".into(),
        950 => "big5".into(),
        10000 => "macintosh".into(),
        20866 => "koi8-r".into(),
        28591..=28599 => format!("iso-8859-{}", number - 28590),
        65001 => "utf-8".into(),
        _ => return None,
    };
    encoding_rs::Encoding::for_label(label.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts() {
        let rtf: &[u8] = b"{\\rtf1\\ansi{\\fonttbl{\\f0 Arial;}}{\\*\\generator x;}\\f0 Hello {\\b bold} caf\\'e9 \\u8364?\\par Next\\par}";
        let h = convert(rtf);
        assert_eq!(h, "<p>Hello <strong>bold</strong> café €</p><p>Next</p>");
    }

    #[test]
    fn surrogate_pairs_and_code_pages() {
        // 😀 is U+1F600: \u-10179 (D83D) \u-8704 (DE00), each with a `?` fallback.
        let emoji: &[u8] = b"{\\rtf1\\ansi a\\u-10179?\\u-8704?b \\u8364?\\par}";
        assert_eq!(convert(emoji), "<p>a\u{1F600}b \u{20AC}</p>");
        let cyrillic: &[u8] = b"{\\rtf1\\ansi\\ansicpg1251 \\'cf\\'f0\\'e8\\'e2\\'e5\\'f2\\par}";
        assert_eq!(
            convert(cyrillic),
            "<p>\u{41F}\u{440}\u{438}\u{432}\u{435}\u{442}</p>"
        );
        let japanese: &[u8] = b"{\\rtf1\\ansi\\ansicpg932 \\'82\\'a0\\par}";
        assert_eq!(convert(japanese), "<p>\u{3042}</p>");
        // A \u fallback written as an escape is skipped, not shown.
        let fallback: &[u8] = b"{\\rtf1\\ansi\\ansicpg1251 \\u1046\\'c6!\\par}";
        assert_eq!(convert(fallback), "<p>\u{416}!</p>");
    }
}
