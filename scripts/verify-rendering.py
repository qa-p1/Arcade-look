#!/usr/bin/env python3
"""Check the normal debug binary through its second-instance channel under Xvfb."""

import argparse
import importlib.util
import os
from pathlib import Path
import subprocess
import time


ROOT = Path(__file__).resolve().parents[1]
LINK = ROOT.parent / "Arcade-link"


def load(name, path, **injected):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    module.__dict__.update(injected)
    spec.loader.exec_module(module)
    return module


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--peers", action="store_true")
    args = parser.parse_args()
    if os.environ.get("ARCADE_E2E_INNER") != "1":
        parser.error("run through Arcade-link/tools/e2e.py run --")
    os.environ.setdefault("ARCADE_E2E_SHOTS", str(LINK / ".orch/shots"))
    runner = load("look_runner", LINK / "tools/e2e.py")
    fixtures = load("look_fixtures", LINK / "tools/e2e_checks/look.py",
                    check=lambda group: lambda f: f, APPS=runner.APPS, CLI=runner.CLI)
    runner.APPS["arcade.look"]["bin"] = "src-tauri/target/debug/arcade-look"
    runner.APPS["arcade.look"]["args"] = ["--service"]
    session = runner.Session(Path(os.environ["ARCADE_E2E_ROOT"]))
    session.env.update(ALOOK_DEBUG="1", ALOOK_E2E_MAP_EARLY="1")
    session.env.pop("ALOOK_E2E_CONTROL", None)
    log = session.root / "arcade.look.log"
    executable = ROOT / "src-tauri/target/debug/arcade-look"
    scenario = "peers" if args.peers else "alone"

    def wait_log(text, offset=0):
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            if log.exists() and text in log.read_text(errors="replace")[offset:]:
                return
            time.sleep(.05)
        raise AssertionError(f"missing {text}: {session.log('arcade.look')}")

    def second_instance(*arguments):
        offset = log.stat().st_size if log.exists() else 0
        result = subprocess.run([str(executable), *map(str, arguments)], env=session.env,
                                capture_output=True, text=True, timeout=20)
        assert result.returncode == 0, result.stderr
        return offset

    try:
        if args.peers:
            fixtures.seed_real_clipboard(session)
            for app in ("arcade.box", "arcade.lens", "arcade.wheel"):
                session.start(app)
        session.start("arcade.look")
        manifests = {p.stem for p in (session.root / "arcade/apps").glob("*.json")}
        expected = set(runner.APPS) if args.peers else {"arcade.look"}
        assert manifests == expected, manifests
        sample = session.root / "Rendered folder"
        sample.mkdir()
        image = fixtures.png(sample / "Rendered image.png", 480, 300)
        pdf = fixtures.pdf(sample / "Rendered document.pdf")
        text = sample / "Rendered text.txt"
        text.write_text("Arcade Look rendering check\n\nThis text arrived through the second-instance channel.\nImages, PDFs, text and folders work with or without peers.\n")
        for path, kind, viewer in ((image, "image", "image"), (pdf, "pdf", "pdf"), (text, "text", "code"), (sample, "folder", "folder")):
            offset = second_instance(path)
            wait_log(f'open Some("{path}") from Local', offset)
            wait_log(f"mounted with {viewer}", offset)
            window = session.wait_window(path.name, timeout=20)
            session.xdotool("windowraise", window)
            time.sleep(.7)
            session.screenshot(f"look-final-{scenario}-{kind}", window)
            print(f"PASS {scenario}: second-instance {kind} rendered", flush=True)
        offset = second_instance("--settings")
        wait_log("open settings", offset)
        window = session.wait_window("Settings.*Arcade Look", timeout=20)
        time.sleep(.7)
        session.screenshot(f"look-final-{scenario}-settings", window)
        session.xdotool("windowfocus", window)
        session.xdotool("mousemove", "--window", window, "800", "600")
        session.xdotool("click", "--repeat", "12", "--delay", "60", "5")
        session.xdotool("click", "--repeat", "2", "--delay", "60", "4")
        time.sleep(.4)
        session.screenshot(f"look-final-{scenario}-connected-apps", window)
        contents = log.read_text(errors="replace")
        assert "frontend ready" in contents
        assert "page load Finished: tauri://localhost" in contents, contents[-2000:]
        assert "http://localhost:1420" not in contents
        print(f"PASS {scenario}: settings rendered from embedded frontend", flush=True)
    finally:
        session.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
