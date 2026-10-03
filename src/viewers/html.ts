// HTML rendered in a fully sandboxed iframe: no scripts, no forms, no same-origin access.
import './doc.css';
import { api } from '../lib/backend';
import { h } from '../lib/dom';
import { dirUrl } from '../lib/urls';
import { renderCode } from './code';
import type { Mounted, ViewCtx } from './types';

export function sandboxedFrame(html: string, baseDir: string | null): HTMLIFrameElement {
  const inject = `${baseDir ? `<base href="${dirUrl(baseDir)}">` : ''}<meta http-equiv="Content-Security-Policy" content="script-src 'none'; object-src 'none'; form-action 'none'">`;
  const doc = /<head[^>]*>/i.test(html) ? html.replace(/<head[^>]*>/i, (m) => m + inject) : inject + html;
  const frame = h('iframe.html-frame') as HTMLIFrameElement;
  frame.setAttribute('sandbox', '');
  frame.setAttribute('referrerpolicy', 'no-referrer');
  frame.srcdoc = doc;
  return frame;
}

export async function mount(host: HTMLElement, ctx: ViewCtx): Promise<Mounted> {
  const t = await api.readText(ctx.info.path);
  const rendered = h('div.html-host', sandboxedFrame(t.text, ctx.info.dir));
  const source = h('div.sub-host.hidden');
  host.append(rendered, source);
  const title = t.text.match(/<title[^>]*>([^<]*)<\/title>/i)?.[1]?.trim();
  if (title) ctx.setStatus(title);
  ctx.setDetails([['Title', title ?? ''], ['Lines', t.lines.toLocaleString()], ['Encoding', t.encoding]]);

  let src: Mounted | null = null;
  const seg = h('div.segmented');
  const bR = h('button.active', 'Rendered');
  const bS = h('button', 'Source');
  seg.append(bR, bS);
  const set = (s: boolean) => {
    bR.classList.toggle('active', !s);
    bS.classList.toggle('active', s);
    rendered.classList.toggle('hidden', s);
    source.classList.toggle('hidden', !s);
    if (s && !src) src = renderCode(source, { ...ctx, nested: true }, t, 'xml');
  };
  bR.addEventListener('click', () => set(false));
  bS.addEventListener('click', () => set(true));
  ctx.toolbar([{ title: 'View', el: seg }]);
  return { keydown: (e) => (!source.classList.contains('hidden') ? (src?.keydown?.(e) ?? false) : false) };
}
