#!/usr/bin/env python3
"""Upgrade a real released Look with a user's data to this package (PLAN exit gate "Upgrade test").

    python3 <link>/tools/e2e.py run -- python3 scripts/verify-upgrade.py PREVIOUS.AppImage CANDIDATE.AppImage

Only inside Link's isolated runner (private HOME, XDG folders, D-Bus and Xvfb). Installs PREVIOUS from a download
folder, gives it the files a user of that version has (settings with non-default values, the window state with the
first-launch setup done, a plugin), runs it, then installs CANDIDATE over it, runs that and checks: every setting
value is kept (a text limit the user chose stays; only 0.4's default 4 moves to 1, LK1), the window state and the
plugin are unchanged, there is exactly one launcher and one login item (fleet rule "One launcher per app"), and an
uninstall without --remove-data keeps the data. CI runs it from the previous stable release with the candidate it built; the release gates run it on the
published files.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import re
import time

ROOT = Path(__file__).resolve().parent.parent
# A regular expression, not tomllib: CI's Linux job runs Ubuntu 22.04's Python 3.10.
VERSION = re.search(r'^version = "([^"]+)"', (ROOT / 'src-tauri/Cargo.toml').read_text(encoding='utf-8'), re.M).group(1)

SETTINGS = {
    'theme': 'dark', 'idleMinutes': 5, 'keepReady': True, 'notifications': True,
    'globalShortcut': 'Ctrl+Alt+Shift+P', 'autoplay': False, 'showHidden': True, 'textLimitMb': 8,
    'highlightLimitKb': 256, 'linkDisabledPeers': ['arcade.box'],
}
STATE = {'width': 900.0, 'height': 600.0, 'infoPanel': True, 'setupDone': True}
PLUGIN = b'{"id": "upgrade-check", "name": "Upgrade check"}\n'


def until(probe, message, seconds=30):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = probe()
        if value:
            return value
        time.sleep(.2)
    raise AssertionError(message)


def main():
    root = Path(os.environ.get('ARCADE_E2E_ROOT', '/')).resolve()
    if os.environ.get('ARCADE_E2E_INNER') != '1' or root == Path('/'):
        raise SystemExit('Run through Link tools/e2e.py run --')
    for key in ('HOME', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_RUNTIME_DIR', 'ARCADE_HOME'):
        assert Path(os.environ[key]).resolve().is_relative_to(root), key
    previous_image, candidate_image = (Path(arg).resolve(strict=True) for arg in sys.argv[1:3])

    # Real downloads outside any checkout: Link refuses to install a file below a .git tree.
    downloads = root / 'Downloads'
    downloads.mkdir(exist_ok=True)
    previous, candidate = downloads / 'previous.AppImage', downloads / 'candidate.AppImage'
    for source, target in ((previous_image, previous), (candidate_image, candidate)):
        shutil.copy2(source, target)
        assert hashlib.sha256(source.read_bytes()).digest() == hashlib.sha256(target.read_bytes()).digest()
    env = dict(os.environ, APPIMAGE_EXTRACT_AND_RUN='1', ARCADE_NO_INSTALL_PROMPT='1')
    for key in ('ARCADE_LOOK_HOME', 'APPIMAGE', 'APPDIR', 'ARCADE_DEV_BUILD', 'ARCADE_UNPACKED'):
        env.pop(key, None)
    config = Path(env['XDG_CONFIG_HOME']) / 'arcade/look'
    log = (root / 'upgrade.log').open('w')

    def command(binary, *args, timeout=60):
        result = subprocess.run([str(binary), *args], env=env, text=True, capture_output=True, timeout=timeout)
        assert result.returncode == 0, (binary.name, args, result.returncode, result.stdout, result.stderr)
        return result.stdout

    def status(binary):
        result = subprocess.run([str(binary), '--status'], env=env, text=True, capture_output=True, timeout=20)
        try:
            return json.loads(result.stdout) if result.returncode == 0 else {}
        except json.JSONDecodeError:
            return {}

    def run_resident(binary, version):
        # The launcher may hand over to Look's unpacked copy and exit 0 (U1); the resident is found by --status.
        launcher = subprocess.Popen([str(binary), '--background'], env=env, stdout=log, stderr=log)
        try:
            state = until(lambda: (s := status(binary)).get('running') and s, f'{version} did not start in the background', 60)
            assert state['runningVersion'] == version, state
            return state
        finally:
            command(binary, '--quit')
            until(lambda: not status(binary).get('running'), f'{version} did not quit')
            assert launcher.wait(timeout=30) == 0, launcher.returncode

    old_receipt = json.loads(command(previous, '--install', '--silent', '--no-launch', '--keep-download'))
    old_version = old_receipt['version']
    # The same version is allowed: before the release bump CI's build still carries the previous release's number.
    old_installed = Path(old_receipt['path'])
    assert old_installed.is_relative_to(root), old_installed

    # The files a user of the previous version has, where it keeps them (0.4 and later: arcade/look).
    (config / 'plugins/upgrade-check').mkdir(parents=True, exist_ok=True)
    (config / 'settings.json').write_text(json.dumps(SETTINGS, indent=2) + '\n', encoding='utf-8')
    (config / 'state.json').write_text(json.dumps(STATE), encoding='utf-8')
    (config / 'plugins/upgrade-check/plugin.json').write_bytes(PLUGIN)
    # Connected apps stay on: Look finds its resident through Link, so `--status` sees nothing with it off.
    run_resident(old_installed, old_version)
    before = json.loads((config / 'settings.json').read_text(encoding='utf-8'))
    for key, value in SETTINGS.items():
        assert before.get(key) == value, f'{old_version} changed setting {key}: {value!r} became {before.get(key)!r}'

    new_receipt = json.loads(command(candidate, '--install', '--silent', '--no-launch', '--keep-download'))
    assert new_receipt['version'] == VERSION, new_receipt
    installed = Path(new_receipt['path'])
    assert installed.is_relative_to(root), installed
    run_resident(installed, VERSION)

    # Settings may be written again with new fields; every value the user had stays.
    after = json.loads((config / 'settings.json').read_text(encoding='utf-8'))
    for key, value in before.items():
        assert key in after, f'setting {key} was dropped'
        assert after[key] == value, f'setting {key}: {value!r} became {after[key]!r}'
    saved_state = json.loads((config / 'state.json').read_text(encoding='utf-8'))
    for key, value in STATE.items():
        assert saved_state.get(key) == value, f'window state {key}: {value!r} became {saved_state.get(key)!r}'
    assert (config / 'plugins/upgrade-check/plugin.json').read_bytes() == PLUGIN, 'the plugin changed in the upgrade'

    # One launcher and one login item, both the new installation's (fleet rule "One launcher per app").
    for key in ('desktopEntry', 'autostart'):
        entry = Path(new_receipt['integration'][key])
        assert entry.is_file() and entry.is_relative_to(root), (key, entry)
        assert str(installed) in entry.read_text(encoding='utf-8'), (key, entry)
        old_entry = Path(old_receipt['integration'][key])
        assert old_entry == entry or not old_entry.exists(), f'the old {key} {old_entry} is still there'
        entries = [p for p in entry.parent.glob('*.desktop')
                   if 'X-Arcade-Id=arcade.look' in p.read_text(encoding='utf-8', errors='replace')
                   or 'look' in p.name.lower()]
        assert entries == [entry], f'{key}: expected only {entry.name}, found {[p.name for p in entries]}'

    assert json.loads(command(installed, '--uninstall', '--silent'))['uninstalled']
    assert not installed.exists()
    assert json.loads((config / 'settings.json').read_text(encoding='utf-8')) == after, \
        'uninstall without --remove-data changed the settings'
    assert (config / 'plugins/upgrade-check/plugin.json').read_bytes() == PLUGIN, 'uninstall removed the plugin'
    print(f'Real AppImage upgrade {old_version} -> {VERSION}: settings kept, window state and plugin '
          'unchanged, one launcher and one login item, uninstall kept the data.')


if __name__ == '__main__':
    main()
