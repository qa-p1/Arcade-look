// Fixed-row-height virtual scrolling. Renders only visible rows, and compresses the scroll
// range for huge row counts (a 100 GB hex dump has 6 billion rows; browsers cap element
// heights at a few million pixels).
import { h } from './dom';

const MAX_PX = 6_000_000;

export interface VirtualOptions {
  scroller: HTMLElement;
  rowHeight: number;
  count: number;
  render: (index: number) => HTMLElement;
  /** Extra width for horizontally scrollable content (tables). */
  width?: number;
  overscan?: number;
}

export interface Virtual {
  setCount(n: number): void;
  refresh(): void;
  scrollToRow(i: number): void;
  firstVisible(): number;
  destroy(): void;
}

export function virtualList(o: VirtualOptions): Virtual {
  const spacer = h('div.v-spacer');
  const win = h('div.v-window');
  o.scroller.classList.add('v-scroller');
  o.scroller.append(spacer, win);
  let count = o.count;
  let first = -1;
  let last = -1;
  const overscan = o.overscan ?? 8;

  const total = () => count * o.rowHeight;
  const virtualHeight = () => Math.min(total(), MAX_PX);
  /** Map the real scrollTop onto the full (uncompressed) content offset. */
  const contentOffset = () => {
    const vh = virtualHeight();
    const view = o.scroller.clientHeight;
    if (total() <= MAX_PX || vh <= view) return o.scroller.scrollTop;
    return (o.scroller.scrollTop / (vh - view)) * (total() - view);
  };

  function layout() {
    spacer.style.height = `${virtualHeight()}px`;
    if (o.width) spacer.style.width = `${o.width}px`;
  }

  function draw(force = false) {
    const view = o.scroller.clientHeight || 600;
    const off = contentOffset();
    const f = Math.max(0, Math.floor(off / o.rowHeight) - overscan);
    const l = Math.min(count, Math.ceil((off + view) / o.rowHeight) + overscan);
    // Position the window at the current scroll position, shifted by the sub-row offset.
    const shift = f * o.rowHeight - off;
    win.style.transform = `translateY(${o.scroller.scrollTop + shift}px)`;
    if (!force && f === first && l === last) return;
    first = f;
    last = l;
    const frag = document.createDocumentFragment();
    for (let i = f; i < l; i++) {
      const row = o.render(i);
      row.style.height = `${o.rowHeight}px`;
      frag.appendChild(row);
    }
    win.replaceChildren(frag);
  }

  let raf = 0;
  const onScroll = () => {
    if (raf) return;
    raf = requestAnimationFrame(() => {
      raf = 0;
      draw();
    });
  };
  const ro = new ResizeObserver(() => draw(true));
  o.scroller.addEventListener('scroll', onScroll, { passive: true });
  ro.observe(o.scroller);
  layout();
  draw(true);

  return {
    setCount(n) {
      count = n;
      layout();
      draw(true);
    },
    refresh() {
      draw(true);
    },
    scrollToRow(i) {
      const view = o.scroller.clientHeight;
      const target = i * o.rowHeight;
      if (total() <= MAX_PX) o.scroller.scrollTop = target;
      else o.scroller.scrollTop = (target / Math.max(1, total() - view)) * (virtualHeight() - view);
    },
    firstVisible() {
      return Math.floor(contentOffset() / o.rowHeight);
    },
    destroy() {
      ro.disconnect();
      o.scroller.removeEventListener('scroll', onScroll);
      cancelAnimationFrame(raf);
    },
  };
}
