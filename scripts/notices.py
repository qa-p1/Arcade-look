#!/usr/bin/env python3
"""Write THIRD_PARTY_NOTICES.md: every third-party part that ships inside Arcade Look, with its licence text.

  scripts/notices.py           # rewrite THIRD_PARTY_NOTICES.md
  scripts/notices.py --check   # fail if it is out of date (scripts/check and CI run this)

Rust: the normal (non-dev, non-build) dependency graph of the shipped binary, for every target platform, from
`cargo metadata --locked`; each crate's own licence files from its source. JavaScript: the production packages in
package-lock.json (`vite build` bundles them; nothing else is in dist/), and the pdf.js decoder, font and CMap files
that vite.config.ts copies next to the app. Fonts: Inter. Arcade Link: the crate, and the files vendored from it
(VENDORED.md names the pinned tag and commit; this script reads the pins from there). Packaging: the AppImage runtime
and linuxdeploy hooks, Tauri's NSIS template, and dav1d, which the Linux build links statically. Native libraries of
the Linux AppImage: packaging/linux/native/, recorded by scripts/native-notices.py from a built image.
Identical licence texts are printed once, with every component that uses them.
"""
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "THIRD_PARTY_NOTICES.md"
MANIFEST = ROOT / "src-tauri" / "Cargo.toml"
SHIPPED = ("arcade-look",)
STANDARD = ROOT / "scripts" / "licences"
PREFERRED = ("MIT", "Apache-2.0", "Zlib", "BSD-3-Clause", "BSL-1.0", "MPL-2.0", "Unlicense")
LICENCE_FILE = re.compile(r"^(licen[cs]e|copying|notice|copyright)([-._].*)?$", re.I)
LINK_REPO = "https://github.com/qa-p1/Arcade-Link"
# The dav1d tag whose COPYING scripts/licences/dav1d-COPYING.txt holds.
DAV1D_TEXT_TAG = "1.5.0"


def normalized(text):
    lines = [line.rstrip() for line in text.replace("\r\n", "\n").replace("\r", "\n").split("\n")]
    while lines and not lines[0]:
        lines.pop(0)
    while lines and not lines[-1]:
        lines.pop()
    return "\n".join(lines)


def read(path):
    return normalized(Path(path).read_text(encoding="utf-8", errors="replace"))


def licence_texts(folder, declared=None):
    files = []
    if declared:
        files.append(folder / declared)
    if folder.is_dir():
        files += sorted(p for p in folder.iterdir() if p.is_file() and LICENCE_FILE.match(p.name))
        sub = folder / "LICENSES"
        if sub.is_dir():
            files += sorted(p for p in sub.iterdir() if p.is_file())
    seen, texts = set(), []
    for path in files:
        if path.resolve() in seen or not path.is_file():
            continue
        seen.add(path.resolve())
        text = normalized(path.read_text(encoding="utf-8", errors="replace"))
        if text:
            texts.append(text)
    return texts


def checkout_root(folder):
    """The repository root of a git dependency (licence files usually live there, not in the crate's folder)."""
    for parent in folder.parents:
        if (parent / ".git").exists() or parent.parent.name == "checkouts":
            return parent
    return None


def standard_text(expression, authors, name):
    """The SPDX text of the first preferred licence the crate offers, with its authors as copyright holders."""
    offered = re.split(r"\s+OR\s+|/", expression or "")
    # "Apache-2.0 WITH LLVM-exception": the licence text, then the exception's.
    exceptions = dict(term.split(" WITH ", 1) for term in offered if " WITH " in term)
    offered += list(exceptions)
    for spdx in PREFERRED:
        if spdx in offered:
            holders = ", ".join(re.sub(r"\s*<[^>]*>", "", a) for a in authors) or f"the {name} developers"
            text = (STANDARD / f"{spdx}.txt").read_text(encoding="utf-8")
            text = re.sub(r"Copyright \(c\) <year> <(copyright holders|owner)>\.?", f"Copyright (c) {holders}", text)
            if spdx in exceptions:
                text += "\n\n" + (STANDARD / f"{exceptions[spdx]}.txt").read_text(encoding="utf-8")
            return [normalized(f"{name} is offered under {expression}; it publishes no licence file, so the standard "
                               f"{spdx} text follows.\n\n{text}")]
    return []


