#!/usr/bin/env python3
"""Generate sample files for every format Arcade Look previews.

Usage: scripts/make-fixtures.py OUT_DIR

Pure-stdlib for document formats; uses ImageMagick (`convert`), `ffmpeg` and `7z`
for media and archives when they are installed (skipped otherwise).
"""
import base64, gzip, io, json, math, os, shutil, struct, subprocess, sys, tarfile, zipfile, zlib

out = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else "fixtures")
os.makedirs(out, exist_ok=True)
P = lambda *a: os.path.join(out, *a)
have = lambda tool: shutil.which(tool) is not None
run = lambda *cmd: subprocess.run(cmd, check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def write(name, data, mode="w"):
    with open(P(name), mode) as f:
        f.write(data)


def zip_write(name, files):
    with zipfile.ZipFile(P(name), "w", zipfile.ZIP_DEFLATED) as z:
        for n, d in files.items():
            z.writestr(n, d)


# ------------------------------------------------------------------ images
def png(w, h, pixel):
    raw = b"".join(b"\x00" + b"".join(bytes(pixel(x, y)) for x in range(w)) for y in range(h))
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b"")

def gradient(x, y, w=640, h=400):
    r = int(255 * x / w); g = int(255 * y / h); b = int(128 + 127 * math.sin((x + y) / 40))
    d = math.hypot(x - w / 2, y - h / 2); a = 255 if d < 170 else max(0, int(255 - (d - 170) * 3))
    return (r, g, b, a)

write("gradient.png", png(640, 400, gradient), "wb")
sprite = ["....XXXX....", "..XXOOOOXX..", ".XOOOOOOOOX.", "XOO.OOOO.OOX", "XOOOOOOOOOOX", "XOO.OOOO.OOX", ".XOO....OOX.", "..XXOOOOXX..", "....XXXX...."]
colors = {".": (0, 0, 0, 0), "X": (40, 30, 90, 255), "O": (250, 200, 60, 255)}
write("pixel-art.png", png(12, 9, lambda x, y: colors[sprite[y][x]]), "wb")
write("logo.svg", """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200" width="400" height="400">
<defs><linearGradient id="g" x1="0" x2="1"><stop offset="0" stop-color="#8b5cf6"/><stop offset="1" stop-color="#0ea5e9"/></linearGradient></defs>
<rect width="200" height="200" rx="40" fill="url(#g)"/><circle cx="100" cy="100" r="50" fill="#fff"/><circle cx="100" cy="100" r="24" fill="#312e81"/>
<script>alert('svg scripts must never run')</script></svg>""")
if have("convert"):
    run("convert", "-size", "1600x1000", "plasma:fractal", "-seed", "7", P("photo.jpg"))
    run("convert", P("gradient.png"), P("scan.tiff"))
    run("convert", P("gradient.png"), P("layered.psd"))
    run("convert", P("gradient.png"), P("texture.tga"))
    run("convert", "-delay", "20", "-size", "120x120", "xc:#8b5cf6", "xc:#6366f1", "xc:#0ea5e9", "-loop", "0", P("animated.gif"))
    run("convert", P("gradient.png"), "-quality", "80", P("modern.webp"))

# ------------------------------------------------------------------ media
if have("ffmpeg"):
    run("ffmpeg", "-y", "-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=30:duration=6", "-f", "lavfi", "-i", "sine=frequency=440:duration=6",
        "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", P("clip.mp4"))
    run("ffmpeg", "-y", "-f", "lavfi", "-i", "testsrc=size=640x360:rate=25:duration=4", "-c:v", "libvpx", "-b:v", "500k", P("clip.webm"))
    run("ffmpeg", "-y", "-f", "lavfi", "-i", "sine=frequency=330:duration=12", "-f", "lavfi", "-i", "sine=frequency=495:duration=12",
        "-filter_complex", "[0][1]amix=inputs=2,volume=2,afade=t=in:d=2,afade=t=out:st=9:d=3,tremolo=f=2:d=0.7[a]", "-map", "[a]", "-ar", "44100", P("tone.wav"))
    if os.path.exists(P("photo.jpg")):
        run("ffmpeg", "-y", "-i", P("tone.wav"), "-i", P("photo.jpg"), "-map", "0", "-map", "1", "-c:a", "libmp3lame", "-b:a", "192k", "-c:v", "mjpeg",
            "-id3v2_version", "3", "-metadata", "title=Arcade Sunrise", "-metadata", "artist=The Previewers", "-metadata", "album=Quick Looks",
            "-metadata", "date=2026", "-metadata", "genre=Synthwave", "-metadata", "track=3/10", "-metadata:s:v", "comment=Cover (front)", P("song.mp3"))
    run("ffmpeg", "-y", "-i", P("tone.wav"), "-c:a", "flac", P("tone.flac"))

