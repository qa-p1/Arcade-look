// Behaviour lock for the preview window: the Escape ladder, the help dialog and the error toast, run against the
// built UI with the native IPC fixture (mock.js). A test marked `known` pins an audit finding that is not fixed yet:
// it must fail, and a known test that passes fails the run, so the marker is removed with the fix.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFileSync, existsSync } from 'node:fs';
import { resolve, extname, join } from 'node:path';
import { chromium } from 'playwright';

const root = resolve(new URL('../..', import.meta.url).pathname);
const dist = join(root, 'dist');
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.woff2': 'font/woff2', '.svg': 'image/svg+xml', '.png': 'image/png' };
const server = createServer((req, res) => {
  let path = join(dist, new URL(req.url, 'http://localhost').pathname);
  if (!existsSync(path) || path.endsWith('/')) path = join(dist, 'index.html');
  res.writeHead(200, { 'content-type': types[extname(path)] ?? 'application/octet-stream' });
  res.end(readFileSync(path));
});
await new Promise((done) => server.listen(0, '127.0.0.1', done));
const url = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({ headless: true, executablePath: process.env.PLAYWRIGHT_CHROMIUM || undefined, args: ['--no-sandbox'] });
const mock = readFileSync(join(root, 'scripts/ui-checks/mock.js'), 'utf8');

const offer = { app: 'arcade.box', action: 'pipeline', title: 'Open in Box', reason: null, outbound: false, pdfPage: false, pipeline: null };

async function open(page, { actions = [], linkAvailable = true, viewport } = {}) {
  if (viewport) await page.setViewportSize(viewport);
  await page.addInitScript(mock);
  await page.addInitScript(([a, l]) => { window.__mock.actions = a; window.__mock.linkAvailable = l; }, [actions, linkAvailable]);
  await page.goto(url);
  await page.getByRole('heading', { name: 'Getting started', exact: true }).waitFor();
  await page.evaluate(() => document.activeElement?.blur());
}
const hidden = (page) => page.evaluate(() => window.__mock.calls.filter(([name]) => name === 'hide_window').length);
const infoOpen = (page) => page.evaluate(() => document.querySelector('.app, #app, body > div')?.classList.contains('info-open') || document.querySelector('.info-open') !== null);
const menuOpen = (page) => page.locator('.arcade-menu').isVisible();
async function settle(page) { await page.waitForTimeout(150); }

const tests = [];
const test = (name, fn, known = null) => tests.push({ name, fn, known });

test('Escape closes the preview from the plain view', async (page) => {
  await open(page);
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => window.__mock.calls.some(([n]) => n === 'hide_window'));
});

test('Space closes the preview', async (page) => {
  await open(page);
  await page.keyboard.press('Space');
  await page.waitForFunction(() => window.__mock.calls.some(([n]) => n === 'hide_window'));
});

test('Escape closes the help dialog and keeps the preview', async (page) => {
  await open(page);
  await page.keyboard.press('?');
  await page.locator('.dialog.help').waitFor();
  await page.keyboard.press('Escape');
  await settle(page);
  assert.equal(await page.locator('.dialog.help').count(), 0);
  assert.equal(await hidden(page), 0, 'Escape in the help dialog closed the preview');
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => window.__mock.calls.some(([n]) => n === 'hide_window'));
});

test('Escape closes the actions menu and keeps the preview', async (page) => {
  await open(page, { actions: [offer] });
  await page.keyboard.press('a');
  await page.locator('.arcade-menu').waitFor({ state: 'visible' });
  await page.keyboard.press('Escape');
  await settle(page);
  assert.equal(await menuOpen(page), false);
  assert.equal(await hidden(page), 0, 'Escape in the menu closed the preview');
});

