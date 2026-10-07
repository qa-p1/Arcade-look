#!/usr/bin/env python3
"""Run the shared checks using Look's separate test artifacts."""

import argparse
import importlib.util
import os
from pathlib import Path
import subprocess


ROOT = Path(__file__).resolve().parents[1]
LINK = ROOT.parent / "Arcade-link"
TARGET = ROOT / "src-tauri/target/link-tests"


def look_consumer_boundaries(session):
    binaries = [p for p in (TARGET / "debug/deps").glob("link_consumer-*")
                if p.is_file() and os.access(p, os.X_OK)]
    assert binaries, "run python3 scripts/verify-link.py --build-e2e first"
    binary = max(binaries, key=lambda p: p.stat().st_mtime)
    result = subprocess.run([str(binary), "--ignored", "--nocapture", "--test-threads=1"],
                            env=session.env, capture_output=True, text=True, timeout=90)
    assert result.returncode == 0, result.stdout + result.stderr
    assert "8 passed" in result.stdout and "0 failed" in result.stdout, result.stdout
    return result.stdout.strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--only", default="look", choices=("look", "failure"))
    args = parser.parse_args()
    if os.environ.get("ARCADE_E2E_INNER") != "1":
        parser.error("run through Arcade-link/tools/e2e.py run --")
    binary = TARGET / "debug/arcade-look"
    assert binary.is_file(), "run python3 scripts/verify-link.py --build-e2e first"
    os.environ.pop("ALOOK_E2E_DEBUG_BINARY", None)
    os.environ.setdefault("ARCADE_E2E_SHOTS", str(LINK / ".orch/shots"))
    spec = importlib.util.spec_from_file_location("look_e2e_runner", LINK / "tools/e2e.py")
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.APPS["arcade.look"]["bin"] = str(binary)
    load_checks = runner.load_check_modules

    def load_checks_with_test_paths():
        load_checks()
        checks = runner.CHECKS["look"]
        matches = [i for i, check in enumerate(checks)
                   if check.__name__ == "look_consumer_boundaries"]
        assert len(matches) == 1, "shared Look boundary check changed; update this path adapter"
        checks[matches[0]] = look_consumer_boundaries

    runner.load_check_modules = load_checks_with_test_paths
    print(f"Look e2e binary: {binary}", flush=True)
    return runner.run_checks(Path(os.environ["ARCADE_E2E_ROOT"]), args.only)


if __name__ == "__main__":
    raise SystemExit(main())
