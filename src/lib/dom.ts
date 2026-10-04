// A tiny hyperscript helper: h('div.card#id', { onclick }, child, 'text').

type Child = Node | string | number | null | undefined | false | Child[];
type Props = Record<string, unknown> & { style?: Partial<CSSStyleDeclaration> | string };

export function h<K extends keyof HTMLElementTagNameMap>(
  sel: K | `${K}.${string}` | `${K}#${string}`,
  props?: Props | Child,
  ...children: Child[]
): HTMLElementTagNameMap[K];
export function h(sel: string, props?: Props | Child, ...children: Child[]): HTMLElement;
export function h(sel: string, props?: Props | Child, ...children: Child[]): HTMLElement {
  const m = sel.match(/^([a-z0-9-]+)?((?:[.#][\w-]+)*)$/i);
  const tag = m?.[1] || 'div';
  const el = document.createElement(tag);
  for (const part of (m?.[2] || '').match(/[.#][\w-]+/g) || []) {
    if (part[0] === '.') el.classList.add(part.slice(1));
    else el.id = part.slice(1);
  }
  if (props && typeof props === 'object' && !(props instanceof Node) && !Array.isArray(props)) {
    for (const [k, v] of Object.entries(props)) {
      if (v === undefined || v === null || v === false) continue;
      if (k.startsWith('on') && typeof v === 'function') {
        el.addEventListener(k.slice(2).toLowerCase(), v as EventListener);
      } else if (k === 'style') {
        if (typeof v === 'string') el.setAttribute('style', v);
        else Object.assign(el.style, v);
      } else if (k === 'class') {
        el.className += ` ${v}`;
      } else if (k === 'html') {
        el.innerHTML = String(v);
      } else if (k in el && typeof v !== 'string') {
        (el as unknown as Record<string, unknown>)[k] = v;
      } else {
        el.setAttribute(k, v === true ? '' : String(v));
      }
    }
  } else if (props !== undefined) {
    children.unshift(props as Child);
  }
  append(el, children);
  return el;
}

export function append(el: Node, children: Child[]) {
  for (const c of children) {
    if (c === null || c === undefined || c === false) continue;
    if (Array.isArray(c)) append(el, c);
    else el.appendChild(c instanceof Node ? c : document.createTextNode(String(c)));
  }
}

export function clear(el: Element) {
  while (el.firstChild) el.removeChild(el.firstChild);
}

/** Parse a trusted SVG string (our own icons) into an element. */
export function svg(markup: string): SVGElement {
  const t = document.createElement('template');
  t.innerHTML = markup.trim();
  return t.content.firstElementChild as SVGElement;
}

/** Inputs that don't take typing: once clicked they keep focus, but Space/Esc/arrows must still work. */
const NON_TEXT_INPUTS = new Set(['range', 'checkbox', 'radio', 'button', 'submit', 'reset', 'color', 'file', 'image']);

/** True for elements that take typed text (and so own Space, Esc and the arrow keys). */
export function isEditable(t: EventTarget | null): boolean {
  if (!(t instanceof HTMLElement)) return false;
  if (t.isContentEditable || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT') return true;
  return t instanceof HTMLInputElement && !NON_TEXT_INPUTS.has(t.type);
}

/** Run `fn` at most once per animation frame. */
export function rafThrottle<T extends unknown[]>(fn: (...a: T) => void): (...a: T) => void {
  let queued = false;
  let last: T;
  return (...a: T) => {
    last = a;
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => {
      queued = false;
      fn(...last);
    });
  };
}

export function nextFrame(): Promise<void> {
  return new Promise((r) => requestAnimationFrame(() => r()));
}
