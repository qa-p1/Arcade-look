// Word processing documents (DOCX, ODT, RTF) and EPUB books on a paper-like page.
import './doc.css';
import { api } from '../lib/backend';
import { h } from '../lib/dom';
import * as fmt from '../lib/format';
import { sanitize, wireLinks } from '../lib/sanitize';
import type { Mounted, ViewCtx } from './types';

export async function mount(host: HTMLElement, ctx: ViewCtx): Promise<Mounted> {
  const { info } = ctx;
  const d = await api.readDocument(info.path, info.kind === 'epub' ? 'epub' : info.format);
  const paper = h(`article.prose.paper.selectable${info.kind === 'epub' ? '.book' : ''}`);
  paper.append(sanitize(d.html, { baseDir: info.dir, allowDataImages: true }));
  if (!paper.textContent?.trim() && !paper.querySelector('img')) {
    paper.append(h('p.muted', 'This document has no text content.'));
  }
  wireLinks(paper, { openLocal: (p) => ctx.open(p), openExternal: (u) => void api.openUrl(u) });
  if (d.truncated) host.append(h('div.notice', 'This document is very long; only the beginning is shown.'));
  host.append(h('div.prose-scroll.doc-scroll', paper));
  const words = (paper.textContent ?? '').trim().split(/\s+/).filter(Boolean).length;
  const pages = d.meta.find(([k]) => k === 'Pages')?.[1];
  ctx.setStatus([d.title ?? '', pages ? `${pages} pages` : '', fmt.plural(words, 'word')].filter(Boolean).join('  ·  '));
  ctx.setDetails([...d.meta, ['Words', words.toLocaleString()]]);
  return {};
}
