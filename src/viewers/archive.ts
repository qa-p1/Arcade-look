// Archive browser: a collapsible tree on the left, a live preview of the selected entry
// (extracted on demand into a temp cache) on the right — any viewer works nested.
import './archive.css';
import { api, errorMessage, type ArchiveEntry } from '../lib/backend';
import { h } from '../lib/dom';
import * as fmt from '../lib/format';
import { icon, kindBadge } from '../lib/icons';
import type { Mounted, ViewCtx } from './types';

interface Node {
  name: string;
  path: string;
  dir: boolean;
  entry?: ArchiveEntry;
  children: Map<string, Node>;
  size: number;
}

const EXT_KIND: Record<string, string> = {};
for (const [k, exts] of Object.entries({
  image: 'png jpg jpeg gif webp bmp svg ico tif tiff psd heic avif',
  video: 'mp4 mov mkv webm avi m4v',
  audio: 'mp3 flac wav ogg m4a opus aac',
  pdf: 'pdf',
  code: 'js ts rs py go c h cpp java kt rb php sh json yaml yml toml xml html css',
  doc: 'md markdown txt rtf docx odt epub log',
  table: 'csv tsv xlsx xls ods',
  archive: 'zip tar gz 7z bz2 xz zst',
  font: 'ttf otf woff woff2',
  model: 'glb gltf obj stl fbx',
})) for (const e of exts.split(' ')) EXT_KIND[e] = k;

function extOf(name: string) {
  const i = name.lastIndexOf('.');
  return i > 0 ? name.slice(i + 1).toLowerCase() : '';
}

