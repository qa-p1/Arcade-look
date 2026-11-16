#!/usr/bin/env python3
"""Tier 1 (headless Hyprland, Link's desktop harness) for Arcade Look's preview window.

    look_desktop.py <AppImage>

In a private profile, from the AppImage the way a user starts it (it hands over to its unpacked copy, D12):
- ID-1: the preview window's class is Look's desktop id.
- The preview takes keyboard focus when it opens, so Escape and Space reach it.
- The Escape ladder (RD-LOOK-1, LK-10) on the real compositor: with Info open the first Escape closes only Info,
  the second closes the preview.
- Space closes the preview (Quick Look's toggle).
- The fleet popup rule: the preview stays open when the pointer moves into another window.
- Opening a second file reuses the running Look instead of starting another, and --quit stops it.
Exit 0 when every check passes, 1 otherwise.
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, ".arcade-link/tools/desktop")
from arcade_desktop import Desktop, DesktopError  # noqa: E402

CLASS = "dev.arcade.Look"
d = Desktop()
home = tempfile.mkdtemp(prefix="look-desktop-")
downloads = Path(home, "Downloads")
downloads.mkdir()
app = str(downloads / "Arcade-Look.AppImage")
shutil.copy2(sys.argv[1], app)
os.chmod(app, 0o755)
env = {**os.environ, "HOME": home, "XDG_CONFIG_HOME": f"{home}/.config", "XDG_DATA_HOME": f"{home}/.local/share",
       "XDG_CACHE_HOME": f"{home}/.cache", "ARCADE_HOME": f"{home}/arcade-link", "ARCADE_NO_INSTALL_PROMPT": "1"}
# The harness marks AppImages "already unpacked" so tests keep the process they start; Look is followed through
# --status and the compositor instead, so it runs as a user's would.
env.pop("ARCADE_UNPACKED", None)
log = open(d.artifact("look.log"), "wb")
results = []

files = Path(home, "files")
files.mkdir()
first = files / "notes.txt"
first.write_text("Arcade Look desktop test\n" + "line\n" * 40)
second = files / "list.csv"
second.write_text("name,count\napples,3\npears,5\n")


def run(*args, timeout=60):
    return subprocess.run([app, *args], env=env, stdout=log, stderr=subprocess.STDOUT, timeout=timeout)


def status():
    out = subprocess.run([app, "--status"], env=env, capture_output=True, text=True, timeout=30)
    try:
        return json.loads(out.stdout.strip().splitlines()[-1])
    except (IndexError, ValueError):
        return {}


def check(name, ok, detail=""):
    status_ = "PASS" if ok else "FAIL"
    if not ok:
        d.screenshot(name.replace(" ", "-").lower()[:60])
    results.append((status_, name, detail))
    print(f"{status_} {name}{': ' + detail if detail else ''}", flush=True)
    return ok


def preview():
    """Look's preview window while it is shown (mapped and not hidden)."""
    w = d.find_window(CLASS)
    return w if w and w.get("mapped", True) and not w.get("hidden") else None


def shown(timeout=30):
    try:
        return d.wait(preview, timeout, "preview shown")
    except DesktopError:
        return None


def closed(timeout=5):
    try:
        d.wait(lambda: preview() is None, timeout, "preview closed")
        return True
    except DesktopError:
        return False


def active():
    try:
        return json.loads(d.hyprctl("-j", "activewindow")).get("address")
    except (DesktopError, ValueError):
        return None


def exact_point_in(window):
    """A point inside `window` the harness's pointer reaches exactly: its virtual pointer sends x/3840 and y/1080 to
    Hyprland, which reports the cursor truncated, so a fraction that is not exact in binary reads back a pixel short
    and the harness stops. x a multiple of 15 and y of 135 are exact (Link 1.0.12 will accept a one-pixel difference)."""
    (x, y), (w, h) = window["at"], window["size"]
    px = next((v for v in range(x + w // 2, x + w) if v % 15 == 0), x + w // 2)
    py = next((v for v in range(y, y + h) if v % 135 == 0 and v > y), y + h // 2)
    return px, py


def open_file(path):
    # The launcher hands over to the running Look (or to its unpacked copy) and exits.
    return subprocess.Popen([app, str(path)], env=env, stdout=log, stderr=subprocess.STDOUT)


other = subprocess.Popen(["/usr/bin/python3.12", str(Path(__file__).with_name("other_window.py"))],
                         stdout=log, stderr=subprocess.STDOUT)
try:
    other_window = d.wait_window(cls="arcade-other-window")

    # --- identity and focus ------------------------------------------------------------------------------------
    open_file(first)
    window = shown()
    if not check("the preview opens for a file", window is not None):
        raise SystemExit(1)
    check("ID-1: the window's class is Look's desktop id", window.get("class") == CLASS, window.get("class", ""))
    first_pid = window.get("pid")
    try:
        d.wait(lambda: active() == preview()["address"] if preview() else False, 5, "preview focused")
        check("the preview has keyboard focus when it opens", True)
    except DesktopError:
        check("the preview has keyboard focus when it opens", False, f"active {active()}")
    resident = status()
    check("--status reports the running Look", bool(resident.get("running")), json.dumps(resident))

    # --- Escape ladder: Info first, then the preview ------------------------------------------------------------
    d.send_shortcut(window["address"], "", "i")
    time.sleep(0.5)
    d.escape(window["address"])
    time.sleep(0.8)
    check("RD-LOOK-1/LK-10: Escape with Info open closes only Info", preview() is not None)
    if preview():
        d.escape(preview()["address"])
    check("the second Escape closes the preview", closed())

    # --- popup rule: the pointer moving into another window does not close the preview ---------------------------
    open_file(first)
    window = shown()
    check("the preview opens again", window is not None)
    if window:
        x, y = exact_point_in(other_window)
        d.pointer_move(x, y)
        time.sleep(1.5)
        check("the preview stays open when the pointer moves into another window", preview() is not None)
        if preview():
            d.focus(preview()["address"])
            d.send_shortcut(preview()["address"], "", "space")
        check("Space closes the preview", closed())

    # --- one Look: a second file reuses it ----------------------------------------------------------------------
    open_file(second)
    window = shown()
    check("a second file opens a preview", window is not None)
    check("the second file reuses the running Look", window is not None and window.get("pid") == first_pid,
          f"pid {first_pid} then {window.get('pid') if window else None}")
    if window:
        d.escape(window["address"])
        closed()

    run("--quit")
    try:
        d.wait(lambda: not status().get("running"), 20, "Look to quit")
        check("--quit stops Look", True)
    except DesktopError:
        check("--quit stops Look", False)
finally:
    other.terminate()
    subprocess.run([app, "--quit"], env=env, capture_output=True, timeout=30)
    log.flush()
    failed = [r for r in results if r[0] == "FAIL"]
    print(f"\n{len(results) - len(failed)}/{len(results)} checks passed", flush=True)
    if failed:  # Also when a check stopped the run early.
        print(Path(d.artifact("look.log")).read_text(errors="replace")[-6000:], flush=True)

sys.exit(1 if failed else 0)
