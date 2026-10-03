// The preview window shell: title bar, stage, info panel, keyboard, navigation.
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { api, errorMessage, type Bootstrap, type FileInfo } from './lib/backend';
import { append, clear, h, isEditable } from './lib/dom';
import * as fmt from './lib/format';
import { kindLabel } from './lib/kinds';
import { setMediaBase } from './lib/urls';
import { icon, kindBadge } from './lib/icons';
import { mountChain, preload } from './viewers/registry';
import type { Mounted, ToolItem, ViewCtx } from './viewers/types';

const SWAP_GRACE_MS = 180;


export class App {
  private boot!: Bootstrap;
  private current: FileInfo | null = null;
  private mounted: Mounted | null = null;
  private abort: AbortController | null = null;
  private gen = 0;
  private history: string[] = [];
  private status = '';
  private details: [string, string][] = [];
  private infoOpen = false;
  private missing: string | null = null;
  private shown = false;

  private titleName = h('div.title-name');
  private titleMeta = h('div.title-meta');
  private titleBadge = h('div.title-badge');
  private actions = h('div.title-actions');
  private stage = h('main.stage');
  private info = h('aside.info-panel');
  private loading = h('div.loading-bar');
  private toastEl = h('div.toast');
  private dropOverlay = h('div.drop-overlay', h('div.drop-card', icon('open', 28), h('span', 'Drop to preview')));
  private helpEl: HTMLElement | null = null;
  private toastTimer = 0;

  constructor(private root: HTMLElement) {}

  private traceT0 = 0;
  private trace(msg: string) {
    if (!this.boot?.debug) return;
    if (msg.startsWith('open ')) this.traceT0 = performance.now();
    void api.log('trace', `${msg} (+${Math.round(performance.now() - this.traceT0)} ms)`);
  }

  async start() {
    try {
      this.boot = await api.bootstrap();
    } catch (e) {
      this.root.append(h('div.fatal', h('h1', 'Arcade Look'), h('p', `Could not reach the app backend: ${errorMessage(e)}`)));
      return;
    }
    setMediaBase(this.boot.mediaBase);
    const html = document.documentElement;
    html.dataset.platform = this.boot.platform;
    if (this.boot.config.theme !== 'system') html.dataset.theme = this.boot.config.theme;
    this.infoOpen = this.boot.infoPanel;
    this.build();
    this.bindGlobal();

    await listen<string | null>('open', (e) => void this.open(e.payload ?? null));
    await listen('hidden', () => this.teardown());

    if (this.boot.pending) await this.open(this.boot.pending);
    else if (!this.boot.service) await this.open(null);
  }

  // ------------------------------------------------------------------ layout

  private build() {
    const isMac = this.boot.platform === 'macos';
    const winControls = isMac
      ? null
      : h('div.win-controls',
          this.button('minimize', 'Minimize', () => getCurrentWindow().minimize(), 'win-btn'),
          this.button('maximize', 'Maximize', () => getCurrentWindow().toggleMaximize(), 'win-btn'),
          this.button('close', 'Close (Space / Esc)', () => this.close(), 'win-btn win-close'));

    const titleText = h('div.title-text', { 'data-tauri-drag-region': true }, this.titleName, this.titleMeta);
    this.titleName.setAttribute('data-tauri-drag-region', '');
    this.titleMeta.setAttribute('data-tauri-drag-region', '');
    const bar = h('header.titlebar', { 'data-tauri-drag-region': true },
      isMac ? h('div.traffic-spacer', { 'data-tauri-drag-region': true }) : null,
      this.titleBadge,
      titleText,
      h('div.title-fill', { 'data-tauri-drag-region': true }),
      this.actions,
      winControls,
    );
    bar.addEventListener('dblclick', (e) => {
      if ((e.target as HTMLElement).closest('button')) return;
      void getCurrentWindow().toggleMaximize();
    });

    this.actions.append(
      this.button('open', 'Open with default app (Enter)', () => this.openDefault()),
      this.button('reveal', 'Show in folder (Ctrl+R)', () => this.reveal()),
      this.button('copy', 'Copy path (Ctrl+C)', () => this.copyPath()),
      this.button('info', 'Info (I)', () => this.toggleInfo(), 'icon-btn info-toggle'),
    );

    this.root.append(bar, h('div.body', this.stage, this.info), this.loading, this.toastEl, this.dropOverlay);
    this.root.classList.toggle('info-open', this.infoOpen);
  }

