// What a previewed document can do in the preview window: the allow-list sanitiser (Markdown, Office, EPUB and
// notebook output) and the sandboxed frame for HTML. The two modules are built from src/ exactly as the app builds
// them and run in a real browser engine; no IPC is involved. SECURITY.md ("Previewed documents") cites these tests.
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { build } from 'vite';
import { chromium } from 'playwright';

const root = resolve(new URL('../..', import.meta.url).pathname);
const work = mkdtempSync(join(tmpdir(), 'look-sanitize-'));
const entry = join(work, 'entry.ts');
writeFileSync(
  entry,
  `export { sanitize } from ${JSON.stringify(join(root, 'src/lib/sanitize.ts'))};\n` +
    `export { sandboxedFrame } from ${JSON.stringify(join(root, 'src/viewers/html.ts'))};\n`,
);
const built = await build({
  configFile: false,
  logLevel: 'silent',
  root,
  build: { write: false, minify: false, lib: { entry, formats: ['iife'], name: 'Look', fileName: 'look' } },
});
rmSync(work, { recursive: true, force: true });
const code = (Array.isArray(built) ? built : [built]).flatMap((b) => b.output).find((o) => o.type === 'chunk').code;

const browser = await chromium.launch({
  headless: true,
  executablePath: process.env.PLAYWRIGHT_CHROMIUM || undefined,
  args: ['--no-sandbox'],
});
const tests = [];
const test = (name, fn) => tests.push({ name, fn });

const hostile = `
<script>window.pwned = 1</script>
<img src=x onerror="window.pwned = 2">
<a href="javascript:alert(1)">js</a>
<a href="JaVaScRiPt:alert(1)">js mixed case</a>
<a href="  javascript:alert(1)">js leading space</a>
<a href="data:text/html,<script>1</script>">data</a>
<a href="vbscript:x">vb</a>
<iframe src="https://evil.example/"></iframe>
<object data="x"></object><embed src="x">
<svg onload="window.pwned = 3"><script>1</script></svg>
<math><mi>1</mi></math>
<form action="https://evil.example/"><input name=q><button>go</button></form>
<style>body { display: none }</style>
<link rel=stylesheet href="https://evil.example/x.css">
<meta http-equiv=refresh content="0;url=https://evil.example/">
<base href="https://evil.example/">
<video src=x></video><audio src=x></audio><canvas></canvas><dialog open>d</dialog>
<div onclick="window.pwned = 4" style="background:url(https://evil.example/t.gif)" class="kept">kept text</div>
<img src="file:///etc/passwd" alt="local file">
<img src="data:text/html;base64,PHNjcmlwdD4=" alt="html as image">
<img src="data:image/png;base64,AAAA" alt="embedded">
<img src="https://example.com/p.png" alt="remote">
<a href="https://example.com/x">web</a>
<a href="mailto:a@b.c">mail</a>
`;

async function load(page) {
  await page.setContent('<!doctype html><html><body></body></html>');
  await page.addScriptTag({ content: code });
}

test('the sanitiser keeps no script, frame, form, style or media element', async (page) => {
  await load(page);
  const found = await page.evaluate((html) => {
    const host = document.createElement('div');
    host.append(window.Look.sanitize(html, {}));
    return {
      tags: [...new Set([...host.querySelectorAll('*')].map((e) => e.tagName.toLowerCase()))],
      attrs: [...host.querySelectorAll('*')].flatMap((e) => [...e.attributes].map((a) => a.name)),
      pwned: window.pwned ?? null,
    };
  }, hostile);
  for (const tag of ['script', 'iframe', 'object', 'embed', 'svg', 'math', 'form', 'button', 'style', 'link', 'meta', 'base', 'video', 'audio', 'canvas', 'dialog']) {
    assert.ok(!found.tags.includes(tag), `<${tag}> survived`);
  }
  assert.deepEqual(found.attrs.filter((a) => a.startsWith('on') || a === 'style'), [], 'an event handler or style attribute survived');
  assert.equal(found.pwned, null, 'a script ran while sanitising');
});

