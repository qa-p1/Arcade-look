// Presentations (PPTX, ODP) as a scrollable stack of slide cards.
import './doc.css';
import { api, type Slide } from '../lib/backend';
import { h } from '../lib/dom';
import type { Mounted, ViewCtx } from './types';

function slideCard(s: Slide, i: number, aspect: number): HTMLElement {
  const body = h('div.slide-body');
  const hasBody = () => body.childElementCount > 0;
  let level0 = 0;
  const texts: HTMLElement[] = [];
  const media: HTMLElement[] = [];
  for (const it of s.items) {
    if (it.type === 'text') {
      level0++;
      texts.push(h(`div.slide-text.lvl-${Math.min(it.level, 4)}`, it.text));
    } else if (it.type === 'image') {
      media.push(h('img.slide-img', { src: it.src, alt: '', loading: 'lazy' }));
    } else if (it.type === 'table') {
      const t = h('table.slide-table');
      it.rows.forEach((r, ri) => t.append(h('tr', r.map((c) => h(ri === 0 ? 'th' : 'td', c)))));
      media.push(t);
    }
  }
  if (texts.length) body.append(h('div.slide-texts', texts));
  if (media.length) body.append(h(`div.slide-media${texts.length ? '' : '.only'}`, media));
  const onlyTitle = !level0 && !media.length;
  return h('section.slide-wrap',
    h('div.slide-num', String(i + 1)),
    h(`div.slide${onlyTitle ? '.title-only' : ''}`, { style: `aspect-ratio:${aspect}` },
      s.title ? h('h2.slide-title', s.title) : null,
      s.subtitle ? h('div.slide-subtitle', s.subtitle) : null,
      hasBody() ? body : null),
    s.notes ? h('details.slide-notes', h('summary', 'Speaker notes'), h('div', s.notes)) : null);
}

export async function mount(host: HTMLElement, ctx: ViewCtx): Promise<Mounted> {
  const deck = await api.readSlides(ctx.info.path, ctx.info.format);
  const list = h('div.slides.selectable');
  deck.slides.forEach((s, i) => list.append(slideCard(s, i, deck.aspect || 16 / 9)));
  if (!deck.slides.length) list.append(h('div.center-fill', 'This presentation has no slides.'));
  host.append(h('div.prose-scroll.slides-scroll', list));
  const n = deck.slides.length;
  ctx.setStatus([deck.title ?? '', `${n} ${n === 1 ? 'slide' : 'slides'}`].filter(Boolean).join('  ·  '));
  ctx.setDetails([...deck.meta, ['Slides', String(n)], ['Aspect ratio', deck.aspect > 1.5 ? '16:9' : '4:3']]);
  return {};
}
