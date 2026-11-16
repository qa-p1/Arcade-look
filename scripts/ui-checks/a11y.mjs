// Accessibility check (docs/v1/DECISIONS.md D17; modelled on Arcade Box's tests/a11y.mjs, its D21). Against the built UI
// with the native IPC fixture (mock.js), in Chromium, in light and dark:
//   - axe-core's WCAG 2.1 A and AA rules (contrast, names, roles, labels) on the preview window for each viewer
//     (image, code, Markdown, JSON, table, PDF, archive, font, audio, video, and the info card) with the Info panel
//     closed and open, on the shortcuts overlay ("?") and on every Settings section;
//   - every control Tab reaches on each of those surfaces shows a visible focus indicator;
//   - with reduced motion requested, nothing animates or transitions for longer than 10 ms.
// Run after `npm run build`: node scripts/ui-checks/a11y.mjs (PLAYWRIGHT_CHROMIUM may name a Chromium).
import { createServer } from 'node:http';
import { readFileSync, existsSync, readdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve, extname, join } from 'node:path';
import { deflateSync } from 'node:zlib';
import { chromium } from 'playwright';

const root = resolve(new URL('../..', import.meta.url).pathname);
const dist = join(root, 'dist');
const axeSource = readFileSync(createRequire(import.meta.url).resolve('axe-core/axe.min.js'), 'utf8');
const mock = readFileSync(join(root, 'scripts/ui-checks/mock.js'), 'utf8');
const failures = [];

