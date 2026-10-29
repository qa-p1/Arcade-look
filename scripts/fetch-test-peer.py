#!/usr/bin/env python3
"""Fetch the pinned real Shelf peer used by the preserved Look consumer test."""
import hashlib
from pathlib import Path
import sys
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
NAME = 'Arcade-Shelf-0.3.3-x86_64.AppImage'
SHA256 = '4915e298a6b640a9007844b989482bdcc940a8628575d8fea7dc11999237e7de'
URL = 'https://github.com/qa-p1/Arcade-Shelf/releases/download/v0.3.3/' + NAME
path = ROOT / 'artifacts/peers' / NAME
path.parent.mkdir(parents=True, exist_ok=True)
if not path.exists() or hashlib.sha256(path.read_bytes()).hexdigest() != SHA256:
    temporary = path.with_suffix('.download')
    print('Fetching verified Shelf v0.3.3 real test peer', file=sys.stderr)
    with urllib.request.urlopen(URL, timeout=120) as response, temporary.open('wb') as output:
        while chunk := response.read(1024 * 1024):
            output.write(chunk)
    if hashlib.sha256(temporary.read_bytes()).hexdigest() != SHA256:
        temporary.unlink()
        raise SystemExit('Shelf test peer checksum mismatch')
    temporary.replace(path)
path.chmod(0o755)
print(path)
