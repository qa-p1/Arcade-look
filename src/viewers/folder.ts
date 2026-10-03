// Folder overview: item count, recursive size (time-budgeted), and a browsable grid.
import './folder.css';
import { api, type DirEntry } from '../lib/backend';
import { h } from '../lib/dom';
import * as fmt from '../lib/format';
import { icon, kindBadge } from '../lib/icons';
import { fileUrl } from '../lib/urls';
import type { Mounted, ViewCtx } from './types';

const THUMB_EXT = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'avif', 'ico']);
const THUMB_MAX = 3 * 1024 * 1024;

export async function mount(host: HTMLElement, ctx: ViewCtx): Promise<Mounted> {
  const { info } = ctx;
  const listing = await api.listDir(info.path);
  const showHidden = ctx.boot.config.showHidden;
  const entries = listing.entries.filter((e) => showHidden || !e.hidden);
  const hiddenCount = listing.entries.length - entries.length;
  const folders = entries.filter((e) => e.dir).length;
  const files = entries.length - folders;

  const sizeEl = h('span.folder-size', 'Calculating size…');
  const header = h('div.folder-head',
    h('div.folder-icon', icon('folder', 46)),
    h('div',
      h('div.folder-name', info.name),
      h('div.folder-stats',
        h('span', [fmt.count(folders, 'folder'), fmt.count(files, 'file')].join(' · ')),
        hiddenCount ? h('span.muted', ` · ${hiddenCount} hidden`) : null,
        h('span', ' · '),
        sizeEl)));

  const grid = h('div.folder-grid');
  let thumbs = 0;
  const tile = (e: DirEntry) => {
    const t = h(`button.ftile${e.hidden ? '.dim' : ''}`, { title: e.name });
    t.tabIndex = -1;
    const ext = e.ext;
    let visual: HTMLElement = kindBadge(e.kind, ext, 26);
    if (!e.dir && THUMB_EXT.has(ext) && (e.size ?? 0) < THUMB_MAX && thumbs < 300) {
      thumbs++;
      const img = h('img.fthumb', { loading: 'lazy', decoding: 'async', alt: '' }) as HTMLImageElement;
      img.src = fileUrl(e.path);
      img.onerror = () => img.replaceWith(kindBadge(e.kind, ext, 26));
      visual = h('div.fthumb-wrap', img);
    }
    t.append(visual, h('span.ftile-name', e.name), h('span.ftile-meta', e.dir ? '' : fmt.bytes(e.size)));
    t.addEventListener('click', () => ctx.open(e.path));
    return t;
  };
  for (const e of entries) grid.append(tile(e));
  if (!entries.length) grid.append(h('div.folder-empty', 'This folder is empty.'));
  if (listing.truncated) grid.append(h('div.folder-empty', `…and ${(listing.total - listing.entries.length).toLocaleString()} more items`));

  host.append(h('div.prose-scroll', h('div.folder-view', header, grid)));
  ctx.setStatus(fmt.count(listing.total, 'item'));

  void api.dirSize(info.path).then((s) => {
    if (ctx.signal.aborted) return;
    const text = `${s.complete ? '' : '≥ '}${fmt.bytes(s.bytes)}`;
    sizeEl.textContent = text;
    ctx.setStatus(`${fmt.count(listing.total, 'item')}  ·  ${text}`);
    ctx.setDetails([
      ['Items', listing.total.toLocaleString()],
      ['Total size', `${s.complete ? '' : 'at least '}${fmt.bytes(s.bytes, true)}`],
      ['Files (all levels)', `${s.complete ? '' : '≥ '}${s.files.toLocaleString()}`],
      ['Folders (all levels)', `${s.complete ? '' : '≥ '}${s.dirs.toLocaleString()}`],
    ]);
  }).catch(() => (sizeEl.textContent = 'size unavailable'));

  return {};
}
