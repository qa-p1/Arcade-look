import './code.css';
import { api } from '../lib/backend';
import { h } from '../lib/dom';
import * as fmt from '../lib/format';
import { highlightBlocks } from '../lib/highlight';
import { sanitize, wireLinks } from '../lib/sanitize';
import { renderCode } from './code';
import type { Mounted, ViewCtx } from './types';

export async function mount(host: HTMLElement, ctx: ViewCtx): Promise<Mounted> {
  const { info, signal } = ctx;
  const md = await api.renderMarkdown(info.path);
  const article = h('article.prose.markdown');
  article.append(sanitize(md.html, { baseDir: info.dir }));
  wireLinks(article, {
    openLocal: (p) => ctx.open(p),
    openExternal: (u) => void api.openUrl(u).catch((e) => ctx.toast(String(e))),
  });
  const preview = h('div.prose-scroll', article);
  const source = h('div.sub-host.hidden');
  host.append(preview, source);
  void highlightBlocks(article, signal);

  const words = (article.textContent ?? '').trim().split(/\s+/).filter(Boolean).length;
  const minutes = Math.max(1, Math.round(words / 230));
  ctx.setStatus(`${fmt.plural(words, 'word')}  ·  ${minutes} min read`);
  ctx.setDetails([
    ['Words', words.toLocaleString()],
    ['Headings', String(article.querySelectorAll('h1,h2,h3,h4,h5,h6').length)],
    ['Links', String(article.querySelectorAll('a').length)],
    ['Images', String(article.querySelectorAll('img').length)],
  ]);

  let sourceMounted: Mounted | null = null;
  let showingSource = false;
  const seg = h('div.segmented');
  const bPrev = h('button.active', 'Preview');
  const bSrc = h('button', 'Source');
  seg.append(bPrev, bSrc);
  const setMode = async (src: boolean) => {
    showingSource = src;
    bPrev.classList.toggle('active', !src);
    bSrc.classList.toggle('active', src);
    preview.classList.toggle('hidden', src);
    source.classList.toggle('hidden', !src);
    if (src && !sourceMounted) {
      const t = await api.readText(info.path);
      sourceMounted = renderCode(source, { ...ctx, nested: true }, t, 'markdown');
    }
  };
  bPrev.addEventListener('click', () => void setMode(false));
  bSrc.addEventListener('click', () => void setMode(true));
  ctx.toolbar([{ title: 'View', el: seg }]);

  return {
    keydown(e) {
      if (!e.ctrlKey && !e.metaKey && (e.key === 's' || e.key === 'S')) {
        void setMode(!showingSource);
        return true;
      }
      return showingSource ? (sourceMounted?.keydown?.(e) ?? false) : false;
    },
  };
}
