// Arcade Link v0.4.3 supplies every UI path and its stroke weight.
import { icons, filledIcons, ICON_STROKE, type IconName as KitIconName } from '../design/icons';
import { svg } from './dom';
import type { Kind } from './backend';

const aliases: Record<string, KitIconName> = {
  close: 'x',
  open: 'external',
  reveal: 'showInFolder',
  left: 'chevronLeft',
  right: 'chevronRight',
  back: 'arrowLeft',
  play: 'playFilled',
  pause: 'pauseFilled',
  doc: 'document',
  model: 'cube',
  grid: 'layout',
  sparkles: 'sparkle',
  reset: 'restart',
  external: 'arrowUpRight',
};
export type IconName = KitIconName | keyof typeof aliases;

export function icon(name: IconName | string, size = 16): SVGElement {
  const key = aliases[name] ?? (name in icons ? (name as KitIconName) : 'file');
  const filled = filledIcons.has(key);
  return svg(`<svg class="icon" width="${size}" height="${size}" viewBox="0 0 24 24"
    fill="${filled ? 'currentColor' : 'none'}" stroke="${filled ? 'none' : 'currentColor'}"
    stroke-width="${ICON_STROKE}" stroke-linecap="round" stroke-linejoin="round"
    aria-hidden="true"><path d="${icons[key]}"/></svg>`);
}

/**
 * Visual identity (icon + hue) for a file kind; used by badges and folder listings.
 * The hues are Look's content palette for file kinds (LOOK.md §1), not UI colours.
 */
export function kindStyle(kind: Kind | string, ext = ''): { icon: string; color: string } {
  switch (kind) {
    case 'folder':
      return { icon: 'folder', color: '#60a5fa' };
    case 'image':
    case 'image-decode':
    case 'image-raw':
    case 'image-psd':
    case 'svg':
      return { icon: 'image', color: '#f59e0b' };
    case 'video':
      return { icon: 'video', color: '#f43f5e' };
    case 'audio':
      return { icon: 'audio', color: '#ec4899' };
    case 'pdf':
      return { icon: 'doc', color: '#ef4444' };
    case 'markdown':
    case 'text':
    case 'doc':
      return { icon: 'doc', color: '#3b82f6' };
    case 'document':
    case 'epub':
      return { icon: 'book', color: '#2563eb' };
    case 'code':
    case 'html':
    case 'notebook':
      return { icon: 'code', color: '#10b981' };
    case 'json':
      return { icon: 'code', color: '#14b8a6' };
    case 'csv':
    case 'spreadsheet':
    case 'table':
      return { icon: 'table', color: '#22c55e' };
    case 'presentation':
    case 'slides':
      return { icon: 'slides', color: '#f97316' };
    case 'archive':
      return { icon: 'archive', color: '#eab308' };
    case 'font':
      return { icon: 'font', color: '#a78bfa' };
    case 'model':
      return { icon: 'model', color: '#06b6d4' };
    default:
      return { icon: ext ? 'file' : 'binary', color: '#94a3b8' };
  }
}

export function kindBadge(kind: Kind | string, ext = '', size = 18): HTMLElement {
  const { icon: name, color } = kindStyle(kind, ext);
  const el = document.createElement('span');
  el.className = 'kind-badge';
  el.style.setProperty('--kind', color);
  el.appendChild(icon(name, size));
  return el;
}
