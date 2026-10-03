// The universal fallback: a file card plus a virtualised hex view that reads on demand,
// so even a 100 GB disk image previews instantly.
import './info.css';
import { kindLabel } from '../lib/kinds';
import { api } from '../lib/backend';
import { h } from '../lib/dom';
import * as fmt from '../lib/format';
import { icon, kindBadge } from '../lib/icons';
import { virtualList, type Virtual } from '../lib/virtual';
import type { Mounted, ViewCtx } from './types';

const ROW = 20;
const CHUNK = 64 * 1024;
const MAX_CHUNKS = 64;

export function mount(host: HTMLElement, ctx: ViewCtx): Mounted {
  const { info } = ctx;
  const exists = !!info.dir || info.size > 0 || info.modified !== null;
  const card = h('div.info-card',
    kindBadge(info.kind, info.ext, 34),
    h('div.info-card-text',
      h('div.info-card-name.selectable', info.name),
      h('div.info-card-meta', exists ? `${kindLabel(info)}  ·  ${fmt.bytes(info.size, true)}  ·  Modified ${fmt.date(info.modified)}` : info.path)),
    exists ? h('div.info-card-actions',
      h('button.btn', { onclick: () => void api.openDefault(info.path).catch((e) => ctx.toast(String(e))) }, icon('open', 15), 'Open'),
      h('button.btn', { onclick: () => void api.reveal(info.path).catch((e) => ctx.toast(String(e))) }, icon('reveal', 15), 'Show in folder')) : null);

  const err = ctx.previousError
    ? h('div.info-error', icon('alert', 16), h('span', ctx.info.kind === 'binary' || !exists ? ctx.previousError : `Couldn't render a rich preview: ${ctx.previousError}`))
    : null;
  host.append(h('div.info-top', card, err));

  if (!exists || info.kind === 'folder' || info.size === 0) {
    if (info.size === 0 && exists) host.append(h('div.center-fill', 'This file is empty.'));
    ctx.setStatus('');
    return {};
  }

  // ---------------------------------------------------------------- hex view
  const rows = Math.ceil(info.size / 16);
  const offsetDigits = Math.max(8, info.size.toString(16).length);
  const cache = new Map<number, Uint8Array>();
  const pending = new Set<number>();
  let virt: Virtual | null = null;
  let disposed = false;

  const load = (chunk: number) => {
    if (cache.has(chunk) || pending.has(chunk)) return;
    pending.add(chunk);
    api.readBytes(info.path, chunk * CHUNK, CHUNK)
      .then((buf) => {
        pending.delete(chunk);
        if (disposed) return;
        cache.set(chunk, new Uint8Array(buf));
        if (cache.size > MAX_CHUNKS) cache.delete(cache.keys().next().value as number);
        virt?.refresh();
      })
      .catch(() => pending.delete(chunk));
  };

  const hex = (b: number) => (b < 16 ? '0' : '') + b.toString(16);
  const printable = (b: number) => (b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : '·');

  const scroller = h('div.hex-scroll.selectable');
  host.append(h('div.hex-head', h('span.hex-off', 'Offset'), h('span.hex-bytes', '00 01 02 03 04 05 06 07  08 09 0A 0B 0C 0D 0E 0F'), h('span.hex-ascii', 'ASCII')), scroller);

  virt = virtualList({
    scroller,
    rowHeight: ROW,
    count: rows,
    overscan: 20,
    render: (r) => {
      const off = r * 16;
      const chunk = Math.floor(off / CHUNK);
      const data = cache.get(chunk);
      const row = h('div.hex-row');
      const offEl = h('span.hex-off', off.toString(16).padStart(offsetDigits, '0'));
      if (!data) {
        load(chunk);
        row.append(offEl, h('span.hex-bytes.loading', '·· '.repeat(16).trim()), h('span.hex-ascii'));
        return row;
      }
      const start = off - chunk * CHUNK;
      const end = Math.min(start + 16, data.length);
      let hx = '';
      let asc = '';
      for (let i = start; i < start + 16; i++) {
        if (i < end) {
          hx += hex(data[i]);
          asc += printable(data[i]);
        } else {
          hx += '  ';
        }
        hx += i - start === 7 ? '  ' : ' ';
      }
      row.append(offEl, h('span.hex-bytes', hx.trimEnd()), h('span.hex-ascii', asc));
      return row;
    },
  });

  ctx.setStatus(`${rows.toLocaleString()} rows of 16 bytes`);
  return {
    keydown(e) {
      if (e.key === 'Home') { virt?.scrollToRow(0); return true; }
      if (e.key === 'End') { virt?.scrollToRow(rows); return true; }
      return false;
    },
    dispose() {
      disposed = true;
      virt?.destroy();
    },
  };
}