// ---------------------------------------------------------------- fixture files (served at /f/<path>, as the app's
// own protocol does)
function crc32(buf) {
  let c, crc = ~0;
  for (const byte of buf) {
    c = (crc ^ byte) & 0xff;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    crc = (crc >>> 8) ^ c;
  }
  return ~crc >>> 0;
}
function png(width, height) {
  const raw = Buffer.alloc((width * 4 + 1) * height);
  for (let y = 0; y < height; y++) {
    raw[y * (width * 4 + 1)] = 0;
    for (let x = 0; x < width; x++) raw.set([x * 4, y * 5, 160, 255], y * (width * 4 + 1) + 1 + x * 4);
  }
  const chunk = (type, data) => {
    const body = Buffer.concat([Buffer.from(type), data]);
    const head = Buffer.alloc(4); head.writeUInt32BE(data.length);
    const tail = Buffer.alloc(4); tail.writeUInt32BE(crc32(body));
    return Buffer.concat([head, body, tail]);
  };
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(width, 0); ihdr.writeUInt32BE(height, 4); ihdr.set([8, 6, 0, 0, 0], 8);
  return Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk('IHDR', ihdr), chunk('IDAT', deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}
function wav(seconds = 1, rate = 8000) {
  const n = seconds * rate, data = Buffer.alloc(n * 2), head = Buffer.alloc(44);
  for (let i = 0; i < n; i++) data.writeInt16LE(Math.round(Math.sin((i / rate) * 2 * Math.PI * 440) * 12000), i * 2);
  head.write('RIFF', 0); head.writeUInt32LE(36 + data.length, 4); head.write('WAVEfmt ', 8); head.writeUInt32LE(16, 16);
  head.writeUInt16LE(1, 20); head.writeUInt16LE(1, 22); head.writeUInt32LE(rate, 24); head.writeUInt32LE(rate * 2, 28);
  head.writeUInt16LE(2, 32); head.writeUInt16LE(16, 34); head.write('data', 36); head.writeUInt32LE(data.length, 40);
  return Buffer.concat([head, data]);
}
function pdf() {
  const content = 'BT /F1 24 Tf 72 700 Td (Accessibility fixture) Tj ET';
  const objects = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>',
    `<< /Length ${content.length} >>\nstream\n${content}\nendstream`,
    '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
  ];
  let out = '%PDF-1.4\n';
  const offsets = objects.map((body, i) => { const at = out.length; out += `${i + 1} 0 obj\n${body}\nendobj\n`; return at; });
  const xref = out.length;
  out += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n${offsets.map((o) => `${String(o).padStart(10, '0')} 00000 n \n`).join('')}`;
  out += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return Buffer.from(out, 'latin1');
}
const fontFile = join(dist, 'assets', readdirSync(join(dist, 'assets')).find((name) => /^InterVariable-.*\.woff2$/.test(name)));
// A short WebM is recorded by the browser itself, so the check needs no media tools.
let webm = Buffer.alloc(0);

const fixtureBytes = {
  png: () => png(64, 48),
  wav: () => wav(),
  pdf: () => pdf(),
  woff2: () => readFileSync(fontFile),
  webm: () => webm,
};
const fixtureTypes = { png: 'image/png', wav: 'audio/wav', pdf: 'application/pdf', woff2: 'font/woff2', webm: 'video/webm' };

const types = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.woff2': 'font/woff2', '.svg': 'image/svg+xml', '.png': 'image/png', '.wasm': 'application/wasm', '.json': 'application/json' };
const server = createServer((req, res) => {
  const pathname = new URL(req.url, 'http://localhost').pathname;
  const fixture = pathname.startsWith('/f/') ? fixtureBytes[extname(decodeURIComponent(pathname)).slice(1)] : null;
  if (fixture) {
    const body = fixture();
    const range = /^bytes=(\d+)-(\d*)$/.exec(req.headers.range ?? '');
    const type = fixtureTypes[extname(decodeURIComponent(pathname)).slice(1)];
    if (range) {
      const start = Number(range[1]), end = Math.min(body.length - 1, range[2] ? Number(range[2]) : body.length - 1);
      res.writeHead(206, { 'content-type': type, 'accept-ranges': 'bytes', 'content-range': `bytes ${start}-${end}/${body.length}`, 'content-length': end - start + 1 });
      res.end(body.subarray(start, end + 1));
    } else {
      res.writeHead(200, { 'content-type': type, 'accept-ranges': 'bytes', 'content-length': body.length });
      res.end(body);
    }
    return;
  }
  let path = join(dist, pathname);
  if (!existsSync(path) || path.endsWith('/')) path = join(dist, 'index.html');
  res.writeHead(200, { 'content-type': types[extname(path)] ?? 'application/octet-stream' });
  res.end(readFileSync(path));
});
await new Promise((done) => server.listen(0, '127.0.0.1', done));
const url = `http://127.0.0.1:${server.address().port}`;

// ---------------------------------------------------------------- viewers: the file each opens and what the IPC
// fixture answers for it
const sampleCode = `import { open } from './files';\n\n// Opens the first file.\nexport async function main(path: string): Promise<number> {\n  const file = await open(path);\n  return file.size > 0 ? 1 : 0;\n}\n`;
const richMarkdown = '<h1>Getting started</h1><p>Press <kbd>Space</kbd>, see anything. Read the <a href="https://example.com/guide">guide</a>.</p><h2>Keep your place</h2><blockquote><p>A quote with a <code>code span</code>.</p></blockquote><table><thead><tr><th>Key</th><th>Action</th></tr></thead><tbody><tr><td>I</td><td>Info</td></tr><tr><td>A</td><td>Actions</td></tr></tbody></table><ul><li>First</li><li>Second</li></ul><pre><code>arcade-look --settings</code></pre>';
const VIEWERS = [
  { viewer: 'markdown', patch: {} },
  { viewer: 'code', patch: { name: 'main.ts', ext: 'ts', kind: 'code', format: 'typescript', lang: 'typescript', mime: 'text/typescript', size: sampleCode.length } },
  { viewer: 'json', patch: { name: 'data.json', ext: 'json', kind: 'json', format: 'json', mime: 'application/json', size: 60 } },
  { viewer: 'image', patch: { name: 'picture.png', ext: 'png', kind: 'image', format: 'png', mime: 'image/png', size: 4000 } },
  { viewer: 'table', patch: { name: 'table.csv', ext: 'csv', kind: 'csv', format: 'csv', mime: 'text/csv', size: 90 } },
  { viewer: 'pdf', patch: { name: 'paper.pdf', ext: 'pdf', kind: 'pdf', format: 'pdf', mime: 'application/pdf', size: 900 } },
  { viewer: 'archive', patch: { name: 'bundle.zip', ext: 'zip', kind: 'archive', format: 'zip', mime: 'application/zip', size: 9000 } },
  { viewer: 'font', patch: { name: 'face.woff2', ext: 'woff2', kind: 'font', format: 'woff2', mime: 'font/woff2', size: 100000 } },
  { viewer: 'audio', patch: { name: 'tone.wav', ext: 'wav', kind: 'audio', format: 'wav', mime: 'audio/wav', size: 16044 } },
  { viewer: 'video', patch: { name: 'clip.webm', ext: 'webm', kind: 'video', format: 'webm', mime: 'video/webm', size: 5000 } },
  { viewer: 'info', patch: { name: 'blob.bin', ext: 'bin', kind: 'binary', format: 'binary', mime: 'application/octet-stream', size: 96 } },
];
const answers = () => {
  const text = { text: '', encoding: 'UTF-8', truncated: false, size: 0, lines: 0 };
  const calls = window.__mock.calls;
  const orig = window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__);
  const table = (rows) => ({ sheets: [], sheet: 0, rows, columns: rows[0].length, truncatedRows: false, truncatedCols: false, delimiter: ',', encoding: 'UTF-8' });
  const fixtures = {
    render_markdown: () => ({ html: window.__richMarkdown, truncated: false }),
    read_text: ({ path }) => path.endsWith('.json')
      ? { ...text, text: '{\n  "name": "look",\n  "tags": ["a", "b"],\n  "nested": { "ok": true, "n": 3 }\n}\n', size: 60, lines: 6 }
      : { ...text, text: window.__sampleCode, size: window.__sampleCode.length, lines: 7 },
    read_table: () => table([['Name', 'Kind', 'Size'], ['alpha', 'image', '4 KB'], ['beta', 'text', '1 KB'], ['gamma', 'archive', '9 KB']]),
    list_archive: () => ({ format: 'zip', totalSize: 9000, packedSize: 4000, truncated: false, note: null, entries: [
      { path: 'docs/', size: null, packed: null, dir: true, modified: null, encrypted: false },
      { path: 'docs/readme.md', size: 1200, packed: 600, dir: false, modified: 1700000000, encrypted: false },
      { path: 'src/main.ts', size: 7800, packed: 3400, dir: false, modified: 1700000000, encrypted: false },
    ] }),
    font_info: () => ({ family: 'Fixture Sans', subfamily: 'Regular', fullName: 'Fixture Sans Regular', version: '1.0', designer: 'Fixture', manufacturer: null, copyright: null, license: null, glyphs: 200, unitsPerEm: 1000, faces: 1, monospaced: false, variable: true, axes: [{ tag: 'wght', name: 'Weight', min: 100, default: 400, max: 900 }], codepoints: Array.from({ length: 94 }, (_, i) => 33 + i), coverage: 94, format: 'woff2' }),
    audio_info: () => ({ title: 'Tone', artist: 'Fixture', album: 'Fixtures', albumArtist: null, genre: null, year: '2026', track: 1, trackTotal: 1, disc: null, composer: null, comment: null, durationMs: 1000, bitrate: 128000, sampleRate: 8000, channels: 1, bitDepth: 16, hasCover: false }),
    image_info: () => ({ width: 64, height: 48, exif: [['Camera', 'Fixture'], ['Exposure', '1/60 s']] }),
    read_bytes: () => new Uint8Array(Array.from({ length: 96 }, (_, i) => (i * 7) & 255)).buffer,
  };
  window.__TAURI_INTERNALS__.invoke = async (command, args = {}) => {
    if (command in fixtures && !window.__mock.fail.includes(command)) { calls.push([command, args]); return fixtures[command](args); }
    return orig(command, args);
  };
};