  private button(name: string, title: string, onClick: () => void, cls = 'icon-btn'): HTMLButtonElement {
    const b = h('button', { class: cls, title, 'aria-label': title, onclick: (e: Event) => { e.stopPropagation(); onClick(); } }, icon(name, 17));
    b.tabIndex = -1;
    return b;
  }

  private renderTitle() {
    const i = this.current;
    clear(this.titleBadge);
    if (!i) {
      this.titleName.textContent = 'Arcade Look';
      this.titleMeta.textContent = 'Press Space, see anything';
      this.actions.classList.add('hidden');
      document.title = 'Arcade Look';
      return;
    }
    this.actions.classList.remove('hidden');
    this.titleBadge.append(kindBadge(i.kind, i.ext, 16));
    this.titleName.textContent = i.name;
    this.titleName.title = i.path;
    if (this.missing) {
      this.titleMeta.textContent = this.missing;
      return;
    }
    const parts = [kindLabel(i)];
    if (i.kind !== 'folder') parts.push(fmt.bytes(i.size));
    if (this.status) parts.push(this.status);
    this.titleMeta.textContent = parts.join('  ·  ');
    document.title = `${i.name} — Arcade Look`;
  }

  private renderInfo() {
    clear(this.info);
    const i = this.current;
    if (!i || !this.infoOpen) return;
    const row = (k: string, v: string | null | undefined, cls = '') =>
      v ? h('div.info-row', h('div.info-key', k), h(`div.info-val${cls ? `.${cls}` : ''}`, v)) : null;
    const general = [
      row('Kind', kindLabel(i)),
      i.kind !== 'folder' ? row('Size', fmt.bytes(i.size, true)) : null,
      row('Created', i.created ? fmt.date(i.created) : null),
      row('Modified', fmt.date(i.modified)),
      row('Where', i.dir, 'mono'),
      row('Type', i.mime, 'mono'),
      i.mode !== null ? row('Permissions', fmt.permissions(i.mode), 'mono') : null,
      i.symlink !== null ? row('Link to', i.symlink, 'mono') : null,
      i.readonly ? row('Access', 'Read-only') : null,
      i.hidden ? row('Visibility', 'Hidden') : null,
      i.plugin ? row('Plugin', i.plugin.name) : null,
    ];
    append(this.info, [
      h('div.info-head', kindBadge(i.kind, i.ext, 22), h('div.info-name', i.name)),
      h('section.info-section', h('h3', 'General'), general),
      this.details.length
        ? h('section.info-section', h('h3', 'Details'), this.details.map(([k, v]) => row(k, v)))
        : null,
    ]);
  }

  // ------------------------------------------------------------------ navigation

  async open(path: string | null, opts: { drill?: boolean; back?: boolean } = {}) {
    const gen = ++this.gen;
    this.abort?.abort();
    const ac = new AbortController();
    this.abort = ac;
    this.closeHelp();

    if (path === null) {
      this.teardown();
      this.current = null;
      this.renderTitle();
      this.renderInfo();
      this.showWelcome();
      await this.reveal_window();
      return;
    }
    this.trace(`open ${path}`);
    if (opts.drill && this.current) this.history.push(this.current.path);
    else if (!opts.back) this.history = [];

    const loadingTimer = window.setTimeout(() => this.loading.classList.add('on'), 120);
    let info: FileInfo;
    let inspectError: string | null = null;
    try {
      info = await api.inspect(path);
    } catch (e) {
      info = missingInfo(path);
      inspectError = errorMessage(e);
    }
    if (gen !== this.gen) return clearTimeout(loadingTimer);
    this.trace(`inspected: ${info.kind}/${info.format}${inspectError ? ` error=${inspectError}` : ''}`);

    this.current = info;
    this.missing = inspectError;
    this.status = '';
    this.details = [];
    this.renderTitle();
    this.renderInfo();

    const host = h('div.viewer-host.pending');
    this.stage.append(host);
    const ctx = this.makeCtx(info, ac.signal, gen, host, false);
    ctx.previousError = inspectError;

    const mountP = mountChain(host, ctx);
    // Keep the old preview on screen briefly so fast switches don't flash.
    const quick = await Promise.race([mountP.then(() => true, () => true), sleep(SWAP_GRACE_MS).then(() => false)]);
    if (gen !== this.gen) {
      mountP.then((r) => r.mounted.dispose?.(), () => {});
      host.remove();
      return clearTimeout(loadingTimer);
    }
    this.swapIn(host);
    if (!quick) await this.reveal_window(); // slow content: show the window with a loading state

    let result;
    try {
      result = await mountP;
    } catch {
      clearTimeout(loadingTimer);
      return; // aborted
    }
    clearTimeout(loadingTimer);
    if (gen !== this.gen) {
      result.mounted.dispose?.();
      return;
    }
    this.mounted = result.mounted;
    this.trace(`mounted with ${result.viewer}`);
    this.loading.classList.remove('on');
    await this.reveal_window();
    this.prefetchNeighbors(info, gen);
  }