# ------------------------------------------------------------------ PDF (hand-written, 3 pages)
def make_pdf(pages):
    objs = []
    def add(s):
        objs.append(s); return len(objs)
    font = add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
    bold = add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>")
    page_ids = []
    parent_placeholder = "__PARENT__"
    for i, (title, lines) in enumerate(pages):
        ops = [f"BT /F2 28 Tf 72 720 Td ({title}) Tj ET"]
        y = 680
        for ln in lines:
            ops.append(f"BT /F1 13 Tf 72 {y} Td ({ln}) Tj ET"); y -= 22
        ops.append("0.42 0.36 0.99 rg 72 120 468 6 re f")
        ops.append(f"BT /F1 10 Tf 72 90 Td (Page {i + 1} of {len(pages)}) Tj ET")
        stream = "\n".join(ops).encode()
        content = add(f"<< /Length {len(stream)} >>\nstream\n{stream.decode()}\nendstream")
        page_ids.append(add(f"<< /Type /Page /Parent {parent_placeholder} /MediaBox [0 0 612 792] /Contents {content} 0 R /Resources << /Font << /F1 {font} 0 R /F2 {bold} 0 R >> >> >>"))
    pages_id = add(f"<< /Type /Pages /Kids [{' '.join(f'{p} 0 R' for p in page_ids)}] /Count {len(page_ids)} >>")
    info = add("<< /Title (Arcade Look Test Document) /Author (Fixture Generator) /Producer (make-fixtures.py) >>")
    catalog = add(f"<< /Type /Catalog /Pages {pages_id} 0 R >>")
    body = b"%PDF-1.4\n"
    offsets = []
    for i, o in enumerate(objs, 1):
        offsets.append(len(body))
        body += f"{i} 0 obj\n{o.replace(parent_placeholder, f'{pages_id} 0 R')}\nendobj\n".encode()
    xref = len(body)
    body += f"xref\n0 {len(objs) + 1}\n0000000000 65535 f \n".encode()
    body += "".join(f"{o:010d} 00000 n \n" for o in offsets).encode()
    body += f"trailer\n<< /Size {len(objs) + 1} /Root {catalog} 0 R /Info {info} 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
    return body

write("report.pdf", make_pdf([
    ("Quarterly Preview Report", ["Arcade Look renders PDFs with pdf.js.", "Text on this page is selectable.", "Pages render lazily as you scroll.", "", "Section 1: Speed", "Warm previews open in milliseconds."]),
    ("Section 2: Formats", ["Images, video, audio, PDFs, archives,", "code, fonts, Markdown, JSON, Office", "documents, 3D models and more."]),
    ("Appendix", ["Thanks for reading!"]),
]), "wb")

