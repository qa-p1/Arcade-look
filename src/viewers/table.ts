// CSV/TSV and spreadsheets in a virtualised grid (smooth with 100k+ cells).
import './table.css';
import { api, type TableData } from '../lib/backend';
import { h } from '../lib/dom';
import { virtualList, type Virtual } from '../lib/virtual';
import type { Mounted, ViewCtx } from './types';

const ROW_H = 28;
const NUM_RE = /^-?[\d,.' ]*\d([.,]\d+)?%?$|^-?\d+(\.\d+)?[eE][+-]?\d+$/;

function colName(i: number): string {
  let s = '';
  i++;
  while (i > 0) {
    const m = (i - 1) % 26;
    s = String.fromCharCode(65 + m) + s;
    i = Math.floor((i - 1) / 26);
  }
  return s;
}

export async function mount(host: HTMLElement, ctx: ViewCtx): Promise<Mounted> {
  const { info } = ctx;
  const isCsv = info.kind === 'csv';
  let virt: Virtual | null = null;
  const area = h('div.table-area');
  const tabs = h('div.sheet-tabs');
  host.append(area, tabs);

  async function show(sheet: number) {
    const d = await api.readTable(info.path, info.format, sheet);
    virt?.destroy();
    render(d);
  }

  function render(d: TableData) {
    area.replaceChildren();
    const cols = Math.max(1, d.columns);
    let rows = d.rows;
    let header: string[] | null = null;
    if (isCsv && rows.length > 1) {
      header = rows[0];
      rows = rows.slice(1);
    }
    // Column widths and alignment from a sample.
    const sample = rows.slice(0, 400);
    const widths: number[] = [];
    const numeric: boolean[] = [];
    for (let c = 0; c < cols; c++) {
      let max = header?.[c]?.length ?? 3;
      let nums = 0;
      let filled = 0;
      for (const r of sample) {
        const v = r[c] ?? '';
        if (v) {
          filled++;
          if (NUM_RE.test(v.trim())) nums++;
        }
        max = Math.max(max, Math.min(v.length, 60));
      }
      widths.push(Math.min(380, Math.max(64, Math.round(max * 7.4 + 24))));
      numeric.push(filled > 0 && nums / filled > 0.8);
    }
    const rnW = Math.max(46, String(rows.length).length * 8 + 22);
    const template = `${rnW}px ${widths.map((w) => `${w}px`).join(' ')}`;
    const totalW = rnW + widths.reduce((a, b) => a + b, 0);

    const grid = h('div.grid', { style: `--cols: ${template}` });
    const headRow = h('div.tr.th-row', { style: `width:${totalW}px` }, h('div.td.rn.corner'));
    for (let c = 0; c < cols; c++) {
      const label = header ? header[c] ?? '' : colName(c);
      headRow.append(h(`div.td.th${numeric[c] ? '.num' : ''}`, { title: label }, label));
    }
    const headWrap = h('div.thead', headRow);
    const scroller = h('div.tbody.selectable');
    grid.append(headWrap, scroller);
    area.append(grid);

    scroller.addEventListener('scroll', () => {
      headRow.style.transform = `translateX(${-scroller.scrollLeft}px)`;
    }, { passive: true });

    virt = virtualList({
      scroller,
      rowHeight: ROW_H,
      count: rows.length,
      width: totalW,
      render: (i) => {
        const r = rows[i];
        const row = h('div.tr', { style: `width:${totalW}px` }, h('div.td.rn', String(i + 1)));
        for (let c = 0; c < cols; c++) {
          const v = r[c] ?? '';
          const cell = h(`div.td${numeric[c] ? '.num' : ''}`, v);
          if (v.length > 30) cell.title = v.length > 500 ? `${v.slice(0, 500)}…` : v;
          row.append(cell);
        }
        if (i % 2) row.classList.add('odd');
        return row;
      },
    });

    tabs.replaceChildren();
    tabs.classList.toggle('hidden', d.sheets.length < 2);
    d.sheets.forEach((name, i) => {
      const b = h(`button.sheet-tab${i === d.sheet ? '.active' : ''}`, name);
      b.addEventListener('click', () => i !== d.sheet && void show(i));
      tabs.append(b);
    });

    const more = d.truncatedRows ? ' (first 10,000)' : '';
    ctx.setStatus(`${rows.length.toLocaleString()} rows${more}  ·  ${cols} ${cols === 1 ? 'column' : 'columns'}`);
    ctx.setDetails([
      ['Rows', `${rows.length.toLocaleString()}${more}`],
      ['Columns', `${cols}${d.truncatedCols ? ' (first 256)' : ''}`],
      ['Sheets', d.sheets.length > 1 ? String(d.sheets.length) : ''],
      ['Sheet', d.sheets[d.sheet] ?? ''],
      ['Delimiter', d.delimiter ?? ''],
      ['Encoding', d.encoding ?? ''],
    ]);
  }

  await show(0);
  return {
    dispose: () => virt?.destroy(),
  };
}
