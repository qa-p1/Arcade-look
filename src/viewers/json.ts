import { formatCount } from '../design/format';
// Collapsible JSON tree with lazy expansion (children render only when opened).
import './code.css';
import { api } from '../lib/backend';
import { h } from '../lib/dom';
import { renderCode } from './code';
import type { Mounted, ViewCtx } from './types';

const CHUNK = 250;
const TREE_LIMIT = 24 * 1024 * 1024;

type J = null | boolean | number | string | J[] | { [k: string]: J };

/** Marks a number literal that a JavaScript number can't hold exactly. */
const EXACT = '\uE000';
const NUMBER = /-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/y;

/**
 * JSON.parse, except that number literals a double would change (long IDs
 * such as 9007199254740993, or more than 17 significant digits) keep their
 * original text, so the tree shows what the file says.
 */
export function parseExact(text: string): J {
  if (!/\d{16}/.test(text)) return JSON.parse(text) as J;
  let out = '';
  let last = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    if (c === 34 /* " */) {
      for (i++; i < text.length && text.charCodeAt(i) !== 34; i++)
        if (text.charCodeAt(i) === 92) i++;
      continue;
    }
    if (c === 45 || (c >= 48 && c <= 57)) {
      NUMBER.lastIndex = i;
      const m = NUMBER.exec(text);
      if (!m) continue;
      const literal = m[0];
      if (literal.replace(/[-.eE+]/g, '').length > 15 && !sameNumber(literal)) {
        out += `${text.slice(last, i)}"${EXACT}${literal}"`;
        last = i + literal.length;
      }
      i += literal.length - 1;
    }
  }
  return JSON.parse(out + text.slice(last)) as J;
}

function sameNumber(literal: string): boolean {
  const n = Number(literal);
  if (!Number.isFinite(n)) return false;
  if (/^-?\d+$/.test(literal))
    return BigInt(literal) === BigInt(Math.trunc(n)) && Number.isSafeInteger(n);
  return String(n) === literal;
}

function typeOf(v: J): string {
  if (v === null) return 'null';
  if (Array.isArray(v)) return 'array';
  return typeof v;
}

function preview(v: J): string {
  if (Array.isArray(v)) return v.length ? `[ ${v.length} ]` : '[ ]';
  if (v && typeof v === 'object') {
    const keys = Object.keys(v);
    if (!keys.length) return '{ }';
    const head = keys.slice(0, 4).join(', ');
    return `{ ${head}${keys.length > 4 ? `, … ${keys.length - 4} more` : ''} }`;
  }
  return '';
}