function buildTree(entries: ArchiveEntry[]): Node {
  const root: Node = { name: '', path: '', dir: true, children: new Map(), size: 0 };
  for (const e of entries) {
    const clean = e.path.replace(/\\/g, '/').replace(/^(\.\/|\/)+/, '').replace(/\/+$/, '');
    if (!clean) continue;
    const parts = clean.split('/');
    let node = root;
    let acc = '';
    parts.forEach((p, i) => {
      acc = acc ? `${acc}/${p}` : p;
      const last = i === parts.length - 1;
      let child = node.children.get(p);
      if (!child) {
        child = { name: p, path: acc, dir: !last || e.dir, children: new Map(), size: 0 };
        node.children.set(p, child);
      }
      if (last) {
        child.entry = e;
        child.dir = e.dir;
      }
      node = child;
    });
  }
  const sum = (n: Node): number => {
    if (!n.dir) return (n.size = n.entry?.size ?? 0);
    let s = 0;
    for (const c of n.children.values()) s += sum(c);
    return (n.size = s);
  };
  sum(root);
  return root;
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' });
const sorted = (n: Node) => [...n.children.values()].sort((a, b) => Number(b.dir) - Number(a.dir) || collator.compare(a.name, b.name));

export async function mount(host: HTMLElement, ctx: ViewCtx): Promise<Mounted> {
  const { info } = ctx;
  const listing = await api.listArchive(info.path, info.format);
  const root = buildTree(listing.entries);
  const files = listing.entries.filter((e) => !e.dir);
  const countDirs = (n: Node): number => [...n.children.values()].reduce((a, c) => a + (c.dir ? 1 + countDirs(c) : 0), 0);
  const dirs = countDirs(root);

  const list = h('div.arc-list');
  const preview = h('div.arc-preview');
  host.append(h('div.arc-split', list, preview));

  // ---------------------------------------------------------------- summary
  const ratio = listing.totalSize > 0 && listing.packedSize > 0 ? Math.max(0, 1 - listing.packedSize / listing.totalSize) : 0;
  const summary = () =>
    h('div.arc-summary',
      kindBadge('archive', '', 34),
      h('div.arc-sum-title', info.name),
      h('div.arc-sum-stats',
        stat(fmt.count(files.length, 'file'), dirs ? fmt.count(dirs, 'folder') : 'no folders'),
        stat(fmt.bytes(listing.totalSize), 'uncompressed'),
        listing.packedSize ? stat(fmt.bytes(listing.packedSize), `compressed${ratio > 0.01 ? ` · ${Math.round(ratio * 100)}% saved` : ''}`) : null),
      listing.truncated ? h('div.arc-note', icon('alert', 14), h('span', listing.note ?? 'Only the first 50,000 entries are listed.')) : null,
      listing.note && !listing.truncated ? h('div.arc-note', icon('alert', 14), h('span', listing.note)) : null,
      h('div.arc-hint', 'Select a file to preview it here.'));
  preview.append(summary());

  // ---------------------------------------------------------------- tree
  let selected: HTMLElement | null = null;
  let nested: Mounted | null = null;
  let token = 0;

  function row(n: Node, depth: number): HTMLElement {
    const ext = extOf(n.name);
    const r = h('div.arc-row', { style: `--depth:${depth}`, title: n.path },
      h('span.arc-caret', n.dir ? '▸' : ''),
      kindBadge(n.dir ? 'folder' : EXT_KIND[ext] ?? 'file', ext, 14),
      h('span.arc-name', n.name),
      n.entry?.encrypted ? icon('lock', 13) : null,
      h('span.arc-size', n.dir ? (n.size ? fmt.bytes(n.size) : '') : fmt.bytes(n.entry?.size ?? null)),
      h('span.arc-date', fmt.shortDate(n.entry?.modified)));
    const wrap = h('div.arc-node', r);
    let open = false;
    let kids: HTMLElement | null = null;
    r.addEventListener('click', () => {
      if (n.dir) {
        open = !open;
        r.classList.toggle('open', open);
        if (open && !kids) {
          kids = h('div.arc-kids');
          appendChildren(kids, n, depth + 1);
          wrap.append(kids);
        }
        kids?.classList.toggle('hidden', !open);
      } else {
        void select(n, r);
      }
    });
    r.addEventListener('dblclick', async () => {
      if (n.dir) return;
      try {
        await api.openDefault(await api.extractEntry(info.path, info.format, n.entry?.path ?? n.path));
      } catch (e) {
        ctx.toast(errorMessage(e));
      }
    });
    return wrap;
  }

  function appendChildren(container: HTMLElement, n: Node, depth: number, from = 0) {
    const kids = sorted(n);
    const end = Math.min(kids.length, from + 1000);
    for (let i = from; i < end; i++) container.append(row(kids[i], depth));
    if (end < kids.length) {
      const more = h('button.arc-more', `Show ${Math.min(1000, kids.length - end)} more of ${kids.length - end}`);
      more.addEventListener('click', () => {
        more.remove();
        appendChildren(container, n, depth, end);
      });
      container.append(more);
    }
  }

  async function select(n: Node, r: HTMLElement) {
    selected?.classList.remove('selected');
    selected = r;
    r.classList.add('selected');
    const my = ++token;
    nested?.dispose?.();
    nested = null;
    preview.replaceChildren(h('div.center-fill', h('div.spinner'), h('span', `Extracting ${n.name}…`)));
    try {
      const tmp = await api.extractEntry(info.path, info.format, n.entry?.path ?? n.path);
      const childInfo = await api.inspect(tmp);
      childInfo.name = n.name;
      if (my !== token || ctx.signal.aborted) return;
      const sub = h('div.sub-host.arc-sub');
      preview.replaceChildren(
        h('div.arc-sub-head', kindBadge(childInfo.kind, childInfo.ext, 14), h('span.arc-sub-name', n.path), h('span.arc-sub-size', fmt.bytes(childInfo.size))),
        sub);
      nested = await ctx.mountNested(sub, childInfo);
      if (my !== token) nested.dispose?.();
    } catch (e) {
      if (my !== token) return;
      preview.replaceChildren(h('div.center-fill', icon('alert', 28), h('span', errorMessage(e))));
    }
  }

  appendChildren(list, root, 0);
  // A lone file (e.g. log.txt.gz) previews straight away.
  if (files.length === 1 && root.children.size === 1) {
    const only = list.querySelector<HTMLElement>('.arc-row');
    const n = [...root.children.values()][0];
    if (only && !n.dir) void select(n, only);
  }

  ctx.setStatus(`${fmt.count(files.length, 'file')}  ·  ${fmt.bytes(listing.totalSize)}`);
  ctx.setDetails([
    ['Format', listing.format.toUpperCase()],
    ['Files', files.length.toLocaleString()],
    ['Folders', dirs.toLocaleString()],
    ['Uncompressed', fmt.bytes(listing.totalSize, true)],
    ['Compressed', listing.packedSize ? fmt.bytes(listing.packedSize, true) : ''],
    ['Space saved', ratio > 0.01 ? `${Math.round(ratio * 100)}%` : ''],
    ['Encrypted', listing.entries.some((e) => e.encrypted) ? 'Some entries' : ''],
  ]);

  return {
    keydown: (e) => nested?.keydown?.(e) ?? false,
    dispose() {
      token++;
      nested?.dispose?.();
    },
  };
}

function stat(big: string, small: string) {
  return h('div.arc-stat', h('div.arc-stat-big', big), h('div.arc-stat-small', small));
}