# ------------------------------------------------------------------ text formats
os.makedirs(P("docs"), exist_ok=True)
shutil.copy(P("gradient.png"), P("docs", "screenshot.png"))
write("README.md", """---
title: Arcade Look fixtures
tags: [test, markdown]
---

<p align="center"><img src="docs/screenshot.png" width="220" alt="logo"></p>

# Arcade Look Fixtures

A **Markdown** file with _everything_: [a link](https://example.com), `inline code`, and a [local link](docs/notes.md).

> [!NOTE]
> GitHub-style alerts render as callouts.

## Table

| Format | Viewer | Fast? |
|:-------|:------:|------:|
| PNG    | image  | ✅ |
| PDF    | pdf.js | ✅ |

## Tasks

- [x] Render Markdown
- [ ] Take over the world

## Code

```rust
fn main() {
    println!("Hello from Arcade Look! {}", 42);
}
```

<details><summary>Click to expand</summary>

Hidden content with <kbd>Ctrl</kbd>+<kbd>C</kbd>.

</details>

<script>alert("scripts never run")</script>
<a href="javascript:alert(1)">bad link</a>

Footnote reference[^1].

[^1]: The footnote.
""")
write(os.path.join("docs", "notes.md"), "# Notes\n\nYou followed a local link. Press **Backspace** to go back.\n")
write("main.rs", """use std::collections::HashMap;

/// A tiny word counter.
fn count_words(text: &str) -> HashMap<&str, usize> {
    let mut map = HashMap::new();
    for word in text.split_whitespace() {
        *map.entry(word).or_insert(0) += 1;
    }
    map
}

fn main() {
    let text = "the quick brown fox jumps over the lazy dog the end";
    let counts = count_words(text);
    let mut v: Vec<_> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    for (w, c) in v.iter().take(3) {
        println!("{w:>8}: {c}");
    }
}
""")
write("app.py", "#!/usr/bin/env python3\n\"\"\"Example script.\"\"\"\nimport sys\n\n\nclass Greeter:\n    def __init__(self, name: str) -> None:\n        self.name = name\n\n    def greet(self) -> str:\n        return f\"Hello, {self.name}!\"\n\n\nif __name__ == \"__main__\":\n    print(Greeter(sys.argv[1] if len(sys.argv) > 1 else \"world\").greet())\n")
write("notes.txt", "Plain text notes.\n\nLine three.\n" + "\n".join(f"Line {i}: lorem ipsum dolor sit amet" for i in range(4, 200)) + "\n")
write("latin1.txt", "Caf\xe9 cr\xe8me br\xfbl\xe9e — encoded as Windows-1252.\n".encode("cp1252"), "wb")
write("data.json", json.dumps({"name": "Arcade Look", "version": "0.1.0", "fast": True, "rating": 4.9, "tags": ["preview", "quick look", "cross-platform"],
    "nested": {"deep": {"deeper": {"deepest": [1, 2, 3]}}, "nothing": None},
    "items": [{"id": i, "label": f"Item {i}", "price": round(i * 1.37, 2)} for i in range(1, 60)]}, indent=2))
write("events.jsonl", "\n".join(json.dumps({"ts": 1700000000 + i, "event": ["click", "view", "buy"][i % 3], "user": f"u{i % 7}"}) for i in range(30)) + "\n")
write("people.csv", "name,city,age,score\n" + "\n".join(f"\"Person {i}, Jr.\",{['Berlin','Tokyo','Lima','Oslo'][i % 4]},{20 + i % 50},{(i * 37) % 1000 / 10}" for i in range(1, 2001)) + "\n")
write("page.html", """<!doctype html><html><head><title>Sandboxed Page</title><style>body{font-family:sans-serif;padding:40px;background:linear-gradient(135deg,#fdf2f8,#eef2ff)}h1{color:#6d28d9}</style></head>
<body><h1>Hello, HTML</h1><p>This page renders in a sandboxed frame.</p><img src="gradient.png" width="200"><script>document.body.innerHTML='<h1>SCRIPT RAN (bad)</h1>'</script></body></html>""")
write("Dockerfile", "FROM rust:1.88 AS build\nWORKDIR /src\nCOPY . .\nRUN cargo build --release\n\nFROM debian:stable-slim\nCOPY --from=build /src/target/release/app /usr/local/bin/app\nCMD [\"app\"]\n")

# ------------------------------------------------------------------ notebook
nb = {"nbformat": 4, "nbformat_minor": 5, "metadata": {"kernelspec": {"display_name": "Python 3", "language": "python", "name": "python3"}, "language_info": {"name": "python"}},
      "cells": [
          {"cell_type": "markdown", "metadata": {}, "source": ["# Analysis\n", "Some **markdown** in a notebook."]},
          {"cell_type": "code", "execution_count": 1, "metadata": {}, "source": ["import math\n", "print([round(math.sqrt(i), 2) for i in range(5)])"], "outputs": [{"output_type": "stream", "name": "stdout", "text": ["[0.0, 1.0, 1.41, 1.73, 2.0]\n"]}]},
          {"cell_type": "code", "execution_count": 2, "metadata": {}, "source": ["df.head()"], "outputs": [{"output_type": "execute_result", "execution_count": 2, "metadata": {}, "data": {"text/html": ["<table><thead><tr><th></th><th>a</th><th>b</th></tr></thead><tbody><tr><th>0</th><td>1</td><td>x</td></tr><tr><th>1</th><td>2</td><td>y</td></tr></tbody></table>"], "text/plain": ["   a  b"]}}]},
          {"cell_type": "code", "execution_count": 3, "metadata": {}, "source": ["plot()"], "outputs": [{"output_type": "display_data", "metadata": {}, "data": {"image/png": base64.b64encode(png(160, 100, lambda x, y: (int(255 * x / 160), 80, 200, 255))).decode()}}]},
          {"cell_type": "code", "execution_count": 4, "metadata": {}, "source": ["1/0"], "outputs": [{"output_type": "error", "ename": "ZeroDivisionError", "evalue": "division by zero", "traceback": ["\u001b[0;31mZeroDivisionError\u001b[0m: division by zero"]}]},
      ]}