  private swapIn(host: HTMLElement) {
    this.mounted?.dispose?.();
    this.mounted = null;
    for (const old of Array.from(this.stage.children)) if (old !== host) old.remove();
    host.classList.remove('pending');
  }

  private async reveal_window() {
    if (this.shown) {
      void api.showWindow();
      return;
    }
    this.trace('revealing window');
    // Note: no requestAnimationFrame here; WebKit doesn't run it while the window is hidden.
    this.shown = true;
    await api.showWindow();
  }

  private makeCtx(info: FileInfo, signal: AbortSignal, gen: number, host: HTMLElement, nested: boolean): ViewCtx {
    const live = () => gen === this.gen && !signal.aborted;
    return {
      info,
      boot: this.boot,
      signal,
      nested,
      previousError: null,
      setStatus: (text) => {
        if (nested || !live()) return;
        this.status = text;
        this.renderTitle();
      },
      setDetails: (rows) => {
        if (nested || !live()) return;
        this.details = rows.filter(([, v]) => v !== '' && v !== null && v !== undefined);
        this.renderInfo();
      },
      toolbar: (items) => this.makeToolbar(host, items),
      open: (path) => void this.open(path, { drill: true }),
      toast: (t) => this.toast(t),
      mountNested: async (nestedHost, nestedInfo) => {
        const r = await mountChain(nestedHost, this.makeCtx(nestedInfo, signal, gen, nestedHost, true));
        return r.mounted;
      },
    };
  }

  private makeToolbar(host: HTMLElement, items: ToolItem[]): HTMLElement {
    const bar = h('div.toolbar');
    for (const it of items) {
      if (it.separator) {
        bar.append(h('span.tb-sep'));
      } else if (it.el) {
        bar.append(it.el);
      } else {
        const b = h('button.tb-btn', { title: it.title, 'aria-label': it.title }, it.icon ? icon(it.icon, 17) : null, it.label ? h('span', it.label) : null);
        b.tabIndex = -1;
        if (it.active) b.classList.add('active');
        b.addEventListener('click', (e) => {
          e.stopPropagation();
          it.onClick?.(b);
        });
        bar.append(b);
      }
    }
    // Fade when the pointer is idle, like a video player.
    let t = 0;
    const wake = () => {
      bar.classList.remove('idle');
      clearTimeout(t);
      t = window.setTimeout(() => bar.classList.add('idle'), 2200);
    };
    host.addEventListener('pointermove', wake);
    bar.addEventListener('pointerenter', () => clearTimeout(t));
    wake();
    host.append(bar);
    return bar;
  }

  private async prefetchNeighbors(info: FileInfo, gen: number) {
    // Warm the viewer chunk for the next file so → feels instant.
    try {
      const next = await api.neighbor(info.path, 1);
      if (!next || gen !== this.gen) return;
      preload(await api.inspect(next));
    } catch {
      /* best effort */
    }
  }

  async navigate(delta: number) {
    if (!this.current) return;
    try {
      if (await api.navigateExternal(delta)) return;
      const next = await api.neighbor(this.current.path, delta);
      if (next) await this.open(next, { back: false });
      else this.bump(delta);
    } catch (e) {
      this.toast(errorMessage(e));
    }
  }

  private bump(delta: number) {
    const cls = delta < 0 ? 'bump-left' : 'bump-right';
    this.stage.classList.remove('bump-left', 'bump-right');
    void this.stage.offsetWidth;
    this.stage.classList.add(cls);
  }

  async back() {
    const prev = this.history.pop();
    if (prev) await this.open(prev, { back: true });
  }

  /** Stop media and free memory: the window is hidden. */
  teardown() {
    this.gen++;
    this.abort?.abort();
    this.mounted?.dispose?.();
    this.mounted = null;
    clear(this.stage);
    this.loading.classList.remove('on');
    this.shown = false;
    this.closeHelp();
  }

  close() {
    this.teardown();
    void api.hideWindow();
  }

  // ------------------------------------------------------------------ actions

  private openDefault() {
    if (this.current) api.openDefault(this.current.path).catch((e) => this.toast(errorMessage(e)));
  }

  private reveal() {
    if (this.current) api.reveal(this.current.path).catch((e) => this.toast(errorMessage(e)));
  }