function primitive(v: J): HTMLElement {
  const t = typeOf(v);
  if (t === 'string' && (v as string).startsWith(EXACT)) {
    return h('span.jv.jv-number', (v as string).slice(EXACT.length));
  }
  if (t === 'string') {
    const s = v as string;
    const el = h('span.jv.jv-string', JSON.stringify(s.length > 5000 ? `${s.slice(0, 5000)}…` : s));
    if (/^https?:\/\//.test(s)) el.classList.add('jv-url');
    return el;
  }
  return h(`span.jv.jv-${t}`, String(v));
}

function node(key: string | null, v: J, depth: number, expandDepth: number): HTMLElement {
  const t = typeOf(v);
  const keyEl = key !== null ? h('span.jk', key) : null;
  if (t !== 'object' && t !== 'array') {
    return h(
      'div.jn',
      h('span.jcaret.leaf'),
      keyEl,
      keyEl ? h('span.jcolon', ': ') : null,
      primitive(v),
    );
  }
  const entries: [string, J][] = Array.isArray(v)
    ? v.map((x, i) => [String(i), x])
    : Object.entries(v as Record<string, J>);
  const caret = h('span.jcaret');
  const summary = h('span.jsum', preview(v));
  const head = h('div.jn.jhead', caret, keyEl, keyEl ? h('span.jcolon', ': ') : null, summary);
  const kids = h('div.jkids');
  const wrap = h('div.jgroup', head, kids);
  let rendered = 0;
  let open = false;

  const renderMore = () => {
    const end = Math.min(entries.length, rendered + CHUNK);
    const frag = document.createDocumentFragment();
    for (let i = rendered; i < end; i++)
      frag.append(
        node(Array.isArray(v) ? `${i}` : entries[i][0], entries[i][1], depth + 1, expandDepth),
      );
    rendered = end;
    kids.querySelector(':scope > .jmore')?.remove();
    kids.append(frag);
    if (rendered < entries.length) {
      const more = h(
        'button.btn.ghost.jmore',
        `Show ${Math.min(CHUNK, entries.length - rendered)} more of ${entries.length - rendered}`,
      );
      more.addEventListener('click', (e) => {
        e.stopPropagation();
        renderMore();
      });
      kids.append(more);
    }
  };
  const toggle = (state = !open) => {
    open = state;
    wrap.classList.toggle('open', open);
    if (open && rendered === 0) renderMore();
  };
  head.addEventListener('click', () => toggle());
  (wrap as HTMLElement & { _toggle?: (s: boolean) => void })._toggle = toggle;
  if (depth < expandDepth && entries.length <= 500) toggle(true);
  return wrap;
}

export async function mount(host: HTMLElement, ctx: ViewCtx): Promise<Mounted> {
  const t = await api.readText(ctx.info.path, TREE_LIMIT);
  // Navigated away while loading: build nothing for a preview that is gone.
  if (ctx.signal.aborted) return {};
  if (t.truncated) throw new Error('Large JSON: showing as text.');
  let data: J;
  const text = t.text.replace(/^﻿/, '');
  if (ctx.info.format === 'jsonl' || ctx.info.format === 'ndjson') {
    data = text
      .split(/\r?\n/)
      .filter((l) => l.trim())
      .map((l) => parseExact(l));
  } else {
    data = parseExact(text);
  }

  const tree = h('div.json-tree.selectable', node(null, data, 0, 2));
  const treeScroll = h('div.prose-scroll.json-scroll', tree);
  const raw = h('div.sub-host.hidden');
  host.append(treeScroll, raw);

  let rawMounted: Mounted | null = null;
  const seg = h('div.segmented');
  const bTree = h('button.active', 'Tree');
  const bRaw = h('button', 'Raw');
  seg.append(bTree, bRaw);
  const setRaw = (on: boolean) => {
    bTree.classList.toggle('active', !on);
    bRaw.classList.toggle('active', on);
    treeScroll.classList.toggle('hidden', on);
    raw.classList.toggle('hidden', !on);
    if (on && !rawMounted) rawMounted = renderCode(raw, { ...ctx, nested: true }, t, 'json');
  };
  bTree.addEventListener('click', () => setRaw(false));
  bRaw.addEventListener('click', () => setRaw(true));
  type Group = HTMLElement & { _toggle?: (s: boolean) => void };
  const setAll = (open: boolean) => {
    if (!open) {
      for (const g of tree.querySelectorAll<Group>('.jgroup')) g._toggle?.(false);
      (tree.firstElementChild as Group)?._toggle?.(true);
      return;
    }
    // Opening a group mounts its children, which may hold closed groups of
    // their own: repeat, level by level, up to 2,000 groups.
    let n = 0;
    for (;;) {
      const closed = tree.querySelectorAll<Group>('.jgroup:not(.open)');
      if (!closed.length) return;
      for (const g of closed) {
        if (++n > 2000) return;
        g._toggle?.(true);
      }
    }
  };
  ctx.toolbar([
    { title: 'View', el: seg },
    { separator: true, title: '' },
    { icon: 'list', title: 'Expand all', onClick: () => setAll(true) },
    { icon: 'minimize', title: 'Collapse all', onClick: () => setAll(false) },
  ]);

  const kind = typeOf(data);
  const count = Array.isArray(data)
    ? `${formatCount(data.length)} items`
    : data && typeof data === 'object'
      ? `${formatCount(Object.keys(data).length)} keys`
      : kind;
  ctx.setStatus(`${kind === 'array' ? 'Array' : kind === 'object' ? 'Object' : kind}  ·  ${count}`);
  ctx.setDetails([
    ['Root', kind],
    ['Size', count],
    ['Lines', formatCount(t.lines)],
  ]);

  return {
    keydown: (e) =>
      !raw.classList.contains('hidden') ? (rawMounted?.keydown?.(e) ?? false) : false,
  };
}
