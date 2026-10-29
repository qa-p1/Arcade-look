// Layout and interaction checks against the actual built UI with isolated native IPC.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFileSync, existsSync, mkdirSync } from 'node:fs';
import { resolve, extname, join } from 'node:path';
import { chromium } from 'playwright';

const root = resolve(new URL('../..', import.meta.url).pathname);
const dist = join(root, 'dist');
const shots = process.env.ARCADE_UI_SHOTS ?? join(root, 'artifacts/ui');
mkdirSync(shots, { recursive: true });
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.woff2': 'font/woff2', '.svg': 'image/svg+xml', '.png': 'image/png' };
const server = createServer((req, res) => {
  let path = join(dist, new URL(req.url, 'http://localhost').pathname);
  if (!existsSync(path) || path.endsWith('/')) path = join(dist, 'index.html');
  res.writeHead(200, { 'content-type': types[extname(path)] ?? 'application/octet-stream' });
  res.end(readFileSync(path));
});
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const url = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({ headless: true, executablePath: process.env.PLAYWRIGHT_CHROMIUM || undefined, args: ['--no-sandbox'] });
const mock = readFileSync(join(root, 'scripts/ui-checks/mock.js'), 'utf8');
let pages = 0;
try {
  for (const theme of ['light', 'dark']) {
    for (const scale of [1, 1.25]) {
      const page = await browser.newPage({ viewport: { width: 720, height: 520 }, colorScheme: theme });
      const errors = [];
      page.on('pageerror', (error) => errors.push(error.message));
      await page.addInitScript(mock);
      await page.goto(url + '/?settings');
      await page.waitForFunction(() => window.__mock.calls.some(([name]) => name === 'show_window'));
      await page.evaluate(async (factor) => {
        await document.fonts.ready;
        if (factor !== 1) {
          const tokens = ['body', 'control', 'small', 'caption', 'heading', 'title', 'title-sm', 'display', 'mono'];
          const style = getComputedStyle(document.documentElement);
          const values = tokens.map((name) => [name, parseFloat(style.getPropertyValue(`--fs-${name}`)), parseFloat(style.getPropertyValue(`--lh-${name}`))]);
          for (const [name, size, line] of values) {
            if (Number.isFinite(size)) document.documentElement.style.setProperty(`--fs-${name}`, size * factor + 'px');
            if (Number.isFinite(line)) document.documentElement.style.setProperty(`--lh-${name}`, line * factor + 'px');
          }
        }
      }, scale);
      for (const section of ['General', 'Shortcuts', 'Previews', 'File manager', 'Plugins', 'Connected apps', 'About']) {
        await page.getByRole('tab', { name: section, exact: true }).click();
        await page.waitForFunction(() => !document.querySelector('.skeleton'));
        const overflow = await page.evaluate(() => {
          const failed = [];
          for (const el of document.querySelectorAll('button, .row-title, .setting-label, .setting-desc, .row-sub')) {
            if (!el.getClientRects().length) continue;
            const style = getComputedStyle(el);
            if (el.scrollWidth > el.clientWidth + 2 && !['hidden', 'auto', 'scroll'].includes(style.overflowX)) failed.push(`${el.className}: ${el.textContent}`);
          }
          if (document.body.scrollWidth > innerWidth || document.documentElement.scrollWidth > innerWidth) failed.push('window scrolls horizontally');
          return failed;
        });
        assert.deepEqual(overflow, [], `${theme} ${scale} ${section}: ${overflow.join('; ')}`);
        await page.screenshot({ path: join(shots, `settings-${section.toLowerCase().replaceAll(' ', '-')}-${theme}-${scale}.png`) });
        pages++;
      }
      await page.getByRole('tab', { name: 'General', exact: true }).click();
      await page.getByRole('radio', { name: theme === 'dark' ? 'Light' : 'Dark', exact: true }).click();
      await page.waitForFunction((theme) => document.documentElement.dataset.theme === theme, theme === 'dark' ? 'light' : 'dark');
      await page.getByRole('radio', { name: 'System', exact: true }).click();
      assert.equal(await page.evaluate(() => document.documentElement.hasAttribute('data-theme')), false);
      // Shortcut capture applies live and conflicts require consent.
      await page.getByRole('tab', { name: 'Shortcuts', exact: true }).click();
      await page.getByRole('button', { name: 'Record global shortcut' }).click();
      await page.keyboard.press('Control+Alt+F');
      await page.getByRole('button', { name: 'Use anyway', exact: true }).waitFor();
      assert.equal(await page.evaluate(() => window.__mock.config.globalShortcut), 'Ctrl+Alt+Shift+Space');
      await page.getByRole('button', { name: 'Choose another', exact: true }).click();
      await page.getByRole('button', { name: 'Record global shortcut' }).focus();
      await page.keyboard.press('Escape');
      await page.getByRole('button', { name: 'Record global shortcut' }).click();
      await page.keyboard.press('Control+Alt+P');
      await page.waitForFunction(() => window.__mock.config.globalShortcut === 'Ctrl+Alt+P');
      // Loaded data can fail, then retry successfully.
      await page.evaluate(() => window.__mock.fail.push('link_connected'));
      await page.getByRole('tab', { name: 'Connected apps', exact: true }).click();
      await page.getByRole('button', { name: 'Retry', exact: true }).waitFor();
      await page.evaluate(() => { window.__mock.fail = []; });
      await page.getByRole('button', { name: 'Retry', exact: true }).click();
      await page.getByText('Arcade Box', { exact: true }).waitFor();
      assert.deepEqual(errors, [], errors.join('\n'));
      await page.close();
    }
    const page = await browser.newPage({ viewport: { width: 1100, height: 760 }, colorScheme: theme });
    const errors = []; page.on('pageerror', (error) => errors.push(error.message));
    await page.addInitScript(mock); await page.goto(url);
    await page.getByRole('heading', { name: 'Getting started', exact: true }).waitFor();
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({ path: join(shots, `preview-${theme}.png`) });
    await page.setViewportSize({ width: 720, height: 520 });
    await page.keyboard.press('Control+,');
    await page.waitForFunction(() => window.__mock.calls.some(([command]) => command === 'open_settings'));
    assert.equal(await page.getByRole('heading', { name: 'Getting started', exact: true }).count(), 1, 'Opening settings must preserve the current preview');
    await page.getByRole('button', { name: 'Info (I)', exact: true }).hover();
    await page.getByRole('tooltip').waitFor();
    assert.equal(await page.getByRole('tooltip').locator('.key-chip').textContent(), 'I');
    await page.mouse.move(0, 0);
    await page.keyboard.press('a'); await page.getByText('No connected apps can take this file', { exact: true }).waitFor();
    await page.keyboard.press('Escape');
    assert.deepEqual(errors, [], errors.join('\n'));
    await page.screenshot({ path: join(shots, `preview-minimum-${theme}.png`) });
    await page.close();
  }
  console.log(`PASS: ${pages} minimum-size Settings states in light/dark at 100/125%, live themes, shortcut conflict/apply, failure/retry, preview preserved while opening Settings, empty actions, and preview screenshots.`);
} finally { await browser.close(); server.close(); }
