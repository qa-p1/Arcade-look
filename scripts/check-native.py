#!/usr/bin/env python3
"""Real GTK/WebKit checks; run only inside Link's disposable D-Bus/Xvfb runner."""
import importlib.util
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import time
import uuid

import psutil

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
env = dict(session.env, ALOOK_DEBUG='1', ALOOK_E2E_CONTROL=str(control))
config = SANDBOX / 'config/arcade/look/settings.json'
config.parent.mkdir(parents=True, exist_ok=True)
config.write_text(json.dumps({'theme': 'light', 'globalShortcut': '', 'autoplay': False, 'keepReady': True}))

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


def capture(name, window, size):
    evaluate('return true;', window, size)
    time.sleep(.25)
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
    assert [result['width'], result['height']] == size, (name, result, size)
    title = 'Settings.*Arcade Look' if window == 'settings' else 'Arcade Look'
    found = subprocess.check_output(['xdotool', 'search', '--onlyvisible', '--name', title], env=env, text=True).splitlines()
    assert found, (name, 'No native window')
    window_id = found[-1]
    subprocess.run(['import', '-window', window_id, str(SHOTS / (name + '.png'))], env=env, check=True, timeout=20)
    return result


def current_pid():
    try:
        return json.loads(endpoint.read_text())['pid']
    except (FileNotFoundError, json.JSONDecodeError):
        return None


def rss():
    process = psutil.Process(current_pid())
    return sum(child.memory_info().rss for child in [process, *process.children(recursive=True)] if child.is_running())


measurements = {}
try:
    with log.open('wb') as stream:
        process = subprocess.Popen([str(exe), '--background'], env=env, stdout=stream, stderr=stream, start_new_session=True)
    wait_for(lambda: endpoint.exists() and control.exists(), 'Look never became ready')
    time.sleep(1)
    measurements['baseline'] = rss()
    # Repeated create/destroy must return the entire process tree to its idle RSS.
    for attempt in range(2):
        old_pid = current_pid()
        second('--settings')
        wait_for(lambda: 'frontend ready' in log.read_text(errors='replace'), 'Settings never became ready')
        wait_for(lambda: ready('settings'), 'Settings did not paint')
        if attempt == 0:
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
        evaluate("await window.__TAURI_INTERNALS__.invoke('plugin:window|close');", 'settings')
        wait_for(lambda: current_pid() not in (None, old_pid), 'Closing Settings did not return to a fresh idle service', seconds=40)
        wait_for(control.exists, 'Restarted service did not expose its private test driver')
        time.sleep(1)
        measurements['after_close_' + str(attempt)] = rss()
        assert abs(measurements['after_close_' + str(attempt)] - measurements['baseline']) <= 5 * 1024 * 1024, measurements
    samples = SANDBOX / 'real-files'
    subprocess.run(['python3', str(ROOT / 'scripts/make-fixtures.py'), str(samples)], check=True)
    (samples / 'command.lookcommand').write_text('command plugin input')
    (samples / 'script.lookscript').write_text('script plugin input')
    fonts = list(samples.glob('*.ttf'))
    assert fonts, 'Fixture font missing'
    matrix = [('gradient.png','image'), ('photo.jpg','image'), ('scan.tiff','image'), ('layered.psd','image'),
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
            wait_for(lambda: f'mounted with {viewer}' in log.read_text(errors='replace')[offset:], 'Wrong viewer for ' + name)
            for size in ([720,520],[1920,1080]):
                result = capture(name.replace('.','-') + '-' + theme + '-' + str(size[0]), 'main', size)
                assert result['viewer'] == viewer, (name, result)
        offset = log.stat().st_size
        second(ROOT / 'docs/user/getting-started.md')
        wait_for(lambda: 'mounted with markdown' in log.read_text(errors='replace')[offset:], 'User guide did not mount')
        capture('readme-preview-' + theme, 'main', [1100,760])
        # Settings must leave the active preview intact, and live theme changes reach both windows.
        original = evaluate('return document.title;')
        second('--settings')
        wait_for(lambda: ready('settings'), 'Second Settings window did not paint')
        assert evaluate('return document.title;') == original
        evaluate("await window.__TAURI_INTERNALS__.invoke('set_config', {patch: {theme: 'system'}});", 'settings')
        assert evaluate('return !document.documentElement.dataset.theme;')
        evaluate("await window.__TAURI_INTERNALS__.invoke('plugin:window|close');", 'settings')
    (SHOTS / 'measurements.json').write_text(json.dumps(measurements, indent=2) + '\n')
    (SHOTS / 'matrix.json').write_text(json.dumps(matrix, indent=2) + '\n')
    print('PASS: native Settings lifecycle/RSS, live themes, all viewer types, minimum/full-size layout; screenshots saved.')
finally:
    try:
        second('--quit')
    except Exception:
        pass
    for entry in SANDBOX.rglob('doc'):
        subprocess.run(['fusermount3', '-u', str(entry)], env=env, capture_output=True)
    session.close()