test('the sanitiser keeps only web, mail and in-page links', async (page) => {
  await load(page);
  const hrefs = await page.evaluate((html) => {
    const host = document.createElement('div');
    host.append(window.Look.sanitize(html, {}));
    return [...host.querySelectorAll('a[href]')].map((a) => a.getAttribute('href'));
  }, hostile);
  assert.deepEqual(hrefs, ['https://example.com/x', 'mailto:a@b.c']);
});

test('the sanitiser keeps the content of a dropped wrapper and the text of a kept one', async (page) => {
  await load(page);
  const text = await page.evaluate((html) => {
    const host = document.createElement('div');
    host.append(window.Look.sanitize(html, {}));
    return host.querySelector('div.kept')?.textContent ?? null;
  }, hostile);
  assert.equal(text, 'kept text');
});

test('images: https is kept (a document may show a web image), file:, html and unpermitted data: are not', async (page) => {
  await load(page);
  const srcs = (allowDataImages) =>
    page.evaluate(
      ([html, allow]) => {
        const host = document.createElement('div');
        host.append(window.Look.sanitize(html, { allowDataImages: allow }));
        return [...host.querySelectorAll('img')].map((i) => i.getAttribute('src'));
      },
      [hostile, allowDataImages],
    );
  assert.deepEqual(await srcs(false), ['https://example.com/p.png']);
  assert.deepEqual(await srcs(true), ['data:image/png;base64,AAAA', 'https://example.com/p.png']);
});

test('the HTML preview frame is fully sandboxed and its policy comes before any document script', async (page) => {
  await load(page);
  const frame = await page.evaluate(() => {
    const withHead = window.Look.sandboxedFrame('<html><head><script>window.parent.pwned = 5</script></head><body>x</body></html>', '/home/u/docs');
    const bare = window.Look.sandboxedFrame('<script>window.parent.pwned = 6</script><p>x</p>', null);
    return {
      sandbox: withHead.getAttribute('sandbox'),
      referrer: withHead.getAttribute('referrerpolicy'),
      policyFirst: [withHead, bare].map((f) => f.srcdoc.indexOf('Content-Security-Policy') < f.srcdoc.indexOf('<script')),
      policy: withHead.srcdoc.match(/content="([^"]*)"/g),
      base: withHead.srcdoc.includes('<base href="'),
    };
  });
  assert.equal(frame.sandbox, '', 'the frame has sandbox="" (no scripts, forms, popups or same-origin access)');
  assert.equal(frame.referrer, 'no-referrer');
  assert.deepEqual(frame.policyFirst, [true, true]);
  assert.ok(frame.policy.some((c) => c.includes("script-src 'none'") && c.includes("form-action 'none'") && c.includes("object-src 'none'")));
  assert.ok(frame.base);
});

test('a script in an HTML preview does not run', async (page) => {
  await load(page);
  const pwned = await page.evaluate(
    () =>
      new Promise((done) => {
        const frame = window.Look.sandboxedFrame(
          '<script>window.parent.pwned = 7</script><img src=x onerror="window.parent.pwned = 8"><p>text</p>',
          null,
        );
        frame.addEventListener('load', () => setTimeout(() => done(window.pwned ?? null), 300));
        document.body.append(frame);
      }),
  );
  assert.equal(pwned, null, 'a script in the previewed HTML ran');
});

let failures = 0;
for (const { name, fn } of tests) {
  const page = await browser.newPage();
  try {
    await fn(page);
    console.log(`pass ${name}`);
  } catch (error) {
    failures++;
    console.error(`FAIL ${name}: ${error.message}`);
  }
  await page.close();
}
await browser.close();
if (failures) process.exit(1);
console.log('sanitize checks done');
