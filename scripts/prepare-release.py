#!/usr/bin/env python3
"""Stage compatible installers, manifest and checksums. Publishing is a separate manual action."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('built', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--channel', choices=['stable','beta','nightly'], required=True)
    parser.add_argument('--notes', required=True)
    args = parser.parse_args()
    version = tomllib.loads((ROOT/'src-tauri/Cargo.toml').read_text(encoding='utf-8'))['package']['version']
    args.output.mkdir(parents=True, exist_ok=False)
    for path in sorted(args.built.iterdir()):
        if not path.is_file(): continue
        name = path.name.replace(' ', '-')
        shutil.copyfile(path, args.output/name)
    subprocess.run([sys.executable, str(ROOT/'scripts/arcade-release.py'), '--id','arcade.look','--version',version,
                    '--channel',args.channel,'--notes',args.notes,str(args.output)], check=True)
    manifest = json.loads((args.output/'arcade-release.json').read_text(encoding='utf-8'))
    assets = manifest['assets']
    assert len(assets)==3, f'Expected all 3 platform installers, found {len(assets)}'
    assert {(a['os'],a['kind']) for a in assets} == {('linux','appimage'),('windows','nsis'),('macos','dmg')}
    assert next(a for a in assets if a['os']=='macos')['arch']=='universal'
    expected = {}
    for line in (args.output/'SHA256SUMS.txt').read_text(encoding='utf-8').splitlines():
        digest,name=line.split(None,1); expected[name.lstrip('*')]=digest
    for asset in assets:
        path=args.output/asset['file']; digest=hashlib.file_digest(path.open('rb'),'sha256').hexdigest()
        assert digest==asset['sha256']==expected[path.name], path.name
        assert path.stat().st_size==asset['size'], path.name
    print(f'Verified {len(assets)} {args.channel} installers and release metadata; nothing published.')

if __name__=='__main__': main()
