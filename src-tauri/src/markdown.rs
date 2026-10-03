//! Markdown → HTML with pulldown-cmark (GFM flavoured). Output is sanitised again in the
//! webview with an allow-list, so raw HTML in READMEs (centered logos, <details>…) renders
//! while scripts and event handlers never do.

use crate::util::escape_html;
use pulldown_cmark::{CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::collections::HashMap;

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
        | Options::ENABLE_GFM
        | Options::ENABLE_DEFINITION_LIST
}

/// A heading whose events are buffered until we know its text (for the anchor id).
type PendingHeading<'a> = (
    HeadingLevel,
    Option<CowStr<'a>>,
    Vec<CowStr<'a>>,
    Vec<(CowStr<'a>, Option<CowStr<'a>>)>,
    Vec<Event<'a>>,
);

pub fn to_html(src: &str) -> String {
    let parser = Parser::new_ext(src, options());
    let mut out_events: Vec<Event> = Vec::new();
    let mut slugs: HashMap<String, usize> = HashMap::new();

    let mut heading: Option<PendingHeading> = None;
    let mut meta: Option<String> = None;

    for ev in parser {
        if let Some(m) = meta.as_mut() {
            match ev {
                Event::End(TagEnd::MetadataBlock(_)) => {
                    let body = meta.take().unwrap_or_default();
                    if !body.trim().is_empty() {
                        out_events.push(Event::Html(
                            format!(
                                "<details class=\"frontmatter\"><summary>Front matter</summary><pre><code class=\"language-yaml\">{}</code></pre></details>\n",
                                escape_html(&body)
                            )
                            .into(),
                        ));
                    }
                }
                Event::Text(t) => m.push_str(&t),
                _ => {}
            }
            continue;
        }
        if let Some((_, _, _, _, buf)) = heading.as_mut() {
            if let Event::End(TagEnd::Heading(_)) = ev {
                let (level, id, classes, attrs, inner) = heading.take().unwrap();
                let id = id.unwrap_or_else(|| {
                    let text: String = inner
                        .iter()
                        .filter_map(|e| match e {
                            Event::Text(t) | Event::Code(t) => Some(t.as_ref()),
                            _ => None,
                        })
                        .collect();
                    let base = slugify(&text);
                    let n = slugs.entry(base.clone()).or_insert(0);
                    let id = if *n == 0 { base } else { format!("{base}-{n}") };
                    *n += 1;
                    id.into()
                });
                out_events.push(Event::Start(Tag::Heading {
                    level,
                    id: Some(id),
                    classes,
                    attrs,
                }));
                out_events.extend(inner);
                out_events.push(ev);
            } else {
                buf.push(ev);
            }
            continue;
        }
        match ev {
            Event::Start(Tag::MetadataBlock(_)) => meta = Some(String::new()),
            Event::Start(Tag::Heading {
                level,
                id,
                classes,
                attrs,
            }) => heading = Some((level, id, classes, attrs, Vec::new())),
            other => out_events.push(other),
        }
    }

    let mut html = String::with_capacity(src.len() * 3 / 2);
    pulldown_cmark::html::push_html(&mut html, out_events.into_iter());
    html
}

/// GitHub-style heading anchors.
pub fn slugify(text: &str) -> String {
    let mut s = String::with_capacity(text.len());
    for c in text.trim().chars() {
        if c.is_alphanumeric() {
            s.extend(c.to_lowercase());
        } else if c == ' ' || c == '-' {
            s.push('-');
        } else if c == '_' {
            s.push('_');
        }
    }
    if s.is_empty() {
        "section".into()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_gfm() {
        let h = to_html("# Hello World\n\n- [x] done\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n~~x~~");
        assert!(h.contains("<h1 id=\"hello-world\">Hello World</h1>"));
        assert!(h.contains("checkbox"));
        assert!(h.contains("<table>"));
        assert!(h.contains("<del>x</del>"));
    }

    #[test]
    fn duplicate_slugs_and_frontmatter() {
        let h = to_html("---\ntitle: x\n---\n\n## A\n\n## A\n");
        assert!(h.contains("id=\"a\""));
        assert!(h.contains("id=\"a-1\""));
        assert!(h.contains("frontmatter"));
        assert!(h.contains("title: x"));
    }

    #[test]
    fn code_block_language_class() {
        let h = to_html("```rust\nfn main() {}\n```");
        assert!(h.contains("class=\"language-rust\""));
    }
}