test('RD-LOOK-1: Escape after the actions menu closed still steps back', async (page) => {
  await open(page, { actions: [offer] });
  await page.keyboard.press('a');
  await page.locator('.arcade-menu').waitFor({ state: 'visible' });
  await page.keyboard.press('Escape');
  await settle(page);
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => window.__mock.calls.some(([n]) => n === 'hide_window'), null, { timeout: 1500 });
}, 'RD-LOOK-1');

test('RD-LOOK-1: fullscreen can be left after the actions menu was used', async (page) => {
  await open(page, { actions: [offer] });
  await page.keyboard.press('a');
  await page.locator('.arcade-menu').waitFor({ state: 'visible' });
  await page.keyboard.press('Escape');
  await page.keyboard.press('f');
  await page.waitForFunction(() => window.__mock.fullscreen === true);
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => window.__mock.fullscreen === false, null, { timeout: 1500 });
  assert.equal(await hidden(page), 0, 'leaving fullscreen closed the preview');
}, 'RD-LOOK-1');

test('LK-10: Escape closes the Info panel before the preview', async (page) => {
  await open(page);
  await page.keyboard.press('i');
  await settle(page);
  assert.equal(await infoOpen(page), true, 'I did not open the Info panel');
  await page.keyboard.press('Escape');
  await settle(page);
  assert.equal(await hidden(page), 0, 'Escape with Info open closed the whole preview');
  assert.equal(await infoOpen(page), false, 'Escape left the Info panel open');
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => window.__mock.calls.some(([n]) => n === 'hide_window'));
}, 'LK-10');

test('Escape ladder: menu, Info, fullscreen, close', async (page) => {
  await open(page, { actions: [offer] });
  await page.keyboard.press('a');
  await page.locator('.arcade-menu').waitFor({ state: 'visible' });
  await page.keyboard.press('i');
  await page.keyboard.press('f');
  await page.waitForFunction(() => window.__mock.fullscreen === true);
  await page.locator('.arcade-menu').waitFor({ state: 'visible' });
  const state = async () => ({ menu: await menuOpen(page), info: await infoOpen(page), fullscreen: await page.evaluate(() => window.__mock.fullscreen), closed: await hidden(page) });
  assert.deepEqual(await state(), { menu: true, info: true, fullscreen: true, closed: 0 });
  const steps = [
    { menu: false, info: true, fullscreen: true, closed: 0 },
    { menu: false, info: false, fullscreen: true, closed: 0 },
    { menu: false, info: false, fullscreen: false, closed: 0 },
    { menu: false, info: false, fullscreen: false, closed: 1 },
  ];
  for (const [index, expected] of steps.entries()) {
    await page.keyboard.press('Escape');
    await page.waitForTimeout(250);
    assert.deepEqual(await state(), expected, `after Escape ${index + 1}`);
  }
}, 'RD-LOOK-1');

for (const [width, height] of [[1040, 720], [720, 520]]) {
  test(`LK-2: the shortcuts dialog fits ${width}x${height} and its Close button is reachable`, async (page) => {
    await open(page, { viewport: { width, height } });
    await page.keyboard.press('?');
    await page.locator('.dialog.help').waitFor();
    const fit = await page.evaluate(() => {
      const dialog = document.querySelector('.dialog.help').getBoundingClientRect();
      const close = [...document.querySelectorAll('.dialog.help button')].find((b) => b.textContent.trim() === 'Close').getBoundingClientRect();
      const grid = document.querySelector('.dialog.help .help-grid');
      const scrolls = ['auto', 'scroll'].includes(getComputedStyle(grid).overflowY) || ['auto', 'scroll'].includes(getComputedStyle(document.querySelector('.dialog.help')).overflowY);
      const overflowing = grid.scrollHeight > grid.clientHeight + 1 || document.querySelector('.dialog.help').scrollHeight > document.querySelector('.dialog.help').clientHeight + 1;
      return { top: dialog.top, bottom: dialog.bottom, closeTop: close.top, closeBottom: close.bottom, height: innerHeight, scrolls, overflowing };
    });
    assert.ok(fit.top >= 0 && fit.bottom <= fit.height, `dialog leaves the window: ${JSON.stringify(fit)}`);
    assert.ok(fit.closeTop >= fit.top && fit.closeBottom <= fit.bottom, `Close button is outside the card: ${JSON.stringify(fit)}`);
    if (fit.overflowing) assert.ok(fit.scrolls, `rows overflow and cannot scroll: ${JSON.stringify(fit)}`);
    const rows = await page.locator('.dialog.help .key-row').evaluateAll((els) => els.map((el) => { const r = el.getBoundingClientRect(); const d = document.querySelector('.dialog.help').getBoundingClientRect(); return r.bottom <= d.bottom + 1; }));
    const visible = rows.filter(Boolean).length;
    assert.ok(visible === rows.length || fit.scrolls, 'rows are outside the card with no way to scroll');
  }, 'LK-2');
}

