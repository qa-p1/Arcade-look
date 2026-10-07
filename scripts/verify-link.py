#!/usr/bin/env python3
"""Run checks and optionally build native e2e Look without changing the login binary."""

import argparse
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build-e2e", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    target = root / "src-tauri/target/link-tests"
    target.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, CARGO_BUILD_JOBS="3", CARGO_TARGET_DIR=str(target))

    def run(command, cwd=root):
        print(f"{cwd}: {' '.join(command)}", flush=True)
        subprocess.run(command, cwd=cwd, env=env, check=True)

    with tempfile.TemporaryDirectory(prefix="registry-", dir=target) as registry:
        env["ARCADE_HOME"] = registry
        run(["npm", "run", "typecheck"])
        run(["npm", "run", "build"])
        for command in (
            ["cargo", "fmt", "--check"],
            ["cargo", "clippy", "--all-targets", "--", "-D", "warnings"],
            ["cargo", "test"],
        ):
            run(command, root / "src-tauri")
        if args.build_e2e:
            run(["npx", "tauri", "build", "--debug", "--no-bundle", "--features", "e2e"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
