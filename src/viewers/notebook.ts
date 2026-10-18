// Jupyter notebooks: Markdown cells, highlighted code, and text / image / HTML outputs.
import './code.css';
import { api } from '../lib/backend';
import { h } from '../lib/dom';
import { highlight, highlightBlocks } from '../lib/highlight';
import { sanitize, wireLinks } from '../lib/sanitize';
import type { Mounted, ViewCtx } from './types';

type Src = string | string[] | undefined;
interface Output {
  output_type: string;
  name?: string;
  /** nbformat 3: the stream's name, and outputs keyed by short format names. */
  stream?: string;
  png?: Src;
  jpeg?: Src;
  gif?: Src;
  svg?: Src;
  html?: Src;
  markdown?: Src;
  json?: Src;
  text?: Src;
  data?: Record<string, Src>;
  ename?: string;
  evalue?: string;
  traceback?: string[];
  execution_count?: number | null;
}
interface Cell {
  cell_type: string;
  source?: Src;
  input?: Src;
  outputs?: Output[];
  execution_count?: number | null;
  prompt_number?: number | null;
}

const join = (s: Src) => (Array.isArray(s) ? s.join('') : (s ?? ''));
const stripAnsi = (s: string) => s.replace(/\u001b\[[0-9;]*[A-Za-z]/g, '');

export async function mount(host: HTMLElement, ctx: ViewCtx): Promise<Mounted> {
  const { info, signal } = ctx;
  const t = await api.readText(info.path, 64 * 1024 * 1024);
  // Navigated away while loading: build nothing for a preview that is gone.
  if (ctx.signal.aborted) return {};
  const nb = JSON.parse(t.text.replace(/^﻿/, '')) as {
    cells?: Cell[];
    worksheets?: { cells: Cell[] }[];
    metadata?: { language_info?: { name?: string }; kernelspec?: { language?: string; display_name?: string } };
  };
  const cells: Cell[] = nb.cells ?? nb.worksheets?.[0]?.cells ?? [];
  if (!Array.isArray(cells)) throw new Error('Not a Jupyter notebook');
  const lang = nb.metadata?.language_info?.name ?? nb.metadata?.kernelspec?.language ?? 'python';

  const mdSources = cells.filter((c) => c.cell_type === 'markdown').map((c) => join(c.source ?? c.input));
  const mdHtml = mdSources.length ? await api.markdownBatch(mdSources) : [];
  let mdIdx = 0;

  const doc = h('div.notebook');
  const codeBlocks: [HTMLElement, string][] = [];
  for (const c of cells) {
    if (c.cell_type === 'markdown') {
      const el = h('div.nb-md.prose');
      el.append(sanitize(mdHtml[mdIdx++] ?? '', { baseDir: info.dir, allowDataImages: true }));
      doc.append(h('div.nb-cell.nb-markdown', h('div.nb-prompt'), el));
      continue;
    }
    const src = join(c.source ?? c.input);
    if (c.cell_type === 'raw') {
      doc.append(h('div.nb-cell', h('div.nb-prompt'), h('pre.nb-raw', src)));
      continue;
    }
    const n = c.execution_count ?? c.prompt_number;
    const code = h('code', src);
    codeBlocks.push([code, src]);
    const outputs = h('div.nb-outputs');
    for (const o of c.outputs ?? []) outputs.append(renderOutput(o, info.dir));
    doc.append(
      h('div.nb-cell.nb-code',
        h('div.nb-prompt', `[${n ?? ' '}]:`),
        h('div.nb-main', h('pre.nb-src', code), outputs.childElementCount ? outputs : null)),
    );
  }
  const scroll = h('div.prose-scroll', h('div.notebook-wrap.selectable', doc));
  host.append(scroll);
  wireLinks(doc, { openLocal: (p) => ctx.open(p), openExternal: (u) => void api.openUrl(u) });

  void (async () => {
    for (const [el, src] of codeBlocks) {
      if (signal.aborted) return;
      if (src.length > 100_000) continue;
      const html = await highlight(src, lang);
      if (html !== null) {
        el.innerHTML = html;
        el.classList.add('hljs');
      }
    }
    await highlightBlocks(doc, signal);
  })();

  const kernel = nb.metadata?.kernelspec?.display_name ?? lang;
  ctx.setStatus(`${cells.length} cells  ·  ${kernel}`);
  ctx.setDetails([
    ['Cells', String(cells.length)],
    ['Code cells', String(codeBlocks.length)],
    ['Markdown cells', String(mdSources.length)],
    ['Kernel', kernel],
    ['Language', lang],
  ]);
  return {};
}

/** nbformat 3 keeps each format under its own key (`text`, `png`, …); v4 under `data`. */
function outputData(o: Output): Record<string, Src> {
  if (o.data) return o.data;
  const legacy: Record<string, Src> = {
    'image/png': o.png,
    'image/jpeg': o.jpeg,
    'image/gif': o.gif,
    'image/svg+xml': o.svg,
    'text/html': o.html,
    'text/markdown': o.markdown,
    'application/json': o.json,
    'text/plain': o.text,
  };
  return Object.fromEntries(Object.entries(legacy).filter(([, v]) => v !== undefined));
}

function renderOutput(o: Output, dir: string | null): HTMLElement {
  if (o.output_type === 'stream') {
    const name = o.name ?? o.stream;
    return h(`pre.nb-stream${name === 'stderr' ? '.nb-stderr' : ''}`, stripAnsi(join(o.text)));
  }
  if (o.output_type === 'error' || o.output_type === 'pyerr') {
    return h('pre.nb-error', stripAnsi((o.traceback ?? [`${o.ename}: ${o.evalue}`]).join('\n')));
  }
  const d = outputData(o);
  for (const mime of ['image/png', 'image/jpeg', 'image/gif']) {
    if (d[mime]) {
      return h('div.nb-image', h('img', { src: `data:${mime};base64,${join(d[mime]).replace(/\s/g, '')}`, alt: '' }));
    }
  }
  if (d['image/svg+xml']) {
    return h('div.nb-image', h('img', { src: `data:image/svg+xml;charset=utf-8,${encodeURIComponent(join(d['image/svg+xml']))}`, alt: '' }));
  }
  if (d['text/html']) {
    const el = h('div.nb-html.prose');
    el.append(sanitize(join(d['text/html']), { baseDir: dir, allowDataImages: true }));
    return el;
  }
  if (d['text/markdown']) return h('pre.nb-stream', join(d['text/markdown']));
  if (d['text/plain']) return h('pre.nb-stream', stripAnsi(join(d['text/plain'])));
  if (d['application/json']) return h('pre.nb-stream', JSON.stringify(d['application/json'], null, 2));
  return h('div');
}