// ---------------------------------------------------------------- checks
async function audit(page, label) {
  if (!(await page.evaluate(() => 'axe' in window))) await page.evaluate(axeSource);
  const violations = await page.evaluate(async () => {
    const result = await window.axe.run(document, { runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'] } });
    return result.violations.map((v) => `${v.id} (${v.impact}): ${v.nodes.slice(0, 3).map((n) => n.target.join(' ')).join(' | ')}${v.nodes.length > 3 ? ` +${v.nodes.length - 3}` : ''}`);
  });
  for (const violation of violations) failures.push(`${label}: ${violation}`);
}

// A focused control must look different from the same control unfocused: an outline, a ring (box-shadow), a border or a
// background change, on the control or on the field around it. Compared by blurring and refocusing the same element,
// so a style that only shows on hover does not count.
async function focusRing(page, label) {
  await page.evaluate(() => document.activeElement?.blur());
  const seen = new Set();
  for (let i = 0; i < 40; i++) {
    await page.keyboard.press('Tab');
    const found = await page.evaluate(() => {
      const el = document.activeElement;
      if (!el || el === document.body) return null;
      const look = () => [el, el.parentElement, el.parentElement?.parentElement].filter(Boolean).map((node) => {
        const s = getComputedStyle(node);
        return [s.outlineStyle !== 'none' && parseFloat(s.outlineWidth) > 0 ? `${s.outlineStyle} ${s.outlineWidth} ${s.outlineColor}` : '', s.boxShadow, s.borderColor, s.backgroundColor].join('|');
      }).join(' / ');
      const id = `${el.tagName.toLowerCase()}.${[...el.classList].join('.')}[${el.getAttribute('aria-label') ?? el.textContent?.trim().slice(0, 24) ?? ''}]`;
      const focused = look();
      el.blur();
      const resting = look();
      el.focus();
      return { id, visible: focused !== resting };
    });
    if (!found || seen.has(found.id)) continue;
    seen.add(found.id);
    if (!found.visible) failures.push(`${label}: no visible focus on ${found.id}`);
  }
  if (process.env.A11Y_VERBOSE) console.log(`${label}: ${seen.size} Tab stops: ${[...seen].join(' ')}`);
}

async function motion(page, label) {
  const moving = await page.evaluate(() => [...document.querySelectorAll('body *')].filter((el) => {
    const s = getComputedStyle(el);
    const longest = (value) => Math.max(...value.split(',').map((t) => parseFloat(t) * (t.trim().endsWith('ms') ? 1 : 1000)));
    return (s.animationName !== 'none' && longest(s.animationDuration) > 10) || longest(s.transitionDuration) > 10;
  }).slice(0, 5).map((el) => `${el.tagName.toLowerCase()}.${[...el.classList].join('.')}`));
  if (moving.length) failures.push(`${label} (reduced motion): still animates ${moving.join(', ')}`);
}

// ---------------------------------------------------------------- run
const browser = await chromium.launch({ headless: true, executablePath: process.env.PLAYWRIGHT_CHROMIUM || undefined, args: ['--no-sandbox', '--autoplay-policy=no-user-gesture-required'] });
let surfaces = 0;
try {
  {
    const page = await browser.newPage();
    page.setDefaultTimeout(15000);
    await page.goto(url);
    const base64 = await page.evaluate(async () => {
      const canvas = document.createElement('canvas');
      canvas.width = 160; canvas.height = 90;
      const g = canvas.getContext('2d');
      const recorder = new MediaRecorder(canvas.captureStream(15), { mimeType: 'video/webm;codecs=vp8' });
      const chunks = [];
      recorder.ondataavailable = (e) => chunks.push(e.data);
      const stopped = new Promise((done) => { recorder.onstop = done; });
      recorder.start();
      for (let i = 0; i < 12; i++) { g.fillStyle = `hsl(${i * 30} 70% 50%)`; g.fillRect(0, 0, 160, 90); await new Promise((r) => setTimeout(r, 80)); }
      recorder.stop();
      await stopped;
      const bytes = new Uint8Array(await new Blob(chunks).arrayBuffer());
      let text = '';
      for (const b of bytes) text += String.fromCharCode(b);
      return btoa(text);
    });
    webm = Buffer.from(base64, 'base64');
    await page.close();
    if (webm.length < 100) failures.push('the video fixture could not be recorded');
  }

  for (const theme of ['light', 'dark']) {
    for (const reducedMotion of ['no-preference', 'reduce']) {
      const reduced = reducedMotion === 'reduce';
      const newPage = async (viewport, search = '') => {
        const page = await browser.newPage({ viewport, colorScheme: theme, reducedMotion });
        page.setDefaultTimeout(15000);
        page.on('pageerror', (error) => failures.push(`${theme}: ${error.message}`));
        await page.addInitScript(mock);
        await page.addInitScript(`window.__sampleCode = ${JSON.stringify(sampleCode)}; window.__richMarkdown = ${JSON.stringify(richMarkdown)}; (${answers.toString()})(); window.__mock.config.theme = ${JSON.stringify(theme)};`);
        await page.goto(url + search);
        return page;
      };
      const check = async (page, label, { focus = true } = {}) => {
        surfaces++;
        await page.waitForTimeout(250);
        if (reduced) await motion(page, `${label} ${theme}`);
        else {
          await audit(page, `${label} ${theme}`);
          if (focus) await focusRing(page, `${label} ${theme}`);
        }
      };

      // The preview window, each viewer, Info closed and open.
      const page = await newPage({ width: 1040, height: 720 });
      await page.getByRole('heading', { name: 'Getting started', exact: true }).waitFor();
      for (const { viewer, patch } of VIEWERS) {
        await page.evaluate((p) => { window.__mock.inspectPatch = p; window.__mock.send('open', '/fixtures/' + (p.name ?? 'start.md')); }, patch);
        try {
          await page.waitForFunction((v) => document.querySelector(`[data-viewer="${v}"]`)?.children.length > 0, viewer, { timeout: 8000 });
        } catch {
          failures.push(`${viewer} ${theme}: the viewer did not mount`);
          continue;
        }
        await page.waitForTimeout(600);
        const mounted = await page.evaluate(() => document.querySelector('[data-viewer]')?.dataset.viewer);
        if (mounted !== viewer) { failures.push(`${viewer} ${theme}: the preview fell back to "${mounted}"`); continue; }
        await page.evaluate(() => document.activeElement?.blur());
        await check(page, `Preview ${viewer} (Info closed)`);
        await page.keyboard.press('i');
        await page.waitForTimeout(300);
        await check(page, `Preview ${viewer} (Info open)`);
        await page.keyboard.press('i');
        await page.waitForTimeout(150);
      }
      // The shortcuts overlay.
      await page.evaluate(() => { window.__mock.inspectPatch = {}; window.__mock.send('open', '/fixtures/start.md'); });
      await page.getByRole('heading', { name: 'Getting started', exact: true }).waitFor();
      await page.evaluate(() => document.activeElement?.blur());
      await page.keyboard.press('?');
      await page.locator('.dialog.help').waitFor();
      await check(page, 'Shortcuts overlay');
      await page.close();

      // Settings, every section.
      const settings = await newPage({ width: 800, height: 520 }, '/?settings');
      await settings.getByRole('tab', { name: 'General', exact: true }).waitFor();
      for (const section of ['General', 'Shortcuts', 'Previews', 'File manager', 'Plugins', 'Connected apps', 'About']) {
        await settings.getByRole('tab', { name: section, exact: true }).click();
        await settings.waitForFunction(() => !document.querySelector('.skeleton'));
        await check(settings, `Settings ${section}`);
      }
      await settings.close();
    }
  }
} finally {
  await browser.close();
  server.close();
}
if (failures.length) {
  console.error(`Accessibility check failed (${new Set(failures).size}):\n  ${[...new Set(failures)].join('\n  ')}`);
  process.exit(1);
}
console.log(`Accessibility check: WCAG 2.1 A/AA (axe-core) on ${surfaces / 4} surfaces (every viewer with Info closed and open, the shortcuts overlay, every Settings section), visible focus on every Tab stop, no motion when reduced, in both themes`);