write("analysis.ipynb", json.dumps(nb, indent=1))

# ------------------------------------------------------------------ office documents
img_bytes = open(P("gradient.png"), "rb").read()
CT = '<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/>{}</Types>'
W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"'
def para(t, style=None, extra=""):
    ppr = '<w:pPr><w:pStyle w:val="%s"/></w:pPr>' % style if style else ""
    return '<w:p>%s%s<w:r><w:t xml:space="preserve">%s</w:t></w:r></w:p>' % (ppr, extra, t)
doc = f'''<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {W}><w:body>
{para("Project Proposal", "Title")}
{para("Introduction", "Heading1")}
<w:p><w:r><w:t xml:space="preserve">Arcade Look previews </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>Word documents</w:t></w:r><w:r><w:t xml:space="preserve"> with </w:t></w:r><w:r><w:rPr><w:i/></w:rPr><w:t>formatting</w:t></w:r><w:r><w:t xml:space="preserve">, </w:t></w:r><w:hyperlink r:id="rIdLink"><w:r><w:t>links</w:t></w:r></w:hyperlink><w:r><w:t xml:space="preserve"> and images.</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Fast</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Light</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Really light</w:t></w:r></w:p>
{para("Budget", "Heading2")}
<w:tbl><w:tr><w:tc><w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Item</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Cost</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:p><w:r><w:t>Coffee</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>$1,000</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
<w:p><w:r><w:drawing><wp:inline><wp:extent cx="2857500" cy="1786000"/><a:graphic><a:graphicData><a:blip r:embed="rIdImg"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>
{para("The end.")}
</w:body></w:document>'''
styles = f'<w:styles {W}><w:style w:styleId="Title"><w:name w:val="Title"/></w:style><w:style w:styleId="Heading1"><w:name w:val="heading 1"/></w:style><w:style w:styleId="Heading2"><w:name w:val="heading 2"/></w:style></w:styles>'
numbering = f'<w:numbering {W}><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/></w:lvl><w:lvl w:ilvl="1"><w:numFmt w:val="decimal"/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>'
rels = '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdImg" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/><Relationship Id="rIdLink" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com" TargetMode="External"/></Relationships>'
zip_write("proposal.docx", {
    "[Content_Types].xml": CT.format('<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'),
    "_rels/.rels": '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>',
    "word/document.xml": doc, "word/styles.xml": styles, "word/numbering.xml": numbering,
    "word/_rels/document.xml.rels": rels, "word/media/image1.png": img_bytes,
    "docProps/core.xml": '<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Project Proposal</dc:title><dc:creator>Ada Lovelace</dc:creator></cp:coreProperties>',
})

def cell(ref, v):
    return f'<c r="{ref}"><v>{v}</v></c>' if isinstance(v, (int, float)) else f'<c r="{ref}" t="inlineStr"><is><t>{v}</t></is></c>'
rows = [["Region", "Q1", "Q2", "Q3", "Q4"]] + [[f"Region {i}", i * 100, i * 120.5, i * 90, i * 150] for i in range(1, 40)]
sheet = '<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>' + "".join(
    f'<row r="{r + 1}">' + "".join(cell(f"{chr(65 + c)}{r + 1}", v) for c, v in enumerate(row)) + "</row>" for r, row in enumerate(rows)) + "</sheetData></worksheet>"