test('LK-2: the shortcut list names each key once and covers M, comma, period and the 3D group', async (page) => {
  await open(page);
  await page.keyboard.press('?');
  await page.locator('.dialog.help').waitFor();
  const rows = await page.locator('.dialog.help .key-row').evaluateAll((els) => els.map((el) => [el.querySelector('kbd').textContent.trim(), el.querySelector('span').textContent.trim()]));
  const keys = rows.map(([key]) => key);
  assert.ok(keys.includes('M'), 'M (mute) is not listed');
  assert.ok(keys.some((k) => k.includes(',')), 'the speed keys are not listed');
  const headings = await page.locator('.dialog.help h3').allTextContents();
  assert.ok(headings.includes('3D models'));
  const plainA = rows.filter(([key]) => /^A$|^A \//.test(key));
  assert.equal(plainA.length, 1, `A is listed ${plainA.length} times: ${JSON.stringify(plainA)}`);
}, 'LK-2');

test('LK-1: an error toast goes away on its own', async (page) => {
  await page.clock.install();
  await open(page);
  await page.evaluate(() => window.__mock.fail.push('open_default'));
  await page.keyboard.press('Enter');
  await page.locator('.toast, #toast, [class*=toast]').first().waitFor({ state: 'visible' });
  const toast = page.getByText("Couldn't finish the action", { exact: false });
  await toast.waitFor({ state: 'visible' });
  await page.clock.fastForward(20000);
  assert.equal(await toast.isVisible(), false, 'the error toast is still shown after 20 s');
}, 'LK-1');

test('LK-1: an error toast does not carry over to the next file', async (page) => {
  await open(page);
  await page.evaluate(() => window.__mock.fail.push('open_default'));
  await page.keyboard.press('Enter');
  const toast = page.getByText("Couldn't finish the action", { exact: false });
  await toast.waitFor({ state: 'visible' });
  await page.evaluate(() => window.__mock.send('open', '/fixtures/other.md'));
  await settle(page);
  assert.equal(await toast.isVisible(), false, 'the old error is shown on the new file');
}, 'LK-1');

test('Actions (A) with no peers says so', async (page) => {
  await open(page, { linkAvailable: false });
  await page.keyboard.press('a');
  await page.getByText('No connected apps can take this file').waitFor({ state: 'visible', timeout: 1500 });
}, 'A-EMPTY');

let failures = 0;
for (const { name, fn, known } of tests) {
  const page = await browser.newPage({ viewport: { width: 1040, height: 720 } });
  let error = null;
  try { await fn(page); } catch (e) { error = e; }
  await page.close();
  if (known && !error) { failures++; console.error(`FAIL ${name}: marked known (${known}) but it passes; remove the marker`); }
  else if (known) console.log(`known ${known}: ${name}\n    fails with: ${String(error.message).split("\n")[0].slice(0, 220)}`);
  else if (error) { failures++; console.error(`FAIL ${name}: ${error.message}`); }
  else console.log(`pass ${name}`);
}
await browser.close();
server.close();
if (failures) process.exit(1);
console.log('behaviour checks done');
