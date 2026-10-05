// PDF via pdf.js (legacy build for older WebKit). Pages render lazily as they scroll into
// view, far-away pages are released, and text is selectable.
import './pdf.css';
import workerUrl from 'pdfjs-dist/legacy/build/pdf.worker.min.mjs?url';
import type { PDFDocumentProxy, PDFPageProxy, RenderTask } from 'pdfjs-dist';
import { h } from '../lib/dom';
import { fileUrl } from '../lib/urls';
import type { Mounted, ViewCtx } from './types';

const MAX_LIVE_PAGES = 14;

// One worker for every PDF: spawning a worker per document grows memory with each preview.
let worker: import('pdfjs-dist').PDFWorker | null = null;

export async function mount(host: HTMLElement, ctx: ViewCtx): Promise<Mounted> {
  const pdfjs = await import('pdfjs-dist/legacy/build/pdf.mjs');
  pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;
  worker ??= new pdfjs.PDFWorker();
  const assets = new URL('vendor/pdfjs/', document.baseURI).href;
  const task = pdfjs.getDocument({
    url: fileUrl(ctx.info.path),
    worker,
    cMapUrl: `${assets}cmaps/`,
    cMapPacked: true,
    standardFontDataUrl: `${assets}standard_fonts/`,
    wasmUrl: `${assets}wasm/`,
    enableXfa: false,
    disableAutoFetch: true,
  });
  const onAbort = () => void task.destroy();
  ctx.signal.addEventListener('abort', onAbort);
  let doc: PDFDocumentProxy;
  try {
    doc = await task.promise;
  } catch (e) {
    void task.destroy();
    const name = (e as { name?: string })?.name;
    if (name === 'PasswordException') throw new Error('This PDF is password-protected.');
    if (name === 'InvalidPDFException') throw new Error('This file is not a valid PDF.');
    throw e;
  } finally {
    ctx.signal.removeEventListener('abort', onAbort);
  }

  const n = doc.numPages;
  const first = await doc.getPage(1);
  const base = first.getViewport({ scale: 1 });
  const scroller = h('div.pdf-scroll');
  const pagesEl = h('div.pdf-pages');
  scroller.append(pagesEl);
  host.append(scroller);

  type PageSlot = { el: HTMLElement; w: number; h: number; page?: PDFPageProxy; task?: RenderTask; rendered: number; text?: HTMLElement };
  const slots: PageSlot[] = [];
  for (let i = 0; i < n; i++) {
    const el = h('div.pdf-page', { 'data-n': String(i + 1) });
    slots.push({ el, w: base.width, h: base.height, rendered: 0 });
    pagesEl.append(el);
  }

  let mode: 'fit' | 'custom' = 'fit';
  let scale = 1;
  const fitScale = () => Math.max(0.25, Math.min((scroller.clientWidth - 56) / base.width, 1.75));
  const zoomLabel = h('span.tb-label', '100%');
  const pageInput = h('input.tb-input', { type: 'text', value: '1', title: 'Page' }) as HTMLInputElement;
  const pageTotal = h('span.tb-label', `/ ${n}`);

  function layout() {
    for (const s of slots) {
      s.el.style.width = `${Math.floor(s.w * scale)}px`;
      s.el.style.height = `${Math.floor(s.h * scale)}px`;
    }
    zoomLabel.textContent = `${Math.round(scale * 100)}%`;
  }

  const live = new Set<number>();
  let renderGen = 0;

  async function render(i: number) {
    const s = slots[i];
    if (s.rendered === renderGen) return;
    s.rendered = renderGen;
    const myGen = renderGen;
    try {
      s.page ??= await doc.getPage(i + 1);
      const page = s.page;
      const vp1 = page.getViewport({ scale: 1 });
      if (vp1.width !== s.w || vp1.height !== s.h) {
        s.w = vp1.width;
        s.h = vp1.height;
        s.el.style.width = `${Math.floor(s.w * scale)}px`;
        s.el.style.height = `${Math.floor(s.h * scale)}px`;
      }
      const dpr = Math.min(devicePixelRatio || 1, 3);
      const vp = page.getViewport({ scale });
      const canvas = document.createElement('canvas');
      canvas.width = Math.floor(vp.width * dpr);
      canvas.height = Math.floor(vp.height * dpr);
      canvas.style.width = `${Math.floor(vp.width)}px`;
      canvas.style.height = `${Math.floor(vp.height)}px`;
      s.task?.cancel();
      s.task = page.render({
        canvas,
        canvasContext: canvas.getContext('2d', { alpha: false })!,
        viewport: vp,
        transform: dpr !== 1 ? [dpr, 0, 0, dpr, 0, 0] : undefined,
      });
      await s.task.promise;
      if (myGen !== renderGen || ctx.signal.aborted) return;
      const text = h('div.textLayer');
      s.el.style.setProperty('--scale-factor', String(scale));
      s.el.style.setProperty('--total-scale-factor', String(scale));
      s.el.style.setProperty('--user-unit', '1');
      s.el.replaceChildren(canvas, text);
      s.text = text;
      live.add(i);
      evict(i);
      try {
        await new pdfjs.TextLayer({ textContentSource: page.streamTextContent(), container: text, viewport: vp }).render();
      } catch {
        /* text selection is a bonus */
      }
    } catch (e) {
      const name = (e as { name?: string })?.name;
      if (name !== 'RenderingCancelledException') {
        s.rendered = 0;
        s.el.replaceChildren(h('div.pdf-page-error', 'This page could not be rendered.'));
      }
    }
  }

  function release(i: number) {
    const s = slots[i];
    s.task?.cancel();
    s.task = undefined;
    s.el.replaceChildren();
    s.rendered = 0;
    live.delete(i);
  }

  function evict(center: number) {
    if (live.size <= MAX_LIVE_PAGES) return;
    const far = [...live].sort((a, b) => Math.abs(b - center) - Math.abs(a - center));
    for (const i of far.slice(0, live.size - MAX_LIVE_PAGES)) release(i);
  }

  const visible = new Set<number>();
  const io = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        const i = Number((e.target as HTMLElement).dataset.n) - 1;
        if (e.isIntersecting) {
          visible.add(i);
          void render(i);
        } else {
          visible.delete(i);
        }
      }
    },
    { root: scroller, rootMargin: '800px 0px' },
  );
  for (const s of slots) io.observe(s.el);

  function currentPage(): number {
    const mid = scroller.scrollTop + scroller.clientHeight / 3;
    let lo = 0;
    let hi = n - 1;
    while (lo < hi) {
      const m = (lo + hi + 1) >> 1;
      if (slots[m].el.offsetTop <= mid) lo = m;
      else hi = m - 1;
    }
    return lo;
  }
  let lastPage = -1;
  const onScroll = () => {
    const p = currentPage();
    if (p !== lastPage) {
      lastPage = p;
      if (document.activeElement !== pageInput) pageInput.value = String(p + 1);
    }
  };
  scroller.addEventListener('scroll', onScroll, { passive: true });

  function goTo(p: number) {
    const i = Math.min(n - 1, Math.max(0, p));
    scroller.scrollTop = slots[i].el.offsetTop - 16;
  }
  pageInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      goTo(parseInt(pageInput.value, 10) - 1);
      pageInput.blur();
    }
    e.stopPropagation();
  });

  function setScale(next: number, keepMode = false) {
    const anchor = currentPage();
    const within = (scroller.scrollTop - slots[anchor].el.offsetTop) / Math.max(1, slots[anchor].el.offsetHeight);
    scale = Math.min(5, Math.max(0.2, next));
    if (!keepMode) mode = 'custom';
    renderGen++;
    for (const i of [...live]) slots[i].rendered = 0;
    layout();
    scroller.scrollTop = slots[anchor].el.offsetTop + within * slots[anchor].el.offsetHeight;
    for (const i of visible) void render(i);
  }

  scroller.addEventListener(
    'wheel',
    (e) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      setScale(scale * Math.exp(-e.deltaY * 0.01));
    },
    { passive: false },
  );

  let resizeT = 0;
  const ro = new ResizeObserver(() => {
    if (mode !== 'fit') return;
    clearTimeout(resizeT);
    resizeT = window.setTimeout(() => setScale(fitScale(), true), 80);
  });
  ro.observe(scroller);

  scale = fitScale();
  renderGen = 1;
  layout();
  await render(0);

  ctx.toolbar([
    { icon: 'left', title: 'Previous page', onClick: () => goTo(currentPage() - 1) },
    { title: 'Page', el: pageInput },
    { title: 'Pages', el: pageTotal },
    { icon: 'right', title: 'Next page', onClick: () => goTo(currentPage() + 1) },
    { separator: true, title: '' },
    { icon: 'zoomOut', title: 'Zoom out (−)', onClick: () => setScale(scale / 1.2) },
    { title: 'Zoom', el: zoomLabel },
    { icon: 'zoomIn', title: 'Zoom in (+)', onClick: () => setScale(scale * 1.2) },
    { icon: 'fit', title: 'Fit width (0)', onClick: () => { mode = 'fit'; setScale(fitScale(), true); } },
  ]);

  ctx.setStatus(`${n.toLocaleString()} ${n === 1 ? 'page' : 'pages'}`);
  void doc.getMetadata().then((m) => {
    const i = (m.info ?? {}) as Record<string, unknown>;
    const s = (k: string) => (typeof i[k] === 'string' ? (i[k] as string).trim() : '');
    const mm = (pt: number) => Math.round((pt / 72) * 25.4);
    ctx.setDetails([
      ['Title', s('Title')], ['Author', s('Author')], ['Subject', s('Subject')],
      ['Pages', String(n)],
      ['Page size', `${mm(base.width)} × ${mm(base.height)} mm`],
      ['Creator', s('Creator')], ['Producer', s('Producer')],
      ['Created', pdfDate(s('CreationDate'))], ['Modified', pdfDate(s('ModDate'))],
      ['PDF version', s('PDFFormatVersion')],
      ['Encrypted', i.IsEncrypted ? 'Yes' : ''],
    ]);
  }).catch(() => {});

  return {
    async capturePage() {
      if (ctx.signal.aborted) throw new Error('This PDF is no longer open.');
      const page = await doc.getPage(currentPage() + 1);
      const base = page.getViewport({ scale: 1 });
      const vp = page.getViewport({ scale: Math.min(2, 4096 / Math.max(base.width, base.height)) });
      const canvas = document.createElement('canvas');
      canvas.width = Math.max(1, Math.floor(vp.width));
      canvas.height = Math.max(1, Math.floor(vp.height));
      const rendering = page.render({ canvas, canvasContext: canvas.getContext('2d', { alpha: false })!, viewport: vp });
      const cancel = () => rendering.cancel();
      ctx.signal.addEventListener('abort', cancel, { once: true });
      try {
        await rendering.promise;
        if (ctx.signal.aborted) throw new Error('This PDF is no longer open.');
        const blob = await new Promise<Blob>((resolve, reject) => canvas.toBlob(
          (blob) => blob ? resolve(blob) : reject(new Error('This PDF page could not be rendered.')), 'image/png'));
        if (blob.size > 16 * 1024 * 1024) throw new Error('This PDF page is too large to analyze.');
        return new Uint8Array(await blob.arrayBuffer());
      } finally {
        ctx.signal.removeEventListener('abort', cancel);
        canvas.width = canvas.height = 0;
      }
    },
    keydown(e) {
      if (e.ctrlKey || e.metaKey || e.altKey) return false;
      switch (e.key) {
        case '+': case '=': setScale(scale * 1.2); return true;
        case '-': case '_': setScale(scale / 1.2); return true;
        case '0': mode = 'fit'; setScale(fitScale(), true); return true;
        case 'Home': goTo(0); return true;
        case 'End': goTo(n - 1); return true;
      }
      return false;
    },
    dispose() {
      io.disconnect();
      ro.disconnect();
      for (const s of slots) s.task?.cancel();
      void task.destroy();
    },
  };
}

function pdfDate(s: string): string {
  const m = s.match(/^D:(\d{4})(\d{2})?(\d{2})?(\d{2})?(\d{2})?/);
  if (!m) return s;
  const [, y, mo = '01', d = '01', hh = '00', mi = '00'] = m;
  try {
    return new Date(`${y}-${mo}-${d}T${hh}:${mi}:00`).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
  } catch {
    return s;
  }
}