sheet2 = '<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1">' + cell("A1", "Second sheet") + "</row></sheetData></worksheet>"
zip_write("sales.xlsx", {
    "[Content_Types].xml": CT.format('<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'),
    "_rels/.rels": '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>',
    "xl/workbook.xml": '<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sales" sheetId="1" r:id="rId1"/><sheet name="Notes" sheetId="2" r:id="rId2"/></sheets></workbook>',
    "xl/_rels/workbook.xml.rels": '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/></Relationships>',
    "xl/worksheets/sheet1.xml": sheet, "xl/worksheets/sheet2.xml": sheet2,
})

P_NS = 'xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'
def slide(title, bullets, image=False):
    body = "".join(f'<a:p><a:pPr lvl="{lvl}"/><a:r><a:t>{t}</a:t></a:r></a:p>' for t, lvl in bullets)
    pic = '<p:pic><p:blipFill><a:blip r:embed="rIdImg"/></p:blipFill></p:pic>' if image else ""
    return f'<p:sld {P_NS}><p:cSld><p:spTree><p:sp><p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t>{title}</a:t></a:r></a:p></p:txBody></p:sp><p:sp><p:txBody>{body}</p:txBody></p:sp>{pic}</p:spTree></p:cSld></p:sld>'
slides = [("Arcade Look", []), ("Why it is fast", [("Native Rust core", 0), ("No bundled browser", 0), ("Lazy viewers", 1)]), ("Screenshots", [("A picture is worth a thousand words", 0)])]
files = {"[Content_Types].xml": CT.format(""),
         "ppt/presentation.xml": f'<p:presentation {P_NS}><p:sldIdLst>' + "".join(f'<p:sldId id="{256 + i}" r:id="rId{i + 1}"/>' for i in range(len(slides))) + '</p:sldIdLst><p:sldSz cx="12192000" cy="6858000"/></p:presentation>',
         "ppt/_rels/presentation.xml.rels": '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">' + "".join(f'<Relationship Id="rId{i + 1}" Target="slides/slide{i + 1}.xml"/>' for i in range(len(slides))) + "</Relationships>",
         "ppt/media/image1.png": img_bytes}
for i, (t, b) in enumerate(slides):
    files[f"ppt/slides/slide{i + 1}.xml"] = slide(t, b, image=(i == 2))
    files[f"ppt/slides/_rels/slide{i + 1}.xml.rels"] = '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdImg" Target="../media/image1.png"/></Relationships>'
zip_write("deck.pptx", files)

zip_write("letter.odt", {
    "mimetype": "application/vnd.oasis.opendocument.text",
    "content.xml": '<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"><office:automatic-styles><style:style style:name="T1" style:family="text"><style:text-properties fo:font-weight="bold"/></style:style></office:automatic-styles><office:body><office:text><text:h text:outline-level="1">Dear Reader</text:h><text:p>This is an <text:span text:style-name="T1">OpenDocument</text:span> letter.</text:p><text:list><text:list-item><text:p>First point</text:p></text:list-item><text:list-item><text:p>Second point</text:p></text:list-item></text:list></office:text></office:body></office:document-content>',
    "meta.xml": '<office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:dc="http://purl.org/dc/elements/1.1/"><office:meta><dc:title>A Letter</dc:title></office:meta></office:document-meta>',
})
write("memo.rtf", r"{\rtf1\ansi{\fonttbl{\f0 Times New Roman;}}\f0\fs24 {\b Memo:} please review the {\i attached} files.\par Thanks,\par The Team\par}")
zip_write("novel.epub", {
    "mimetype": "application/epub+zip",
    "META-INF/container.xml": '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>',
    "OEBPS/content.opf": '<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>The Preview</dc:title><dc:creator>A. Writer</dc:creator></metadata><manifest><item id="cover" href="cover.png" media-type="image/png" properties="cover-image"/><item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="ch2.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/><itemref idref="c2"/></spine></package>',
    "OEBPS/cover.png": img_bytes,
    "OEBPS/ch1.xhtml": '<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>Chapter One</h1><p>It was a bright cold day in April, and the previews were loading instantly.</p></body></html>',
    "OEBPS/ch2.xhtml": '<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>Chapter Two</h1><p>The end came quickly.</p></body></html>',
})

