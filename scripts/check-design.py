#!/usr/bin/env python3
"""Check app-owned type and colour use, including syntax palette contrast."""
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
errors = []
for path in (ROOT / 'src').rglob('*'):
    if not path.is_file() or path.suffix not in ('.css', '.ts') or path.is_relative_to(ROOT / 'src/design'):
        continue
    text = path.read_text()
    name = path.relative_to(ROOT).as_posix()
    colours = re.findall(r'#[0-9a-fA-F]{3,8}\b|\b(?:rgba?|hsla?)\(', text)
    if colours and name not in ('src/styles/app.css', 'src/lib/icons.ts'):
        errors.append(f'{name}: colour literal outside the documented palette')
    if name == 'src/styles/app.css':
        for line in text.splitlines():
            if re.search(r'#[0-9a-fA-F]{3,8}\b', line) and not re.search(r'--(?:hl-[\w-]+|page-white|video-black):', line):
                errors.append(f'{name}: undocumented colour: {line.strip()}')
    if path.suffix == '.css':
        for value in re.findall(r'(?<![-\w])font-size:\s*([^;}]+)', text):
            if not (value.startswith('var(--fs-') or value == 'inherit' or name == 'src/viewers/pdf.css' and value == 'calc(var(--text-scale-factor) * var(--font-height))' or name == 'src/viewers/font.css' and value in ('140px','28px','30px')):
                errors.append(f'{name}: off-scale font-size: {value}')
        if re.search(r'font:\s*[\d.]+(?:px|em|rem)', text):
            errors.append(f'{name}: font shorthand must use the kit scale')


def luminance(value):
    channels = [int(value[i:i+2], 16) / 255 for i in (1,3,5)]
    linear = [c / 12.92 if c <= .04045 else ((c+.055)/1.055)**2.4 for c in channels]
    return sum(c*w for c,w in zip(linear, (.2126,.7152,.0722)))


def contrast(a,b):
    x,y = sorted((luminance(a),luminance(b)))
    return (y+.05)/(x+.05)


css = (ROOT / 'src/styles/app.css').read_text().split("/* Look's content palette: syntax",1)[1]
light,dark = css.split('@media (prefers-color-scheme: dark)',1)
kit = (ROOT / 'src/design/arcade.css').read_text()
backgrounds = re.findall(r'--sunken:\s*(#[0-9a-f]{6})',kit)
for theme, block, background in [('light',light,backgrounds[0]),('dark',dark,backgrounds[1])]:
    for name,colour in re.findall(r'--hl-([\w-]+):\s*(#[0-9a-f]{6})',block):
        if contrast(colour,background)<4.5:
            errors.append(f'{theme} syntax {name}: {contrast(colour,background):.2f}:1 on sunken, needs 4.5:1')
if errors:
    raise SystemExit('\n'.join(errors))
print('App colour/type inventory and syntax contrast passed in both themes.')
