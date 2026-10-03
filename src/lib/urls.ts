// URLs for the `alook://` protocol (see src-tauri/src/protocol.rs). Paths go into the URL
// path (one encoded segment per path component) so relative references inside files —
// a glTF's .bin, an HTML page's images — resolve naturally.
import { convertFileSrc } from '@tauri-apps/api/core';

let base: string | null = null;

/** "alook://localhost/" on macOS/Linux, "http://alook.localhost/" on Windows. */
export function protocolBase(): string {
  if (base === null) {
    try {
      base = convertFileSrc('', 'alook');
    } catch {
      base = 'alook://localhost/';
    }
    if (!base.endsWith('/')) base += '/';
  }
  return base;
}

export function encodePath(path: string): string {
  let s = path.replace(/\\/g, '/');
  if (s.startsWith('/')) s = s.slice(1);
  return s.split('/').map(encodeURIComponent).join('/');
}

export const fileUrl = (path: string) => `${protocolBase()}f/${encodePath(path)}`;

let mediaBase: string | null = null;
export function setMediaBase(b: string | null) {
  mediaBase = b;
}
/** URL for <video>/<audio>: the loopback media server on Linux, the custom protocol elsewhere. */
export const mediaUrl = (path: string) => (mediaBase ? `${mediaBase}f/${encodePath(path)}` : fileUrl(path));
export const dirUrl = (dir: string) => `${fileUrl(dir)}/`;
export const imageUrl = (path: string, kind: 'decode' | 'raw' | 'psd', max: number) =>
  `${protocolBase()}img/${kind}/${Math.round(max)}/${encodePath(path)}`;
export const coverUrl = (path: string) => `${protocolBase()}cover/${encodePath(path)}`;
export const pluginUrl = (id: string, file: string) =>
  `${protocolBase()}plugin/${encodeURIComponent(id)}/${file.split('/').map(encodeURIComponent).join('/')}`;

/** Join a relative reference (from a document) onto a directory path. */
export function resolveRelative(dir: string, rel: string): string {
  const win = /^[a-zA-Z]:[\\/]/.test(dir) || dir.startsWith('\\\\');
  const sep = win ? '\\' : '/';
  let clean = rel.split(/[?#]/)[0];
  try {
    clean = decodeURIComponent(clean);
  } catch {
    /* keep as is */
  }
  const parts = dir.replace(/[\\/]+$/, '').split(/[\\/]/);
  const relParts = clean.replace(/^[\\/]+/, '').split(/[\\/]/);
  for (const p of relParts) {
    if (p === '..') {
      if (parts.length > 1) parts.pop();
    } else if (p !== '.' && p !== '') {
      parts.push(p);
    }
  }
  return parts.join(sep) || sep;
}

export function isExternalUrl(u: string): boolean {
  return /^(https?:|mailto:)/i.test(u);
}