# ------------------------------------------------------------------ archives
zip_write("bundle.zip", {"README.md": "# Inside a zip\n\nPreviewed **in place**.\n", "src/main.rs": open(P("main.rs")).read(), "data/people.csv": open(P("people.csv")).read(), "img/gradient.png": img_bytes, "docs/": ""})
with tarfile.open(P("source.tar.gz"), "w:gz") as t:
    for f in ["main.rs", "app.py", "data.json", "logo.svg"]:
        t.add(P(f), arcname=f"project/{f}")
with gzip.open(P("server.log.gz"), "wt") as g:
    g.write("\n".join(f"2026-10-03T12:{i // 60:02d}:{i % 60:02d}Z INFO request id={i} status={[200, 200, 404, 500][i % 4]}" for i in range(500)))
if have("7z"):
    run("7z", "a", "-bd", P("assets.7z"), P("gradient.png"), P("README.md"))

# ------------------------------------------------------------------ fonts
for cand in ["/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf", "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf", "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf"]:
    if os.path.exists(cand):
        shutil.copy(cand, P(os.path.basename(cand)))
        break

# ------------------------------------------------------------------ 3D
def box_gltf():
    pos = [(-1, -1, 1), (1, -1, 1), (1, 1, 1), (-1, 1, 1), (-1, -1, -1), (1, -1, -1), (1, 1, -1), (-1, 1, -1)]
    idx = [0, 1, 2, 0, 2, 3, 1, 5, 6, 1, 6, 2, 5, 4, 7, 5, 7, 6, 4, 0, 3, 4, 3, 7, 3, 2, 6, 3, 6, 7, 4, 5, 1, 4, 1, 0]
    pb = b"".join(struct.pack("<3f", *p) for p in pos)
    ib = b"".join(struct.pack("<H", i) for i in idx)
    buf = pb + ib
    return {"asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": [0]}], "nodes": [{"mesh": 0}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "indices": 1, "material": 0}]}],
            "materials": [{"pbrMetallicRoughness": {"baseColorFactor": [0.42, 0.36, 0.99, 1], "metallicFactor": 0.3, "roughnessFactor": 0.4}}],
            "buffers": [{"uri": "cube.bin", "byteLength": len(buf)}],
            "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": len(pb)}, {"buffer": 0, "byteOffset": len(pb), "byteLength": len(ib)}],
            "accessors": [{"bufferView": 0, "componentType": 5126, "count": 8, "type": "VEC3", "min": [-1, -1, -1], "max": [1, 1, 1]},
                          {"bufferView": 1, "componentType": 5123, "count": len(idx), "type": "SCALAR"}]}, buf
g, buf = box_gltf()
write("cube.gltf", json.dumps(g))
write("cube.bin", buf, "wb")
# Torus as ASCII STL
tri = []
R, r, U, V = 1.0, 0.35, 48, 24
pt = lambda u, v: ((R + r * math.cos(v)) * math.cos(u), r * math.sin(v), (R + r * math.cos(v)) * math.sin(u))
for i in range(U):
    for j in range(V):
        u0, u1 = 2 * math.pi * i / U, 2 * math.pi * (i + 1) / U
        v0, v1 = 2 * math.pi * j / V, 2 * math.pi * (j + 1) / V
        a, b, c, d = pt(u0, v0), pt(u1, v0), pt(u1, v1), pt(u0, v1)
        tri += [(a, b, c), (a, c, d)]
write("torus.stl", "solid torus\n" + "".join(f"facet normal 0 0 0\nouter loop\n" + "".join(f"vertex {x:.5f} {y:.5f} {z:.5f}\n" for x, y, z in t) + "endloop\nendfacet\n" for t in tri) + "endsolid torus\n")
write("pyramid.obj", "o pyramid\nv 0 1.5 0\nv -1 0 -1\nv 1 0 -1\nv 1 0 1\nv -1 0 1\nf 1 2 3\nf 1 3 4\nf 1 4 5\nf 1 5 2\nf 2 5 4 3\n")

# ------------------------------------------------------------------ misc
write("blob.bin", bytes(range(256)) * 64 + os.urandom(4096), "wb")
write("empty.txt", "")
os.makedirs(P("folder", "sub"), exist_ok=True)
for n in ["a.txt", "b.md", "c.json"]:
    write(os.path.join("folder", n), f"{n}\n")
shutil.copy(P("gradient.png"), P("folder", "picture.png"))
print(f"fixtures written to {out}")
for f in sorted(os.listdir(out)):
    print(" ", f)
