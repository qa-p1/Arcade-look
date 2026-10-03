// Maps a file to an ordered chain of viewers. Each viewer is a lazily loaded chunk; if
// one throws, the next is tried, ending with the info card + hex view, which always works.
import type { FileInfo } from '../lib/backend';
import { errorMessage } from '../lib/backend';
import { clear } from '../lib/dom';
import type { Mounted, ViewCtx, ViewerModule } from './types';

const loaders: Record<string, () => Promise<ViewerModule>> = {
  image: () => import('./image'),
  video: () => import('./video'),
  audio: () => import('./audio'),
  pdf: () => import('./pdf'),
  code: () => import('./code'),
  markdown: () => import('./markdown'),
  json: () => import('./json'),
  notebook: () => import('./notebook'),
  table: () => import('./table'),
  archive: () => import('./archive'),
  font: () => import('./font'),
  model: () => import('./model'),
  html: () => import('./html'),
  folder: () => import('./folder'),
  document: () => import('./document'),
  slides: () => import('./slides'),
  plugin: () => import('./plugin'),
  'fallback-plugin': () => import('./plugin'),
  info: () => import('./info'),
};

export function chainFor(info: FileInfo): string[] {
  const c: string[] = [];
  if (info.plugin) c.push('plugin');
  switch (info.kind) {
    case 'image': case 'image-decode': case 'image-raw': case 'image-psd': case 'svg':
      c.push('image'); break;
    case 'video': c.push('video'); break;
    case 'audio': c.push('audio'); break;
    case 'pdf': c.push('pdf'); break;
    case 'markdown': c.push('markdown', 'code'); break;
    case 'code': case 'text': c.push('code'); break;
    case 'json': c.push('json', 'code'); break;
    case 'notebook': c.push('notebook', 'json'); break;
    case 'csv': c.push('table', 'code'); break;
    case 'spreadsheet': c.push('table'); break;
    case 'document': case 'epub': c.push('document'); break;
    case 'presentation': c.push('slides'); break;
    case 'archive': c.push('archive'); break;
    case 'font': c.push('font'); break;
    case 'model': c.push('model'); break;
    case 'html': c.push('html', 'code'); break;
    case 'folder': c.push('folder'); break;
    default: break;
  }
  if (info.fallbackPlugin) c.push('fallback-plugin');
  c.push('info');
  return c;
}

/** Warm the chunk for a kind (e.g. on hover or for the next file). */
export function preload(info: FileInfo) {
  const first = chainFor(info)[0];
  loaders[first]?.().catch(() => {});
}

export interface MountResult {
  mounted: Mounted;
  viewer: string;
}

export async function mountChain(host: HTMLElement, base: ViewCtx): Promise<MountResult> {
  // An error known up front (e.g. the file vanished) goes straight to the info card.
  let previousError: string | null = base.previousError;
  const chain = chainFor(base.info);
  for (const name of chain) {
    if (base.signal.aborted) throw new DOMException('aborted', 'AbortError');
    clear(host);
    host.dataset.viewer = name;
    try {
      const mod = await loaders[name]();
      const ctx: ViewCtx = { ...base, previousError };
      if (name === 'fallback-plugin') {
        ctx.info = { ...base.info, plugin: base.info.fallbackPlugin };
      }
      const mounted = await mod.mount(host, ctx);
      if (base.signal.aborted) {
        mounted.dispose?.();
        throw new DOMException('aborted', 'AbortError');
      }
      return { mounted, viewer: name };
    } catch (e) {
      if (base.signal.aborted || (e instanceof DOMException && e.name === 'AbortError')) throw e;
      previousError = errorMessage(e);
      console.warn(`viewer "${name}" failed for ${base.info.path}:`, e);
    }
  }
  // Unreachable in practice: the info viewer never throws.
  return { mounted: {}, viewer: 'none' };
}
