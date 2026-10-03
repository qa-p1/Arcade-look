import './code.css';
import { api, type FileInfo, type TextData } from '../lib/backend';
import { h } from '../lib/dom';
import * as fmt from '../lib/format';
import { highlight, LANG_NAMES, resolveLang } from '../lib/highlight';
import type { Mounted, ViewCtx } from './types';

function langFor(info: FileInfo): string {
  if (info.kind === 'markdown') return 'markdown';
  if (info.kind === 'json' || info.kind === 'notebook') return 'json';
  if (info.kind === 'html' || info.kind === 'svg') return 'xml';
  return info.lang ?? 'plaintext';
}

/** Render text with line numbers into `host`; used directly and by other viewers' "Source" mode. */
export function renderCode(host: HTMLElement, ctx: ViewCtx, t: TextData, lang: string): Mounted {
  const { signal } = ctx;
  if (t.truncated) {
    host.append(h('div.notice', `Showing the first ${fmt.bytes(t.text.length)} of ${fmt.bytes(t.size)}. Open the file to see everything.`));
  }
  const lines = t.text.endsWith('\n') ? t.lines : Math.max(1, t.lines);
  const gutter = h('pre.gutter', { 'aria-hidden': 'true' });
  gutter.textContent = Array.from({ length: lines }, (_, i) => i + 1).join('\n');
  const code = h('code');
  code.textContent = t.text;
  const body = h('pre.code-body.selectable', code);
  const view = h('div.code-view', gutter, body);
  const scroll = h('div.code-scroll', view);
  host.append(scroll);

  let size = 13;
  const setSize = (s: number) => {
    size = Math.min(28, Math.max(9, s));
    view.style.setProperty('--code-size', `${size}px`);
  };
  let wrap = false;
  const setWrap = (on: boolean) => {
    wrap = on;
    view.classList.toggle('wrap', on);
    wrapBtn?.classList.toggle('active', on);
  };

  const limit = ctx.boot.config.highlightLimitKb * 1024;
  const resolved = resolveLang(lang);
  if (resolved && t.text.length <= limit) {
    // Plain text paints instantly; colours arrive a moment later.
    void highlight(t.text, resolved).then((html) => {
      if (html !== null && !signal.aborted) {
        code.innerHTML = html;
        code.classList.add('hljs');
      }
    });
  }

  const label = LANG_NAMES[resolved ?? lang] ?? (lang === 'plaintext' ? 'Plain text' : lang);
  const bar = ctx.nested
    ? null
    : ctx.toolbar([
        { title: 'Language', el: h('span.tb-label', label) },
        { separator: true, title: '' },
        { icon: 'wrap', title: 'Wrap lines (W)', onClick: () => setWrap(!wrap) },
        { icon: 'zoomOut', title: 'Smaller text (−)', onClick: () => setSize(size - 1) },
        { icon: 'zoomIn', title: 'Larger text (+)', onClick: () => setSize(size + 1) },
        { icon: 'copy', title: 'Copy all text', onClick: () => void navigator.clipboard.writeText(t.text).then(() => ctx.toast('Copied to clipboard')) },
      ]);
  const wrapBtn = bar?.querySelectorAll('button')[0] ?? null;

  if (!ctx.nested) {
    const enc = t.encoding !== 'UTF-8' ? `  ·  ${t.encoding}` : '';
    ctx.setStatus(`${fmt.plural(lines, 'line')}${enc}`);
    ctx.setDetails([
      ['Language', label],
      ['Lines', lines.toLocaleString()],
      ['Characters', t.text.length.toLocaleString()],
      ['Encoding', t.encoding],
      ['Line endings', /\r\n/.test(t.text.slice(0, 65536)) ? 'Windows (CRLF)' : 'Unix (LF)'],
    ]);
  }
  // Long single-line files (minified) read better wrapped.
  if (lines <= 3 && t.text.length > 2000) setWrap(true);

  return {
    keydown(e) {
      if (e.ctrlKey || e.metaKey || e.altKey) return false;
      switch (e.key) {
        case 'w': case 'W': setWrap(!wrap); return true;
        case '+': case '=': setSize(size + 1); return true;
        case '-': case '_': setSize(size - 1); return true;
        case '0': setSize(13); return true;
        case 'Home': scroll.scrollTop = 0; return true;
        case 'End': scroll.scrollTop = scroll.scrollHeight; return true;
      }
      return false;
    },
  };
}

export async function mount(host: HTMLElement, ctx: ViewCtx): Promise<Mounted> {
  const t = await api.readText(ctx.info.path);
  if (ctx.info.kind === 'binary' && /\u0000/.test(t.text.slice(0, 4096))) {
    throw new Error('This file is binary.');
  }
  return renderCode(host, ctx, t, langFor(ctx.info));
}
