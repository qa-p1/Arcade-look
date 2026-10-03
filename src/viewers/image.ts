import './image.css';
import { api } from '../lib/backend';
import { h } from '../lib/dom';
import { fileUrl, imageUrl } from '../lib/urls';
import type { Mounted, ViewCtx } from './types';

const ALPHA_FORMATS = new Set(['png', 'apng', 'gif', 'webp', 'svg', 'avif', 'ico', 'cur', 'tga', 'psd', 'psb', 'tiff', 'exr', 'qoi', 'dds', 'bmp', 'jxl', 'heic', 'heif']);

function loadImage(img: HTMLImageElement, src: string, signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    const done = () => {
      img.onload = img.onerror = null;
      signal.removeEventListener('abort', abort);
    };
    const abort = () => {
      done();
      reject(new DOMException('aborted', 'AbortError'));
    };
    img.onload = () => {
      done();
      // decode() keeps the first paint jank-free for big images.
      img.decode().then(resolve, resolve);
    };
    img.onerror = () => {
      done();
      reject(new Error('decode failed'));
    };
    signal.addEventListener('abort', abort);
    img.src = src;
  });
}

export async function mount(host: HTMLElement, ctx: ViewCtx): Promise<Mounted> {
  const { info, signal } = ctx;
  const max = Math.min(8192, Math.round(Math.max(screen.width, screen.height) * (devicePixelRatio || 1) * 1.5));
  const sources: string[] = [];
  switch (info.kind) {
    case 'image-decode': sources.push(imageUrl(info.path, 'decode', max)); break;
    case 'image-raw': sources.push(imageUrl(info.path, 'raw', max)); break;
    case 'image-psd': sources.push(imageUrl(info.path, 'psd', max)); break;
    default:
      sources.push(fileUrl(info.path));
      if (info.kind !== 'svg') sources.push(imageUrl(info.path, 'decode', max));
  }

  const img = new Image();
  img.draggable = false;
  img.alt = info.name;
  let ok = false;
  for (const src of sources) {
    try {
      await loadImage(img, src, signal);
      ok = true;
      break;
    } catch (e) {
      if (signal.aborted) throw e;
    }
  }
  if (!ok) {
    throw new Error(
      info.kind === 'image-raw'
        ? 'No embedded preview was found in this RAW file.'
        : `This ${info.format.toUpperCase()} image can't be decoded on this system.`,
    );
  }

  let natW = img.naturalWidth || 512;
  let natH = img.naturalHeight || 512;
  if (info.kind === 'svg' && (!img.naturalWidth || !img.naturalHeight)) {
    img.style.width = '512px';
    img.style.height = '512px';
  }
  const tiny = natW <= 128 && natH <= 128;
  const stage = h('div.img-stage');
  const alpha = ALPHA_FORMATS.has(info.format) || ALPHA_FORMATS.has(info.ext);
  let checker = alpha;
  stage.classList.toggle('checker', checker);
  stage.append(img);
  host.append(stage);

  // ---------------------------------------------------------------- view state
  let scale = 1;
  let tx = 0;
  let ty = 0;
  let rot = 0;
  let fitted = true;

  const rotated = () => rot % 180 !== 0;
  const fitScale = () => {
    const w = stage.clientWidth - 32;
    const hgt = stage.clientHeight - 32;
    const [iw, ih] = rotated() ? [natH, natW] : [natW, natH];
    const s = Math.min(w / iw, hgt / ih);
    return Math.max(0.01, tiny ? Math.min(s, 8) : Math.min(s, 1));
  };
  const zoomLabel = h('span.tb-label', '100%');

  function apply(animate = false) {
    img.classList.toggle('animate', animate);
    img.style.transform = `translate(-50%, -50%) translate(${tx}px, ${ty}px) rotate(${rot}deg) scale(${scale})`;
    img.classList.toggle('pixelated', scale >= 3 || (tiny && scale > 1));
    stage.classList.toggle('pannable', !fitted);
    zoomLabel.textContent = `${Math.round(scale * 100)}%`;
  }

  function fit(animate = false) {
    scale = fitScale();
    tx = ty = 0;
    fitted = true;
    apply(animate);
  }

  function zoomTo(next: number, cx?: number, cy?: number, animate = false) {
    const r = stage.getBoundingClientRect();
    const px = (cx ?? r.left + r.width / 2) - (r.left + r.width / 2);
    const py = (cy ?? r.top + r.height / 2) - (r.top + r.height / 2);
    next = Math.min(64, Math.max(0.02, next));
    const k = next / scale;
    // Keep the point under the cursor fixed.
    tx = px - (px - tx) * k;
    ty = py - (py - ty) * k;
    scale = next;
    fitted = false;
    if (Math.abs(scale - fitScale()) < 0.001) fit(animate);
    else apply(animate);
  }

  stage.addEventListener(
    'wheel',
    (e) => {
      e.preventDefault();
      const factor = Math.exp(-e.deltaY * (e.ctrlKey ? 0.01 : 0.0018));
      zoomTo(scale * factor, e.clientX, e.clientY);
    },
    { passive: false },
  );

  let drag: { x: number; y: number; tx: number; ty: number } | null = null;
  stage.addEventListener('pointerdown', (e) => {
    if (e.button !== 0 || fitted) return;
    drag = { x: e.clientX, y: e.clientY, tx, ty };
    stage.setPointerCapture(e.pointerId);
    stage.classList.add('dragging');
  });
  stage.addEventListener('pointermove', (e) => {
    if (!drag) return;
    tx = drag.tx + (e.clientX - drag.x);
    ty = drag.ty + (e.clientY - drag.y);
    apply();
  });
  const endDrag = () => {
    drag = null;
    stage.classList.remove('dragging');
  };
  stage.addEventListener('pointerup', endDrag);
  stage.addEventListener('pointercancel', endDrag);
  stage.addEventListener('dblclick', (e) => {
    if (fitted) zoomTo(Math.max(1, fitScale() * 2), e.clientX, e.clientY, true);
    else fit(true);
  });

  const ro = new ResizeObserver(() => (fitted ? fit() : apply()));
  ro.observe(stage);

  ctx.toolbar([
    { icon: 'zoomOut', title: 'Zoom out (−)', onClick: () => zoomTo(scale / 1.25, undefined, undefined, true) },
    { title: 'Zoom', el: zoomLabel },
    { icon: 'zoomIn', title: 'Zoom in (+)', onClick: () => zoomTo(scale * 1.25, undefined, undefined, true) },
    { separator: true, title: '' },
    { icon: 'fit', title: 'Fit to window (0)', onClick: () => fit(true) },
    { label: '1:1', title: 'Actual size (1)', onClick: () => zoomTo(1, undefined, undefined, true) },
    { icon: 'rotate', title: 'Rotate (R)', onClick: () => rotate() },
    ...(alpha
      ? [{ icon: 'grid', title: 'Transparency checkerboard', active: checker, onClick: (b: HTMLButtonElement) => {
          checker = !checker;
          stage.classList.toggle('checker', checker);
          b.classList.toggle('active', checker);
        } }]
      : []),
  ]);

  function rotate() {
    rot = (rot + 90) % 360;
    if (fitted) fit(true);
    else apply(true);
  }

  fit();
  ctx.setStatus(`${natW.toLocaleString()} × ${natH.toLocaleString()}`);

  // Metadata off the critical path.
  void api
    .imageInfo(info.path, info.format)
    .then((ii) => {
      if (ii.width && ii.height && (info.kind === 'image-raw' || info.kind === 'image-psd')) {
        ctx.setStatus(`${ii.width.toLocaleString()} × ${ii.height.toLocaleString()}`);
        natW = natW || ii.width;
        natH = natH || ii.height;
      }
      const mp = ((ii.width ?? natW) * (ii.height ?? natH)) / 1e6;
      ctx.setDetails([
        ['Dimensions', `${(ii.width ?? natW).toLocaleString()} × ${(ii.height ?? natH).toLocaleString()} px`],
        ['Megapixels', mp >= 0.1 ? mp.toFixed(1) : ''],
        ...ii.exif,
      ]);
    })
    .catch(() => ctx.setDetails([['Dimensions', `${natW} × ${natH} px`]]));

  return {
    keydown(e) {
      if (e.ctrlKey || e.metaKey || e.altKey) return false;
      switch (e.key) {
        case '+': case '=': zoomTo(scale * 1.25, undefined, undefined, true); return true;
        case '-': case '_': zoomTo(scale / 1.25, undefined, undefined, true); return true;
        case '0': fit(true); return true;
        case '1': zoomTo(1, undefined, undefined, true); return true;
        case 'r': case 'R': rotate(); return true;
      }
      return false;
    },
    dispose() {
      ro.disconnect();
      img.removeAttribute('src');
    },
  };
}
