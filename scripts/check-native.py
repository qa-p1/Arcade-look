#!/usr/bin/env python3
"""Real GTK/WebKit checks; run only inside Link's disposable D-Bus/Xvfb runner."""
import argparse
from contextlib import contextmanager
import ctypes
import importlib.util
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import time
import uuid

import psutil

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--lifecycle-only', action='store_true', help='Check native process/window lifecycle without the viewer/layout matrix')
parser.add_argument('--focus-only', action='store_true', help='Check preview focus retention and explicit close controls')
options = parser.parse_args()

ROOT = Path(__file__).resolve().parents[1]
LINK = Path(os.environ.get('ARCADE_LINK', ROOT.parent / 'Arcade-Link')).resolve()
SANDBOX = Path(os.environ['ARCADE_E2E_ROOT'])
assert os.environ.get('ARCADE_E2E_RUN') == '1', 'Use Link tools/e2e.py run --'
SHOTS = ROOT / 'artifacts/ui/native'
SHOTS.mkdir(parents=True, exist_ok=True)
spec = importlib.util.spec_from_file_location('look_native_runner', LINK / 'tools/e2e.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
session = runner.Session(SANDBOX, headless=True)
exe = ROOT / 'src-tauri/target/debug/arcade-look'
control = SANDBOX / 'look-native-control.sock'
log = SANDBOX / 'look-native.log'
endpoint = SANDBOX / 'arcade/run/arcade.look.endpoint'
env = dict(session.env, ALOOK_DEBUG='1', ALOOK_E2E_CONTROL=str(control), ALOOK_E2E_RELEASE_MILLIS='250')
config = SANDBOX / 'config/arcade/look/settings.json'
config.parent.mkdir(parents=True, exist_ok=True)
config.write_text(json.dumps({'theme': 'light', 'globalShortcut': '', 'autoplay': False, 'keepReady': True}))

# A real upgrade can retain the old launcher/login entry after the installer
# creates their new identifiers. The old strict migration must not stop either
# native window from opening, and conflicts must remain byte-for-byte intact.
legacy_entries = {}
for parent, args in (('data/applications', '%U'), ('config/autostart', '--service')):
    folder = SANDBOX / parent
    folder.mkdir(parents=True, exist_ok=True)
    for name, comment in (('arcade-look', 'Legacy custom entry'), ('dev.arcade.Look', 'Current installer entry')):
        destination = SANDBOX / 'arcade/.config/autostart' if parent == 'config/autostart' and name == 'dev.arcade.Look' else folder
        destination.mkdir(parents=True, exist_ok=True)
        path = destination / (name + '.desktop')
        contents = f'[Desktop Entry]\nType=Application\nName=Arcade Look\nComment={comment}\nExec="{exe}" {args}\n'
        path.write_text(contents)
        legacy_entries[path] = contents

# Real command and ES-module plugins, confined to this profile.
for name, manifest, entry in (
    ('native-command', {'name': 'Native command sample', 'extensions': ['lookcommand'], 'command': ['python3', '-c', 'print("# Command plugin\\nRendered from a real process.")'], 'output': 'markdown'}, None),
    ('native-script', {'name': 'Native script sample', 'type': 'script', 'extensions': ['lookscript'], 'entry': 'index.js'}, "export function render(host, ctx) { const p = document.createElement('p'); p.textContent = 'Script plugin: ' + ctx.file.name; host.append(p); ctx.setStatus('Script plugin ready'); return {dispose() { p.remove(); }}; }")
):
    folder = config.parent / 'plugins' / name
    folder.mkdir(parents=True, exist_ok=True)
    (folder / 'plugin.json').write_text(json.dumps(manifest))
    if entry:
        (folder / 'index.js').write_text(entry)


def wait_for(check, message, seconds=30):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = check()
        if value:
            return value
        time.sleep(.05)
    raise AssertionError(message + '\n' + (log.read_text(errors='replace')[-8000:] if log.exists() else ''))


def second(*args):
    result = subprocess.run([str(exe), *map(str, args)], env=env, text=True, capture_output=True, timeout=30)
    assert result.returncode == 0, result.stdout + result.stderr


def evaluate(script, window='main', size=None):
    token = 'NATIVE_' + uuid.uuid4().hex + ':'
    wrapped = '''(async () => { let result; try { result = {ok: true, value: await (async () => { SCRIPT })()}; }
      catch (e) { result = {ok: false, error: String(e), stack: e.stack}; }
      await window.__TAURI_INTERNALS__.invoke('log', {level: 'check', message: TOKEN + JSON.stringify(result)}); })();'''
    wrapped = wrapped.replace('SCRIPT', script).replace('TOKEN', json.dumps(token))
    with socket.socket(socket.AF_UNIX) as channel:
        channel.connect(str(control))
        channel.sendall((json.dumps({'window': window, 'script': wrapped, 'size': size}) + '\n').encode())
    line = wait_for(lambda: next((line.split(token, 1)[1] for line in log.read_text(errors='replace').splitlines() if token in line), None), 'No native evaluation response')
    result = json.loads(line)
    assert result['ok'], result
    return result.get('value')


def ready(window):
    return evaluate("return !!document.querySelector('h1') && !document.querySelector('.skeleton');", window)


@contextmanager
def unrelated_window():
    # A real external X11 window: Settings deliberately did not trigger the old
    # focus-loss hide handler, so using Settings would miss this regression.
    x11 = ctypes.CDLL('libX11.so.6')
    display_type, window_type = ctypes.c_void_p, ctypes.c_ulong
    for name, result, arguments in (
        ('XOpenDisplay', display_type, [ctypes.c_char_p]),
        ('XDefaultRootWindow', window_type, [display_type]),
        ('XCreateSimpleWindow', window_type, [display_type, window_type, ctypes.c_int, ctypes.c_int, ctypes.c_uint, ctypes.c_uint, ctypes.c_uint, ctypes.c_ulong, ctypes.c_ulong]),
        ('XMapWindow', ctypes.c_int, [display_type, window_type]),
        ('XDestroyWindow', ctypes.c_int, [display_type, window_type]),
        ('XSync', ctypes.c_int, [display_type, ctypes.c_int]),
        ('XCloseDisplay', ctypes.c_int, [display_type]),
    ):
        function = getattr(x11, name)
        function.restype, function.argtypes = result, arguments
    display = x11.XOpenDisplay(env['DISPLAY'].encode())
    assert display, 'Could not open the isolated X11 display'
    window = x11.XCreateSimpleWindow(display, x11.XDefaultRootWindow(display), 0, 0, 100, 100, 0, 0, 0)
    try:
        x11.XMapWindow(display, window)
        x11.XSync(display, False)
        yield str(window)
    finally:
        x11.XDestroyWindow(display, window)
        x11.XCloseDisplay(display)


def text_scale(factor, window='main'):
    evaluate("""const base = window.__originalType ??= Object.fromEntries(['body','control','small','caption','heading','title','title-sm','display','mono','launcher'].map(name => {
      const style = getComputedStyle(document.documentElement); return [name, [parseFloat(style.getPropertyValue('--fs-'+name)), parseFloat(style.getPropertyValue('--lh-'+name))]];
    }));
    for (const [name, [size, line]] of Object.entries(base)) {
      document.documentElement.style.setProperty('--fs-'+name, size * FACTOR + 'px');
      document.documentElement.style.setProperty('--lh-'+name, line * FACTOR + 'px');
    }""".replace('FACTOR', str(factor)), window)


def capture(name, window, size):
    # Opening a file can restore the remembered window size after it mounts;
    # apply the requested size until the window reports it.
    for _ in range(12):
        evaluate('return true;', window, size)
        time.sleep(.25)
        if evaluate('return [innerWidth, innerHeight];', window) == size:
            break
    result = evaluate('''await document.fonts.ready;
      const bad = [];
      if (document.documentElement.scrollWidth > innerWidth || document.body.scrollWidth > innerWidth) bad.push('window horizontal overflow');
      if (document.documentElement.scrollHeight > innerHeight || document.body.scrollHeight > innerHeight) bad.push('window vertical overflow');
      for (const node of document.querySelectorAll('button, .row-title, .setting-label, .setting-desc, .row-sub')) {
        if (!node.getClientRects().length) continue;
        const style = getComputedStyle(node);
        if (node.scrollWidth > node.clientWidth + 2 && !['hidden','auto','scroll'].includes(style.overflowX)) bad.push(node.className + ': ' + node.textContent);
      }
      return {bad, width: innerWidth, height: innerHeight, font: getComputedStyle(document.body).fontFamily, title: document.title,
        viewer: document.querySelector('[data-viewer]')?.dataset.viewer};''', window)
    assert not result['bad'], (name, result)
    assert 'Inter' in result['font'], result
    assert [result['width'], result['height']] == size, (name, result, size, log.read_text(errors='replace')[-4000:])
    title = 'Settings.*Arcade Look' if window == 'settings' else 'Arcade Look'
    found = subprocess.check_output(['xdotool', 'search', '--onlyvisible', '--name', title], env=env, text=True).splitlines()
    assert found, (name, 'No native window')
    window_id = found[-1]
    subprocess.run(['import', '-window', window_id, str(SHOTS / (name + '.png'))], env=env, check=True, timeout=20)
    return result


def settled_rss(seconds=30):
    # Startup work (webview warm-up, Link discovery) finishes over a few
    # seconds; compare steady states, not a sample taken mid-startup.
    deadline = time.monotonic() + seconds
    previous = rss()
    while time.monotonic() < deadline:
        time.sleep(1)
        current = rss()
        if abs(current - previous) < 1024 * 1024:
            return current
        previous = current
    return previous


def close_settings():
    # Answer first, then close: a window that is gone can't report back.
    evaluate("setTimeout(() => window.__TAURI_INTERNALS__.invoke('plugin:window|close'), 100); return true;", 'settings')


def current_pid():
    try:
        return json.loads(endpoint.read_text())['pid']
    except (FileNotFoundError, json.JSONDecodeError):
        return None


def window_state():
    with socket.socket(socket.AF_UNIX) as channel:
        channel.settimeout(3)
        channel.connect(str(control))
        channel.sendall(b'{"state":true}\n')
        with channel.makefile('rb') as response:
            return json.loads(response.readline())


def assert_resident(pid, seconds=1.25):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        assert current_pid() == pid, ('Resident process changed unexpectedly', pid, current_pid())
        assert window_state()['pid'] == pid, 'Native driver moved to another process'
        time.sleep(.05)


def memory_snapshot():
    process = psutil.Process(current_pid())
    result = {'rss': 0, 'pss': 0, 'uss': 0, 'processes': []}
    for child in [process, *process.children(recursive=True)]:
        try:
            memory = child.memory_full_info()
            entry = {'pid': child.pid, 'name': child.name(), 'rss': memory.rss,
                     'pss': getattr(memory, 'pss', 0), 'uss': getattr(memory, 'uss', 0)}
        except psutil.NoSuchProcess:
            continue
        result['processes'].append(entry)
        for key in ('rss', 'pss', 'uss'):
            result[key] += entry[key]
    return result


def released_memory():
    snapshot = memory_snapshot()
    # psutil may report the kernel-truncated name on older platforms.
    if any(child['name'].startswith('WebKitWeb') for child in snapshot['processes']):
        return None
    return snapshot


def rss():
    process = psutil.Process(current_pid())
    return sum(child.memory_info().rss for child in [process, *process.children(recursive=True)] if child.is_running())


measurements = {}
try:
    with log.open('wb') as stream:
        process = subprocess.Popen([str(exe), '--background'], env=env, stdout=stream, stderr=stream, start_new_session=True)
    wait_for(lambda: endpoint.exists() and control.exists(), 'Look never became ready with a legacy launcher conflict')
    for path, contents in legacy_entries.items():
        assert path.read_text() == contents, f'Upgrade changed conflicting integration: {path}'
    measurements['baseline'] = settled_rss()
    resident_pid = current_pid()
    measurements['initial_pid'] = resident_pid
    measurements['baseline_memory'] = memory_snapshot()
    # Destroy Settings and measure reclaimed resources without replacing Look.
    for attempt in range(2):
        second('--settings')
        wait_for(lambda: 'frontend ready' in log.read_text(errors='replace'), 'Settings never became ready')
        wait_for(lambda: ready('settings'), 'Settings did not paint')
        if attempt == 0 and not (options.lifecycle_only or options.focus_only):
            native = evaluate("return await window.__TAURI_INTERNALS__.invoke('plugin:window|inner_size');", 'settings')
            assert native == {'width': 760, 'height': 560}, native
            for theme in ('light', 'dark'):
                evaluate("await window.__TAURI_INTERNALS__.invoke('set_config', {patch: {theme: " + json.dumps(theme) + "}});", 'settings')
                for factor in (1, 1.25):
                    evaluate('''const base = window.__originalType ??= Object.fromEntries(['body','control','small','caption','heading','title','title-sm','display','mono','launcher'].map(name => {
                      const s = getComputedStyle(document.documentElement); return [name, [parseFloat(s.getPropertyValue('--fs-'+name)), parseFloat(s.getPropertyValue('--lh-'+name))]];
                    }));
                    for (const [name, [size, line]] of Object.entries(base)) {
                      document.documentElement.style.setProperty('--fs-'+name, size * FACTOR + 'px');
                      document.documentElement.style.setProperty('--lh-'+name, line * FACTOR + 'px');
                    }'''.replace('FACTOR', str(factor)), 'settings')
                    for section in ('General', 'Shortcuts', 'Previews', 'File manager', 'Plugins', 'Connected apps', 'About'):
                        evaluate('document.querySelector(' + json.dumps('button[title="'+section+'"]') + ').click();', 'settings')
                        time.sleep(.15)
                        wait_for(lambda: ready('settings'), 'Settings section did not paint')
                        capture('settings-' + section.lower().replace(' ', '-') + '-' + theme + '-' + str(factor), 'settings', [720, 520])
        close_settings()
        wait_for(lambda: 'settings' not in window_state()['windows'], 'Closing Settings retained its WebView')
        assert_resident(resident_pid, seconds=2)
        measurements['after_close_' + str(attempt)] = settled_rss()
        measurements['after_close_memory_' + str(attempt)] = wait_for(released_memory, 'Closing Settings retained a WebKit renderer', seconds=10)
        assert_resident(resident_pid)

    # Exercise actual hide timers and cold reopening beyond the former tenth-
    # release restart threshold. Only the test binary shortens the 30s timer;
    # the production release path still uses its normal delay.
    note = SANDBOX / 'resident-lifecycle.md'
    note.write_text('# Resident lifecycle\nThe same background process stays available.\n')
    second(note)
    wait_for(lambda: ready('main'), 'Preview did not paint before focus test')
    preview_window = subprocess.check_output(['xdotool', 'search', '--onlyvisible', '--pid', str(resident_pid)], env=env, text=True).splitlines()[-1]
    subprocess.run(['xdotool', 'windowfocus', '--sync', preview_window], env=env, check=True)
    wait_for(lambda: evaluate('return document.hasFocus();'), 'Preview never gained focus')
    # Cross the old 400ms grace period, then move outside and transfer focus as
    # a focus-follows-mouse compositor does. No real desktop is involved.
    assert_resident(resident_pid, seconds=.6)
    with unrelated_window() as other:
        subprocess.run(['xdotool', 'mousemove', '--window', other, '20', '20', 'windowfocus', '--sync', other], env=env, check=True)
        wait_for(lambda: not evaluate('return document.hasFocus();'), 'External window did not take focus')
        for _ in range(15):
            assert evaluate("return await window.__TAURI_INTERNALS__.invoke('plugin:window|is_visible');"), 'Preview hid when pointer/focus moved to another window'
            time.sleep(.1)
    assert_resident(resident_pid)
    for key in ('Escape', 'space'):
        subprocess.run(['xdotool', 'windowfocus', '--sync', preview_window, 'key', '--clearmodifiers', key], env=env, check=True)
        wait_for(lambda: 'main' not in window_state()['windows'], 'Explicit ' + key + ' did not close and release preview')
        second(note)
        wait_for(lambda: ready('main'), 'Preview did not reopen after ' + key)
        preview_window = subprocess.check_output(['xdotool', 'search', '--onlyvisible', '--pid', str(resident_pid)], env=env, text=True).splitlines()[-1]
    evaluate("setTimeout(() => window.__TAURI_INTERNALS__.invoke('plugin:window|close'), 100); return true;")
    wait_for(lambda: 'main' not in window_state()['windows'], 'Explicit window close did not release preview')
    assert_resident(resident_pid)
    if options.focus_only:
        print('PASS: preview survives pointer/focus loss; Escape, Space and window close still work; resident PID unchanged.')
        sys.exit(0)
    # Closing one view must leave the other view's IPC and rendering intact.
    second(note)
    wait_for(lambda: ready('main'), 'Preview did not paint before Settings')
    preview_title = evaluate('return document.title;')
    second('--settings')
    wait_for(lambda: ready('settings'), 'Concurrent Settings did not paint')
    assert set(window_state()['windows']) == {'main', 'settings'}
    close_settings()
    wait_for(lambda: 'settings' not in window_state()['windows'], 'Concurrent Settings was not destroyed')
    assert evaluate('return document.title;') == preview_title
    assert evaluate("return document.querySelector('[data-viewer]')?.dataset.viewer;") == 'markdown'
    second('--hide')
    wait_for(lambda: 'main' not in window_state()['windows'], 'Concurrent preview was not released')
    assert_resident(resident_pid)
    measurements['preview_cycle_memory'] = []
    for cycle in range(12):
        second(note)
        wait_for(lambda: ready('main'), 'Cold preview did not paint')
        second('--hide')
        wait_for(lambda: 'main' not in window_state()['windows'], 'Hidden preview WebView was not released')
        assert_resident(resident_pid)
        measurements['preview_cycle_memory'].append(wait_for(released_memory, 'Preview release retained a WebKit renderer at cycle ' + str(cycle), seconds=10))
    measurements['released_preview_cycles'] = 12
    measurements['after_preview_releases_immediate'] = memory_snapshot()
    # Give asynchronous WebKit child shutdowns a bounded settling interval;
    # PSS apportions shared mappings that aggregate RSS counts repeatedly.
    assert_resident(resident_pid, seconds=15)
    measurements['after_preview_releases'] = rss()
    measurements['after_preview_release_settle'] = memory_snapshot()
    assert released_memory(), 'Renderers remained after the final settling interval'

    # User-requested Restart must remain functional and hand off to a successor.
    second('--restart')
    wait_for(lambda: current_pid() not in (None, resident_pid), 'Explicit Restart did not replace the process')
    def successor_ready():
        try:
            return window_state()['pid'] == current_pid()
        except (OSError, json.JSONDecodeError):
            return False
    wait_for(successor_ready, 'Explicit successor did not become ready')
    measurements['explicit_restart_pid'] = current_pid()
    second('--settings')
    wait_for(lambda: ready('settings'), 'Settings did not reopen after explicit Restart')
    close_settings()
    wait_for(lambda: 'settings' not in window_state()['windows'], 'Restarted Settings WebView was not destroyed')
    assert_resident(measurements['explicit_restart_pid'], seconds=2)
    (SHOTS / 'lifecycle.json').write_text(json.dumps(measurements, indent=2) + '\n')
    if options.lifecycle_only:
        print('PASS: stable resident PID through Settings close and 12 preview releases; WebViews destroyed; explicit Restart works.')
        sys.exit(0)

    samples = SANDBOX / 'real-files'
    subprocess.run(['python3', str(ROOT / 'scripts/make-fixtures.py'), str(samples)], check=True)
    (samples / 'command.lookcommand').write_text('command plugin input')
    (samples / 'script.lookscript').write_text('script plugin input')
    long_name = 'Getting started ' + 'with a deliberately long document name ' * 4 + '.md'
    (samples / long_name).write_text((samples / 'README.md').read_text())
    fonts = list(samples.glob('*.ttf'))
    assert fonts, 'Fixture font missing'
    matrix = [(long_name,'markdown'), ('gradient.png','image'), ('photo.jpg','image'), ('scan.tiff','image'), ('layered.psd','image'),
              ('texture.tga','image'), ('animated.gif','image'), ('modern.webp','image'), ('logo.svg','image'),
              ('clip.mp4','video'), ('clip.webm','video'), ('tone.wav','audio'), ('song.mp3','audio'), ('tone.flac','audio'),
              ('report.pdf','pdf'), ('README.md','markdown'), ('main.rs','code'), ('notes.txt','code'), ('latin1.txt','code'),
              ('data.json','json'), ('analysis.ipynb','notebook'), ('people.csv','table'), ('sales.xlsx','table'),
              ('proposal.docx','document'), ('letter.odt','document'), ('memo.rtf','document'), ('novel.epub','document'),
              ('deck.pptx','slides'), ('bundle.zip','archive'), ('source.tar.gz','archive'), ('server.log.gz','archive'),
              ('assets.7z','archive'), (fonts[0].name,'font'), ('cube.gltf','model'), ('torus.stl','model'), ('pyramid.obj','model'),
              ('page.html','html'), ('command.lookcommand','plugin'), ('script.lookscript','plugin'), ('blob.bin','info'), ('empty.txt','code'), ('folder','folder')]
    for theme in ('light', 'dark'):
        second(samples / 'README.md')
        wait_for(lambda: 'mounted with markdown' in log.read_text(errors='replace'), 'Preview did not mount')
        evaluate("await window.__TAURI_INTERNALS__.invoke('set_config', {patch: {theme: " + json.dumps(theme) + "}});")
        for name, viewer in matrix:
            path = samples / name
            assert path.exists(), 'Fixture generator omitted ' + name
            offset = log.stat().st_size
            second(path)
            wait_for(lambda: f'mounted with {viewer}' in log.read_bytes()[offset:].decode(errors='replace'), 'Wrong viewer for ' + name)
            for size in ([720,520],[1920,1080]):
                result = capture(name.replace('.','-')[:160] + '-' + theme + '-' + str(size[0]), 'main', size)
                assert result['viewer'] == viewer, (name, result)
            text_scale(1.25)
            capture(name.replace('.','-')[:160] + '-' + theme + '-720-125', 'main', [720,520])
            text_scale(1)
        offset = log.stat().st_size
        second(ROOT / 'docs/user/getting-started.md')
        wait_for(lambda: 'mounted with markdown' in log.read_bytes()[offset:].decode(errors='replace'), 'User guide did not mount')
        capture('readme-preview-' + theme, 'main', [1100,760])
        # Settings must leave the active preview intact, and live theme changes reach both windows.
        original = evaluate('return document.title;')
        second('--settings')
        wait_for(lambda: ready('settings'), 'Second Settings window did not paint')
        assert evaluate('return document.title;') == original
        evaluate("await window.__TAURI_INTERNALS__.invoke('set_config', {patch: {theme: 'system'}});", 'settings')
        assert evaluate('return !document.documentElement.dataset.theme;')
        close_settings()
    (SHOTS / 'measurements.json').write_text(json.dumps(measurements, indent=2) + '\n')
    (SHOTS / 'matrix.json').write_text(json.dumps(matrix, indent=2) + '\n')
    print('PASS: native resident PID/WebView lifecycle, explicit Restart, live themes, all viewer types, minimum/full-size layout; screenshots saved.')
finally:
    try:
        second('--quit')
    except Exception:
        pass
    for entry in SANDBOX.rglob('doc'):
        subprocess.run(['fusermount3', '-u', str(entry)], env=env, capture_output=True)
    session.close()
