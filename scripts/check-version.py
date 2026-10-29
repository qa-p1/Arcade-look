#!/usr/bin/env python3
"""Cargo.toml is Look's version source. Other versions are checked generated metadata."""
import argparse
import json
from pathlib import Path
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
CARGO = ROOT / 'src-tauri/Cargo.toml'

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--print', action='store_true')
    parser.add_argument('--set', metavar='VERSION')
    args = parser.parse_args()
    text = CARGO.read_text(encoding='utf-8')
    previous = tomllib.loads(text)['package']['version']
    if args.set:
        if not re.fullmatch(r'\d+\.\d+\.\d+(?:-[\w.-]+)?', args.set): parser.error('Expected a semantic version')
        text = text.replace(f'version = "{previous}"', f'version = "{args.set}"', 1)
        CARGO.write_text(text, encoding='utf-8')
        lock = ROOT / 'src-tauri/Cargo.lock'
        lock.write_text(lock.read_text(encoding='utf-8').replace(f'name = "arcade-look"\nversion = "{previous}"', f'name = "arcade-look"\nversion = "{args.set}"', 1), encoding='utf-8')
        path = ROOT / 'docs/user/shortcuts.json'
        doc = json.loads(path.read_text(encoding='utf-8')); doc['version'] = args.set
        path.write_text(json.dumps(doc, indent=2, ensure_ascii=False)+'\n', encoding='utf-8')
    version = tomllib.loads(text)['package']['version']
    errors = []
    for name in ['package.json', 'src-tauri/tauri.conf.json']:
        if 'version' in json.loads((ROOT/name).read_text(encoding='utf-8')): errors.append(f'{name}: remove the duplicate version')
    lock = json.loads((ROOT/'package-lock.json').read_text(encoding='utf-8'))
    if 'version' in lock or 'version' in lock.get('packages', {}).get('', {}): errors.append('package-lock.json has a duplicate root version')
    if json.loads((ROOT/'docs/user/shortcuts.json').read_text(encoding='utf-8'))['version'] != version: errors.append('Regenerate the shortcut sheet for this version')
    cargo_lock = tomllib.loads((ROOT/'src-tauri/Cargo.lock').read_text(encoding='utf-8'))
    if next(p['version'] for p in cargo_lock['package'] if p['name']=='arcade-look') != version: errors.append('Cargo.lock has a stale app version')
    if errors: sys.exit('\n'.join(errors))
    print(version if args.print else f'Version {version}; Cargo.toml is the source.')

if __name__ == '__main__': main()
