// Allow-list HTML sanitiser. Untrusted HTML (Markdown with raw HTML, DOCX/EPUB output,
// notebook outputs) is parsed in an inert document and rebuilt from scratch, so only known
// tags and attributes survive. Scripts, event handlers, styles, forms and frames never do.
import { fileUrl, isExternalUrl, resolveRelative } from './urls';

const TAGS = new Set([
  'a', 'abbr', 'b', 'blockquote', 'br', 'caption', 'cite', 'code', 'col', 'colgroup', 'dd', 'del',
  'details', 'dfn', 'div', 'dl', 'dt', 'em', 'figcaption', 'figure', 'footer', 'h1', 'h2', 'h3', 'h4',
  'h5', 'h6', 'header', 'hr', 'i', 'img', 'ins', 'kbd', 'li', 'mark', 'ol', 'p', 'pre', 'q', 's', 'samp',
  'section', 'small', 'span', 'strong', 'sub', 'summary', 'sup', 'table', 'tbody', 'td', 'tfoot', 'th',
  'thead', 'tr', 'u', 'ul', 'var', 'article', 'aside', 'nav', 'main', 'time', 'center', 'tt', 'big', 'strike',
]);

/** Elements removed together with everything inside them. */
const DROP = new Set([
  'script', 'style', 'iframe', 'object', 'embed', 'noscript', 'template', 'form', 'button', 'select',
  'textarea', 'link', 'meta', 'base', 'frame', 'frameset', 'applet', 'svg', 'math', 'audio', 'video',
  'canvas', 'title', 'head', 'dialog', 'portal',
]);

const ATTRS: Record<string, string[]> = {
  '*': ['id', 'class', 'title', 'lang', 'dir', 'align'],
  a: ['href', 'name'],
  img: ['src', 'alt', 'width', 'height'],
  td: ['colspan', 'rowspan', 'width'],
  th: ['colspan', 'rowspan', 'width', 'scope'],
  table: ['width'],
  col: ['span', 'width'],
  ol: ['start', 'reversed', 'type'],
  li: ['value'],
  details: ['open'],
  input: ['type', 'checked', 'disabled'],
};

export interface SanitizeOptions {
  /** Directory used to resolve relative links and images. */
  baseDir?: string | null;
  /** Allow data: URIs for images (embedded document images). */
  allowDataImages?: boolean;
}

export function sanitize(html: string, opts: SanitizeOptions = {}): DocumentFragment {
  const doc = new DOMParser().parseFromString(`<!doctype html><body>${html}`, 'text/html');
  const out = document.createDocumentFragment();
  copyChildren(doc.body, out, opts);
  return out;
}

function copyChildren(from: Node, to: Node, opts: SanitizeOptions) {
  for (const n of Array.from(from.childNodes)) {
    if (n.nodeType === Node.TEXT_NODE) {
      to.appendChild(document.createTextNode(n.textContent ?? ''));
    } else if (n.nodeType === Node.ELEMENT_NODE) {
      const el = n as Element;
      const tag = el.tagName.toLowerCase();
      if (DROP.has(tag)) continue;
      if (tag === 'input') {
        // Only GitHub-style task list checkboxes.
        if ((el.getAttribute('type') || '').toLowerCase() === 'checkbox') {
          const cb = document.createElement('input');
          cb.type = 'checkbox';
          cb.disabled = true;
          cb.checked = el.hasAttribute('checked');
          to.appendChild(cb);
        }
        continue;
      }
      if (!TAGS.has(tag)) {
        // Unknown wrappers (picture, font, custom elements…): keep their content.
        copyChildren(el, to, opts);
        continue;
      }
      const clean = document.createElement(tag);
      const allowed = new Set([...ATTRS['*'], ...(ATTRS[tag] ?? [])]);
      for (const attr of Array.from(el.attributes)) {
        const name = attr.name.toLowerCase();
        if (!allowed.has(name)) continue;
        let value = attr.value;
        if (name === 'href') {
          const v = safeHref(value, opts);
          if (!v) continue;
          if (v.local) clean.setAttribute('data-local-path', v.local);
          value = v.href;
        } else if (name === 'src') {
          const v = safeSrc(value, opts);
          if (!v) continue;
          value = v;
        } else if (name === 'id' || name === 'name') {
          value = value.slice(0, 200);
        }
        clean.setAttribute(name, value);
      }
      if (tag === 'img') {
        clean.setAttribute('loading', 'lazy');
        clean.setAttribute('decoding', 'async');
        if (!clean.hasAttribute('src')) continue;
      }
      copyChildren(el, clean, opts);
      to.appendChild(clean);
    }
  }
}

function scheme(u: string): string {
  const m = u.trim().match(/^([a-z][a-z0-9+.-]*):/i);
  return m ? m[1].toLowerCase() : '';
}

function safeHref(raw: string, opts: SanitizeOptions): { href: string; local?: string } | null {
  const u = raw.trim();
  if (!u) return null;
  if (u.startsWith('#')) return { href: u };
  const s = scheme(u);
  if (s === 'http' || s === 'https' || s === 'mailto') return { href: u };
  if (s === 'file') {
    try {
      return { href: '#', local: decodeURIComponent(new URL(u).pathname) };
    } catch {
      return null;
    }
  }
  if (s && !/^[a-z]$/i.test(s)) return null; // javascript:, data:, vbscript:… (single letter = drive)
  if (opts.baseDir) {
    const [path, hash] = u.split('#');
    if (!path) return { href: `#${hash ?? ''}` };
    return { href: '#', local: resolveRelative(opts.baseDir, path) };
  }
  return null;
}

function safeSrc(raw: string, opts: SanitizeOptions): string | null {
  const u = raw.trim();
  if (!u) return null;
  const s = scheme(u);
  if (s === 'https' || s === 'http') return u;
  if (s === 'data') return opts.allowDataImages && /^data:image\/(png|jpe?g|gif|webp|bmp|svg\+xml|avif)[;,]/i.test(u) ? u : null;
  if (s === 'alook') return u;
  if (s && !/^[a-z]$/i.test(s)) return null;
  if (opts.baseDir) return fileUrl(resolveRelative(opts.baseDir, u));
  return null;
}

/**
 * Handle clicks inside rendered documents: in-page anchors scroll, local links preview the
 * target file, web links open in the browser. The webview itself never navigates.
 */
export function wireLinks(root: HTMLElement, handlers: { openLocal(path: string): void; openExternal(url: string): void }) {
  root.addEventListener('click', (e) => {
    const a = (e.target as Element | null)?.closest?.('a');
    if (!a || !root.contains(a)) return;
    e.preventDefault();
    const local = a.getAttribute('data-local-path');
    const href = a.getAttribute('href') || '';
    if (local) {
      handlers.openLocal(local);
    } else if (href.startsWith('#') && href.length > 1) {
      const id = decodeURIComponent(href.slice(1));
      const target = root.querySelector(`[id="${CSS.escape(id)}"], [name="${CSS.escape(id)}"]`);
      target?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    } else if (isExternalUrl(href)) {
      handlers.openExternal(href);
    }
  });
}