def cargo_metadata():
    return json.loads(subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--all-features", "--manifest-path", str(MANIFEST)],
        cwd=ROOT, check=True, capture_output=True, text=True).stdout)


def rust_crates(meta):
    packages = {p["id"]: p for p in meta["packages"]}
    members = set(meta["workspace_members"])
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    todo = [i for i in members if packages[i]["name"] in SHIPPED]
    if len(todo) != len(SHIPPED):
        sys.exit(f"notices: expected workspace crates {SHIPPED}")
    reached = set()
    while todo:
        node = nodes[todo.pop()]
        for dep in node["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]) and dep["pkg"] not in reached:
                reached.add(dep["pkg"])
                todo.append(dep["pkg"])
    crates = []
    for pid in reached - members:
        p = packages[pid]
        folder = Path(p["manifest_path"]).parent
        texts = licence_texts(folder, p.get("license_file"))
        if not texts and p["source"] and p["source"].startswith("git+"):
            root = checkout_root(folder)
            texts = licence_texts(root) if root else []
        if not texts:
            texts = standard_text(p.get("license"), p.get("authors") or [], p["name"])
        crates.append({
            "label": f'{p["name"]} {p["version"]}',
            "licence": p.get("license") or ("see licence file" if p.get("license_file") else "unspecified"),
            "home": p.get("repository") or p.get("homepage") or "",
            "texts": texts,
        })
    return crates


def link_texts(meta):
    """The licence files of the arcade-link checkout the build uses (what the vendored files are offered under)."""
    for p in meta["packages"]:
        if p["name"] == "arcade-link" and p["source"] and p["source"].startswith("git+"):
            root = checkout_root(Path(p["manifest_path"]).parent)
            return licence_texts(root) if root else []
    return []


def js_packages():
    """Production packages of package-lock.json (dev and optional ones are build tools or Node-only)."""
    lock = json.loads((ROOT / "package-lock.json").read_text(encoding="utf-8"))["packages"]
    out, seen = [], set()
    for key, info in sorted(lock.items()):
        if not key or info.get("dev") or info.get("optional"):
            continue
        folder = ROOT / key
        name = key.split("node_modules/")[-1]
        package = json.loads((folder / "package.json").read_text(encoding="utf-8"))
        repo = package.get("repository")
        repo = repo.get("url", "") if isinstance(repo, dict) else (repo or "")
        label = f'{name} {package["version"]}'
        if label in seen:  # the same version nested under two packages (the Tauri plugins carry their own api)
            continue
        seen.add(label)
        out.append({
            "label": label,
            "licence": package.get("license", "unspecified"),
            "home": re.sub(r"^git\+|\.git$", "", repo).replace("git://", "https://"),
            "texts": licence_texts(folder),
        })
    return out


def pdf_assets():
    """vite.config.ts copies these from pdfjs-dist into dist/vendor/pdfjs/ (without quickjs); each ships with its
    own licence file, which the app's installer carries too."""
    base = ROOT / "node_modules" / "pdfjs-dist"
    version = json.loads((base / "package.json").read_text(encoding="utf-8"))["version"]
    home = "https://github.com/mozilla/pdf.js"

    def part(label, licence, folder, *names):
        return {"label": f"{label} (shipped in pdfjs-dist {version})", "licence": licence, "home": home,
                "texts": [read(base / folder / name) for name in names]}

    return [
        part("pdf.js CMaps (Adobe)", "BSD-3-Clause", "cmaps", "LICENSE"),
        part("pdf.js standard font: Foxit (PDFium)", "BSD-3-Clause", "standard_fonts", "LICENSE_FOXIT"),
        part("pdf.js standard font: Liberation Sans", "GPL-2.0 with font exception", "standard_fonts",
             "LICENSE_LIBERATION"),
        part("pdf.js JBIG2 decoder (wasm)", "BSD-3-Clause AND Apache-2.0", "wasm", "LICENSE_JBIG2",
             "LICENSE_PDFJS_JBIG2"),
        part("pdf.js OpenJPEG decoder (wasm)", "BSD-2-Clause", "wasm", "LICENSE_OPENJPEG", "LICENSE_PDFJS_OPENJPEG"),
        part("pdf.js qcms colour engine (wasm)", "MIT", "wasm", "LICENSE_QCMS", "LICENSE_PDFJS_QCMS"),
    ]


def vendored_pins():
    """(tag, commit) pairs VENDORED.md names for Arcade Link: the installer assets, the trimmer, the design kit."""
    text = (ROOT / "VENDORED.md").read_text(encoding="utf-8")
    sections = re.split(r"^## ", text, flags=re.M)
    pins = {}
    for section in sections:
        title = section.split("\n", 1)[0]
        commit = re.search(r"commit\s+`([0-9a-f]{40})`", section)
        tag = re.search(r"tag\s+`(v[0-9][^`]*)`", section)
        # "(v0.3.0 plus unreleased installer fixes)": what a commit with no tag is.
        note = re.search(r"commit\s+`[0-9a-f]{40}`\s+\(([^)]*)\)", section)
        if commit:
            key = title.strip() if section is not sections[0] else "installer"
            pins[key] = (tag.group(1) if tag else None, commit.group(1), note.group(1) if note else None)
    return pins


def link_vendored(texts):
    pins = vendored_pins()

    def entry(label, key, what):
        if key not in pins:
            sys.exit(f"notices: VENDORED.md no longer names a Link commit under {key!r}; update scripts/notices.py")
        tag, commit, note = pins[key]
        shown = f"tag {tag}, commit {commit[:12]}" if tag else f"commit {commit[:12]}, {note}" if note else f"commit {commit[:12]}"
        return {"label": f"Arcade Link {what} ({shown})", "licence": "MIT OR Apache-2.0",
                "home": f"{LINK_REPO}/tree/{commit}", "texts": texts}

    return [
        entry("shared installer, release and shortcut files (Windows installer scripts)", "installer",
              "vendored installer assets"),
        entry("WebKit trimmer C sources (built into libarcade-trim.so in the Linux AppImage)",
              "WebKit trimmer (OPEN1)", "vendored WebKit trimmer"),
        entry("design kit (stylesheet, icons, glyphs, app icons)", "Arcade design kit", "vendored design kit"),
    ]


def dav1d(meta):
    """dav1d is built from source and linked statically into the Linux binary by the dav1d-sys crate (build.yml sets
    SYSTEM_DEPS_DAV1D_BUILD_INTERNAL=always); the tag comes from that crate's build script."""
    for p in meta["packages"]:
        if p["name"] == "dav1d-sys":
            build = (Path(p["manifest_path"]).parent / "build.rs").read_text(encoding="utf-8")
            tag = re.search(r'const TAG: &str = "([^"]+)"', build).group(1)
            if tag != DAV1D_TEXT_TAG:
                sys.exit(f"notices: dav1d-sys now builds dav1d {tag}: refresh scripts/licences/dav1d-COPYING.txt "
                         "and DAV1D_TEXT_TAG")
            return {"label": f"dav1d {tag} (AV1 decoder, linked statically into the Linux AppImage)",
                    "licence": "BSD-2-Clause", "home": "https://code.videolan.org/videolan/dav1d",
                    "texts": [read(STANDARD / "dav1d-COPYING.txt")]}
    sys.exit("notices: dav1d-sys is no longer a dependency; drop it from scripts/notices.py")


def native_packages():
    """(host, packages): the Ubuntu packages bundled in the Linux AppImage, as recorded by scripts/native-notices.py."""
    folder = ROOT / "packaging" / "linux" / "native"
    if not (folder / "packages.json").is_file():
        return "", []
    recorded = json.loads((folder / "packages.json").read_text(encoding="utf-8"))
    record = recorded["packages"]
    return recorded["host"], [{
        "label": f"{name} (Ubuntu package)",
        "home": f"https://launchpad.net/ubuntu/+source/{info['source']}",
        "texts": [normalized((folder / "copyright" / f"{name}.txt").read_text(encoding="utf-8", errors="replace"))],
    } for name, info in sorted(record.items())]


def render():
    meta = cargo_metadata()
    fonts = [{"label": "Inter (typeface)", "licence": "OFL-1.1", "home": "https://rsms.me/inter/",
              "texts": [read(ROOT / "src/design/fonts/OFL.txt")]}]
    mit = (STANDARD / "MIT.txt").read_text(encoding="utf-8")
    windows = ROOT / "src-tauri" / "windows"
    packaging = [
        {"label": "AppImage type2-runtime (Linux AppImage)", "licence": "MIT",
         "home": "https://github.com/AppImage/type2-runtime",
         "texts": [normalized(mit.replace("<year> <copyright holders>", "the AppImage project contributors"))]},
        {"label": "linuxdeploy AppRun, GTK and GStreamer plugin hooks (Linux AppImage)", "licence": "MIT",
         "home": "https://github.com/linuxdeploy/linuxdeploy",
         "texts": [normalized(mit.replace("<year> <copyright holders>", "the linuxdeploy contributors"))]},
        {"label": "Tauri NSIS installer template, adapted (Windows setup)", "licence": "MIT OR Apache-2.0",
         "home": "https://github.com/tauri-apps/tauri",
         "texts": [read(windows / "LICENSE-TAURI-MIT"), read(windows / "LICENSE-TAURI-APACHE-2.0")]},
        dav1d(meta),
    ]
    sections = [("Fonts", fonts), ("JavaScript", js_packages()), ("PDF viewer assets", pdf_assets()),
                ("Rust crates", rust_crates(meta)), ("Arcade Link files vendored into Look",
                                                     link_vendored(link_texts(meta))),
                ("Packaging", packaging)]
    native_host, native = native_packages()
    missing = [c["label"] for _, group in sections for c in group if not c["texts"]]
    if missing:
        sys.exit("notices: no licence text found for " + ", ".join(sorted(missing)))

    lines = [
        "# Third-party notices",
        "",
        "Arcade Look is licensed under the MIT licence (see `LICENSE`). It includes the open-source parts below, each",
        "under its own licence. This file is generated by `scripts/notices.py`; do not edit it by hand.",
        "",
    ]
    by_text = {}
    for title, group in sections:
        lines += [f"## {title}", ""]
        for c in sorted(group, key=lambda c: c["label"].lower()):
            home = f" — {c['home']}" if c["home"] else ""
            lines.append(f"- {c['label']} ({c['licence']}){home}")
            for text in c["texts"]:
                by_text.setdefault(text, []).append(c["label"])
        lines.append("")
    if native:
        lines += [
            "## Native libraries in the Linux AppImage",
            "",
            f"The Linux AppImage bundles these {native_host} packages' libraries (WebKitGTK, GTK, GStreamer and their",
            "dependencies). They are dynamically linked and stored as separate files under `usr/lib` in the image: to",
            "use a modified version of an LGPL library, run the AppImage with `--appimage-extract`, replace the library",
            "in `squashfs-root/usr/lib`, and start `squashfs-root/AppRun`. The source of every version of each package",
            "is published by Ubuntu at the link given. Windows and macOS builds use the system's web view and bundle",
            "none of these.",
            "",
        ]
        for c in native:
            lines.append(f"- {c['label']} — {c['home']}")
            for text in c["texts"]:
                by_text.setdefault(text, []).append(c["label"])
        lines.append("")
    lines += ["## Licence texts", ""]
    for n, (text, users) in enumerate(sorted(by_text.items(), key=lambda kv: (sorted(kv[1])[0].lower(), kv[0])), 1):
        users = sorted(set(users), key=str.lower)
        lines += [f"### {n}. Used by {', '.join(users)}", "", "```text", text.replace("```", "'''"), "```", ""]
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--check", action="store_true", help="fail if THIRD_PARTY_NOTICES.md is out of date")
    args = ap.parse_args()
    text = render()
    if args.check:
        current = normalized(OUT.read_text(encoding="utf-8")) if OUT.exists() else ""
        if current != normalized(text):
            sys.exit("THIRD_PARTY_NOTICES.md is out of date: run scripts/notices.py")
        print("THIRD_PARTY_NOTICES.md is current")
        return
    OUT.write_text(text + "\n", encoding="utf-8", newline="\n")
    print(f"Wrote {OUT.name}: {len(text) // 1024} KiB")


if __name__ == "__main__":
    main()