  private async copyPath() {
    if (!this.current) return;
    const text = this.current.path;
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      const ta = h('textarea', { value: text, style: 'position:fixed;opacity:0' });
      document.body.append(ta);
      ta.select();
      document.execCommand('copy');
      ta.remove();
    }
    this.toast('Path copied');
  }

  private toggleInfo() {
    this.infoOpen = !this.infoOpen;
    this.root.classList.toggle('info-open', this.infoOpen);
    this.renderInfo();
    void api.setInfoPanel(this.infoOpen);
  }

  private async toggleFullscreen() {
    const w = getCurrentWindow();
    await w.setFullscreen(!(await w.isFullscreen()));
  }

  toast(text: string) {
    this.toastEl.textContent = text;
    this.toastEl.classList.add('on');
    clearTimeout(this.toastTimer);
    this.toastTimer = window.setTimeout(() => this.toastEl.classList.remove('on'), 2200);
  }

  // ------------------------------------------------------------------ welcome & help

  private showWelcome() {
    clear(this.stage);
    const integration = h('div.welcome-integration',
      this.boot.integration.map(([label, on]) => h(`span.chip${on ? '.on' : ''}`, on ? icon('check', 13) : null, label)),
    );
    const setup = h('button.btn.primary', 'Set up file manager integration');
    setup.addEventListener('click', async () => {
      setup.disabled = true;
      try {
        const msg = await api.installIntegration();
        this.showMessage('Integration installed', msg);
      } catch (e) {
        this.showMessage('Integration failed', errorMessage(e));
      } finally {
        setup.disabled = false;
      }
    });
    const keys: [string, string][] = [
      ['Space / Esc', 'Close'], ['← →', 'Previous / next file'], ['Enter', 'Open with default app'],
      ['I', 'Info panel'], ['F', 'Fullscreen'], ['+ − 0', 'Zoom'], ['?', 'All shortcuts'], ['Ctrl Q', 'Quit'],
    ];
    const link = (label: string, path: string) =>
      h('a.link', { href: '#', onclick: (e: Event) => { e.preventDefault(); api.openDefault(path).catch((err) => this.toast(errorMessage(err))); } }, label);
    this.stage.append(
      h('div.welcome',
        h('div.welcome-logo', icon('eye', 44)),
        h('h1', 'Arcade Look'),
        h('p.welcome-tag', 'Select a file, press Space, see it instantly.'),
        h('div.dropzone', icon('open', 20), h('span', 'Drop any file or folder here')),
        h('div.keys', keys.map(([k, v]) => h('div.key-row', h('kbd', k), h('span', v)))),
        integration,
        setup,
        h('div.welcome-foot',
          h('span', `v${this.boot.version}`), ' · ',
          link('Settings', this.boot.configPath), ' · ',
          link('Plugins folder', this.boot.pluginsDir)),
      ),
    );
  }

  private showMessage(title: string, body: string) {
    this.closeHelp();
    this.helpEl = h('div.modal-backdrop', { onclick: () => this.closeHelp() },
      h('div.modal', { onclick: (e: Event) => e.stopPropagation() },
        h('h2', title), h('pre.modal-text', body),
        h('button.btn', { onclick: () => this.closeHelp() }, 'OK')));
    this.root.append(this.helpEl);
  }

  private showHelp() {
    if (this.helpEl) return this.closeHelp();
    const groups: [string, [string, string][]][] = [
      ['Everywhere', [['Space / Esc', 'Close the preview'], ['← / →', 'Previous / next file in the folder'], ['Backspace', 'Back (after opening an item)'], ['Enter', 'Open with the default app'], ['Ctrl/⌘ R', 'Show in folder'], ['Ctrl/⌘ C', 'Copy path'], ['I', 'Toggle info panel'], ['F', 'Toggle fullscreen'], ['Ctrl/⌘ Q', 'Quit Arcade Look']]],
      ['Images · PDF · Text', [['+ / −', 'Zoom in / out'], ['0', 'Fit / reset zoom'], ['1', 'Actual size'], ['R', 'Rotate image'], ['W', 'Toggle line wrap']]],
      ['Video · Audio', [['K', 'Play / pause'], ['J / L', 'Back / forward 10 s'], ['M', 'Mute'], [', / .', 'Slower / faster']]],
      ['3D models', [['Drag', 'Orbit'], ['Scroll', 'Zoom'], ['W', 'Wireframe'], ['A', 'Auto-rotate']]],
    ];
    this.helpEl = h('div.modal-backdrop', { onclick: () => this.closeHelp() },
      h('div.modal.help', { onclick: (e: Event) => e.stopPropagation() },
        h('h2', icon('keyboard', 20), 'Keyboard shortcuts'),
        h('div.help-grid', groups.map(([g, rows]) => h('section', h('h3', g), rows.map(([k, v]) => h('div.key-row', h('kbd', k), h('span', v))))))));
    this.root.append(this.helpEl);
  }

  private closeHelp() {
    this.helpEl?.remove();
    this.helpEl = null;
  }

  // ------------------------------------------------------------------ global events

  private bindGlobal() {
    window.addEventListener('keydown', (e) => this.onKey(e));
    // The webview must never navigate away (links are handled by viewers).
    document.addEventListener('click', (e) => {
      const a = (e.target as Element | null)?.closest?.('a');
      if (a && !e.defaultPrevented) e.preventDefault();
    });
    document.addEventListener('contextmenu', (e) => {
      const sel = window.getSelection()?.toString();
      if (!isEditable(e.target) && !sel) e.preventDefault();
    });
    document.addEventListener('dragover', (e) => e.preventDefault());
    document.addEventListener('drop', (e) => e.preventDefault());
    void getCurrentWebview().onDragDropEvent((ev) => {
      const p = ev.payload;
      if (p.type === 'enter' || p.type === 'over') this.dropOverlay.classList.add('on');
      else if (p.type === 'leave') this.dropOverlay.classList.remove('on');
      else if (p.type === 'drop') {
        this.dropOverlay.classList.remove('on');
        if (p.paths[0]) void this.open(p.paths[0]);
      }
    });
  }

  private onKey(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    const k = e.key;
    // Never let the webview reload or open its own UI.
    if (k === 'F5' || (mod && (k === 'r' || k === 'R' || k === 'p' || k === 'f' || k === 'g' || k === 'u' || k === 'j' || k === 'n' || k === 's'))) {
      e.preventDefault();
    }
    if (mod && (k === 'q' || k === 'Q')) {
      e.preventDefault();
      void api.quit();
      return;
    }
    if (mod && (k === 'w' || k === 'W')) {
      e.preventDefault();
      this.close();
      return;
    }
    if (mod && (k === 'r' || k === 'R')) {
      this.reveal();
      return;
    }
    if (mod && (k === 'o' || k === 'O')) {
      e.preventDefault();
      this.openDefault();
      return;
    }
    if (isEditable(e.target)) {
      if (k === 'Escape') (e.target as HTMLElement).blur();
      return;
    }
    if (this.helpEl && (k === 'Escape' || k === '?' || k === ' ' || k === 'Enter')) {
      e.preventDefault();
      this.closeHelp();
      return;
    }
    if (this.mounted?.keydown?.(e)) {
      e.preventDefault();
      return;
    }
    const plain = !mod && !e.altKey;
    switch (k) {
      case ' ':
      case 'Escape':
        e.preventDefault();
        // The Space that opened us (in the file manager) may still be held down: its
        // auto-repeat must not close the preview it just opened.
        if (e.repeat) break;
        if (k === 'Escape') {
          void getCurrentWindow().isFullscreen().then((fs) => (fs ? getCurrentWindow().setFullscreen(false) : this.close()));
        } else {
          this.close();
        }
        break;
      case 'ArrowLeft':
      case 'ArrowRight':
        if (e.altKey && k === 'ArrowLeft') {
          e.preventDefault();
          void this.back();
        } else if (plain) {
          e.preventDefault();
          void this.navigate(k === 'ArrowLeft' ? -1 : 1);
        }
        break;
      case 'Backspace':
        e.preventDefault();
        void this.back();
        break;
      case 'Enter':
        e.preventDefault();
        this.openDefault();
        break;
      case 'i':
      case 'I':
        if (plain) this.toggleInfo();
        break;
      case 'f':
      case 'F':
        if (plain) void this.toggleFullscreen();
        break;
      case '?':
        this.showHelp();
        break;
      case 'c':
      case 'C':
        if (mod && !window.getSelection()?.toString()) {
          e.preventDefault();
          void this.copyPath();
        }
        break;
    }
  }
}

function sleep(ms: number) {
  return new Promise((r) => setTimeout(r, ms));
}

function missingInfo(path: string): FileInfo {
  const name = path.split(/[\\/]/).pop() || path;
  return {
    path, name, dir: null, ext: '', size: 0, modified: null, created: null, accessed: null,
    readonly: false, hidden: false, symlink: null, mode: null, kind: 'binary', format: '',
    lang: null, mime: 'application/octet-stream', plugin: null, fallbackPlugin: null,
  };
}
