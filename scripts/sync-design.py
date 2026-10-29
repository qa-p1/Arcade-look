#!/usr/bin/env python3
"""Copy the Arcade design kit from an Arcade Link checkout at the pinned tag.

    python3 scripts/sync-design.py --link ../Arcade-Link   # an existing checkout
    python3 scripts/sync-design.py                         # clones the tag

The tag is the one src-tauri/Cargo.toml pins arcade-link to. The checkout's
HEAD must be that tag's commit. The files land in src/design/ (stylesheet,
icons, format helpers, Inter, the app glyphs), src-tauri/icons/ (Look's
launcher icon set) and assets/ (the README logo), and their hashes go into the
"Arcade design kit" section of VENDORED.md, which scripts/check-vendored.py
checks. It then renders the tray icons (src-tauri/icons/tray-white.png and
tray-black.png) from Look's glyph with rsvg-convert: the glyph, monochrome
(STANDARD §8).
"""
from __future__ import annotations

import argparse
import hashlib
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
LINK_URL = "https://github.com/qa-p1/Arcade-Link"
HEADER = "Arcade design kit"
# (source in Link, destination here); a trailing slash copies the folder's files.
DESIGN = "src/design/"
FILES = [
    ("design/css/arcade.css", DESIGN + "arcade.css"),
    ("design/css/icons.ts", DESIGN + "icons.ts"),
    ("design/css/format.ts", DESIGN + "format.ts"),
    ("design/fonts/InterVariable.woff2", DESIGN + "fonts/InterVariable.woff2"),
    ("design/fonts/OFL.txt", DESIGN + "fonts/OFL.txt"),
    ("design/glyphs/", DESIGN + "glyphs/"),
    ("design/app-icons/look/tauri/", "src-tauri/icons/"),
    ("design/app-icons/look/icon.svg", "assets/icon.svg"),
    ("design/app-icons/look/png/256.png", "assets/icon.png"),
]


def pinned_tag() -> str:
    match = re.search(r'arcade-link = \{[^}]*tag = "([^"]+)"', (ROOT / "src-tauri/Cargo.toml").read_text())
    if not match:
        raise SystemExit("src-tauri/Cargo.toml doesn't pin arcade-link to a tag")
    return match.group(1)


def git(link: Path, *args: str) -> str:
    return subprocess.run(["git", "-C", str(link), *args], check=True, capture_output=True, text=True).stdout.strip()


def pairs(link: Path) -> list[tuple[Path, str, str]]:
    out = []
    for source, target in FILES:
        if source.endswith("/"):
            files = sorted(p for p in (link / source).iterdir() if p.is_file())
            if not files:
                raise SystemExit(f"{source} is empty in the Link checkout")
            out += [(p, f"{source}{p.name}", f"{target}{p.name}") for p in files]
        else:
            out.append((link / source, source, target))
    return out


def sync(link: Path, tag: str) -> None:
    commit = git(link, "rev-parse", "HEAD")
    tagged = git(link, "rev-parse", f"{tag}^{{commit}}")
    if commit != tagged:
        raise SystemExit(f"{link} is at {commit[:12]}, not {tag} ({tagged[:12]})")
    for folder in (DESIGN + "glyphs",):
        shutil.rmtree(ROOT / folder, ignore_errors=True)
    lines = []
    for path, source, target in pairs(link):
        destination = ROOT / target
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, destination)
        digest = hashlib.sha256(destination.read_bytes()).hexdigest()
        lines.append(f"{digest}  {target}")
    section = "\n".join([
        f"## {HEADER}",
        "",
        f"Copied unchanged from [Arcade Link]({LINK_URL}/tree/{tag}) tag `{tag}` (commit",
        f"`{commit}`) by `scripts/sync-design.py`; never edit these files here.",
        "License: MIT OR Apache-2.0; Inter is under the SIL Open Font License 1.1",
        f"(`{DESIGN}fonts/OFL.txt`).",
        "",
        "```text",
        *lines,
        "```",
    ])
    vendored = ROOT / "VENDORED.md"
    text = vendored.read_text()
    marker = f"## {HEADER}\n"
    if marker in text:
        start = text.index(marker)
        end = text.index("```\n", text.index("```text", start)) + len("```\n")
        text = text[:start] + section + "\n" + text[end:]
    else:
        text = text.rstrip("\n") + "\n\n" + section + "\n"
    vendored.write_text(text)
    print(f"Synced {len(lines)} design kit files from Link {tag} ({commit[:12]})")
    render_tray_icons()


def render_tray_icons() -> None:
    glyph = (ROOT / DESIGN / "glyphs" / "arcade.look.svg").read_text()
    for name, colour in (("white", "#ffffff"), ("black", "#000000")):
        out = ROOT / "src-tauri/icons" / f"tray-{name}.png"
        subprocess.run(["rsvg-convert", "-w", "64", "-h", "64", "-o", str(out)],
                       input=glyph.replace("currentColor", colour).encode(), check=True)
    print("Rendered the tray icons")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--link", type=Path, help="an Arcade Link checkout at the pinned tag")
    args = parser.parse_args(argv)
    tag = pinned_tag()
    if args.link:
        sync(args.link.resolve(), tag)
        return 0
    with tempfile.TemporaryDirectory() as temp:
        checkout = Path(temp) / "Arcade-Link"
        subprocess.run(["git", "clone", "--quiet", "--depth", "1", "--branch", tag, LINK_URL, str(checkout)], check=True)
        sync(checkout, tag)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
