// Plugins: command plugins (external tools) and script plugins (ES modules).
import './doc.css';
import { api } from '../lib/backend';
import { h } from '../lib/dom';
import { highlightBlocks } from '../lib/highlight';
import { sanitize, wireLinks } from '../lib/sanitize';
import { fileUrl, pluginUrl } from '../lib/urls';
import { renderCode } from './code';
import { sandboxedFrame } from './html';
import type { Mounted, ViewCtx } from './types';

/** What script plugins receive. Kept small and stable: see docs/PLUGINS.md. */
export interface PluginContext {
  file: ViewCtx['info'];
  fileUrl(path: string): string;
  readText(path: string, maxBytes?: number): Promise<string>;
  readBytes(path: string, offset: number, length: number): Promise<ArrayBuffer>;
  setStatus(text: string): void;
  setDetails(rows: [string, string][]): void;
  signal: AbortSignal;
}

export async function mount(host: HTMLElement, ctx: ViewCtx): Promise<Mounted> {
  const p = ctx.info.plugin;
  if (!p) throw new Error('No plugin');

  if (p.type === 'script') {
    const mod = (await import(/* @vite-ignore */ pluginUrl(p.id, p.entry ?? 'index.js'))) as {
      render?: (host: HTMLElement, c: PluginContext) => unknown;
      default?: { render?: (host: HTMLElement, c: PluginContext) => unknown };
    };
    const render = mod.render ?? mod.default?.render;
    if (!render) throw new Error(`Plugin "${p.name}" does not export render()`);
    const box = h('div.plugin-host', { style: 'flex:1;min-height:0;position:relative;overflow:auto' });
    host.append(box);
    const result = (await render(box, {
      file: ctx.info,
      fileUrl,
      readText: async (path, max) => (await api.readText(path, max)).text,
      readBytes: api.readBytes,
      setStatus: ctx.setStatus,
      setDetails: ctx.setDetails,
      signal: ctx.signal,
    })) as { dispose?: () => void } | undefined;
    return { dispose: () => result?.dispose?.() };
  }

  const out = await api.runPlugin(p.id, ctx.info.path);
  ctx.setDetails([['Rendered by', p.name]]);
  switch (out.output) {
    case 'html':
      host.append(h('div.html-host', sandboxedFrame(out.text ?? '', ctx.info.dir)));
      return {};
    case 'markdown': {
      const [html] = await api.markdownBatch([out.text ?? '']);
      const article = h('article.prose.selectable');
      article.append(sanitize(html, { baseDir: ctx.info.dir, allowDataImages: true }));
      wireLinks(article, { openLocal: (x) => ctx.open(x), openExternal: (u) => void api.openUrl(u) });
      host.append(h('div.prose-scroll', article));
      void highlightBlocks(article, ctx.signal);
      return {};
    }
    case 'image': {
      const img = h('img', { src: fileUrl(out.path ?? ''), alt: '', style: 'max-width:100%;max-height:100%;object-fit:contain;margin:auto' });
      await img.decode();
      host.append(h('div.center-fill', img));
      return {};
    }
    case 'file': {
      const produced = await api.inspect(out.path ?? '');
      produced.plugin = null; // don't re-run the plugin on its own output
      produced.fallbackPlugin = null;
      const box = h('div.sub-host');
      host.append(box);
      return ctx.mountNested(box, produced);
    }
    default: {
      const text = out.text ?? '';
      return renderCode(host, ctx, { text, encoding: 'UTF-8', truncated: false, size: text.length, lines: text.split('\n').length }, out.language ?? 'plaintext');
    }
  }
}
