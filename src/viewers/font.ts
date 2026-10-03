// Font specimen: editable sample text, size waterfall, variable axes, glyph grid.
import './font.css';
import { api, type FontInfo } from '../lib/backend';
import { h } from '../lib/dom';
import { fileUrl } from '../lib/urls';
import type { Mounted, ViewCtx } from './types';

let counter = 0;
const PANGRAM = 'The quick brown fox jumps over the lazy dog';
const SETS = [
  'ABCDEFGHIJKLMNOPQRSTUVWXYZ',
  'abcdefghijklmnopqrstuvwxyz',
  '0123456789 !?&@#%$€£¥ ()[]{} .,:;\'"-–—/\\*+=<>',
];

export async function mount(host: HTMLElement, ctx: ViewCtx): Promise<Mounted> {
  const { info } = ctx;
  const fi: FontInfo | null = await api.fontInfo(info.path, info.ext).catch(() => null);
  const family = `alook-font-${++counter}`;
  const face = new FontFace(family, `url("${fileUrl(info.path)}")`);
  try {
    await face.load();
  } catch {
    throw new Error("This font couldn't be loaded by the system's web engine.");
  }
  document.fonts.add(face);

  const name = fi?.fullName || fi?.family || info.name.replace(/\.[^.]+$/, '');
  const specimen = h('div.font-specimen', { style: `font-family: "${family}"` });
  const fontStyle = (el: HTMLElement) => {
    el.style.fontFamily = `"${family}", var(--sans)`;
    return el;
  };

  const sample = h('input.font-sample-input', { type: 'text', value: PANGRAM, placeholder: 'Type to preview…', spellcheck: false }) as HTMLInputElement;
  const waterfall = h('div.font-waterfall');
  const sizes = [96, 72, 56, 40, 32, 24, 18, 14, 12];
  const lines: HTMLElement[] = sizes.map((s) => {
    const t = fontStyle(h('div.wf-text', { style: `font-size:${s}px` }, PANGRAM));
    waterfall.append(h('div.wf-row', h('span.wf-size', `${s}`), t));
    return t;
  });
  sample.addEventListener('input', () => {
    for (const l of lines) l.textContent = sample.value || PANGRAM;
  });
  sample.addEventListener('keydown', (e) => e.stopPropagation());

  const axes = h('div.font-axes');
  const settings: Record<string, number> = {};
  const applyAxes = () => {
    const v = Object.entries(settings).map(([t, n]) => `"${t}" ${n}`).join(', ');
    specimen.style.fontVariationSettings = v;
    waterfall.style.fontVariationSettings = v;
    glyphs.style.fontVariationSettings = v;
  };
  for (const a of fi?.axes ?? []) {
    settings[a.tag] = a.default;
    const val = h('span.axis-val', a.default.toFixed(0));
    const range = h('input', { type: 'range', min: String(a.min), max: String(a.max), step: String((a.max - a.min) / 200 || 1), value: String(a.default) }) as HTMLInputElement;
    range.addEventListener('input', () => {
      settings[a.tag] = parseFloat(range.value);
      val.textContent = parseFloat(range.value).toFixed(0);
      applyAxes();
    });
    axes.append(h('label.axis', h('span.axis-name', `${a.name} (${a.tag})`), range, val));
  }

  const glyphs = fontStyle(h('div.glyph-grid'));
  const cps = fi?.codepoints?.length ? fi.codepoints : Array.from(SETS.join('')).map((c) => c.codePointAt(0)!);
  for (const cp of cps) {
    const ch = String.fromCodePoint(cp);
    glyphs.append(h('div.glyph', { title: `U+${cp.toString(16).toUpperCase().padStart(4, '0')}` }, h('span.glyph-char', ch), h('span.glyph-code', cp.toString(16).toUpperCase().padStart(4, '0'))));
  }

  specimen.append(h('div.spec-big', 'Aa'), h('div.spec-meta', h('div.spec-name', name), h('div.spec-sub', [fi?.subfamily, fi?.format, fi?.variable ? 'Variable' : '', fi?.monospaced ? 'Monospaced' : ''].filter(Boolean).join(' · '))));

  const view = h('div.font-view.selectable',
    specimen,
    h('div.font-sets', SETS.map((s) => fontStyle(h('div.font-set', s)))),
    h('div.font-controls', sample, (fi?.axes.length ?? 0) > 0 ? axes : null),
    waterfall,
    h('h3.font-h', `Glyphs${fi ? ` · ${fi.coverage.toLocaleString()} characters` : ''}`),
    glyphs,
  );
  host.append(h('div.prose-scroll', view));

  ctx.setStatus([fi?.family, fi?.subfamily, fi ? `${fi.glyphs.toLocaleString()} glyphs` : ''].filter(Boolean).join('  ·  '));
  if (fi) {
    ctx.setDetails([
      ['Family', fi.family ?? ''], ['Style', fi.subfamily ?? ''], ['Full name', fi.fullName ?? ''],
      ['Version', fi.version ?? ''], ['Format', fi.format], ['Glyphs', fi.glyphs.toLocaleString()],
      ['Characters', fi.coverage.toLocaleString()], ['Units per em', fi.unitsPerEm ? String(fi.unitsPerEm) : ''],
      ['Faces', fi.faces > 1 ? String(fi.faces) : ''], ['Variable axes', fi.axes.map((a) => a.tag).join(', ')],
      ['Designer', fi.designer ?? ''], ['Foundry', fi.manufacturer ?? ''], ['Copyright', fi.copyright ?? ''],
      ['License', fi.license ?? ''],
    ]);
  }
  return {
    dispose: () => {
      document.fonts.delete(face);
    },
  };
}
