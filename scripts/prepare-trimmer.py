#!/usr/bin/env python3
"""Build libarcade-trim.so (OPEN1, D12) into src-tauri/binaries/ for the AppImage; other platforms need nothing.

tauri.conf.json puts it in the AppImage's usr/lib, and the resident preloads it into WebKit's helper processes
(src-tauri/src/memory.rs). Run it before `tauri build` for an AppImage (scripts/package and build.yml do)."""
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent


def main():
    if not sys.platform.startswith('linux'):
        print('No WebKit trimmer needed on this platform')
        return
    destination = ROOT / 'src-tauri/binaries'
    destination.mkdir(exist_ok=True)
    trimmer = destination / 'libarcade-trim.so'
    if trimmer.is_symlink():
        raise SystemExit('The generated trimmer destination is a symlink')
    linux = ROOT / 'packaging/linux'
    subprocess.run(['cc', '-O2', '-shared', '-fPIC', '-Wall', '-Werror', '-o', str(trimmer),
                    str(linux / 'arcade-trim-preload.c'), f'-I{linux}', '-lpthread'], check=True)
    print(f'Staged {trimmer.relative_to(ROOT)}')


if __name__ == '__main__':
    main()
