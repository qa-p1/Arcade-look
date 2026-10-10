// The preview window shell: title bar, stage, info panel, keyboard, navigation.
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { invoke } from '@tauri-apps/api/core';
import { api, errorMessage, type Bootstrap, type Config, type FileInfo } from './lib/backend';
import { clear, h, isEditable } from './lib/dom';
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
  private future: string[] = [];
  private status = '';
  private details: [string, string][] = [];
  private infoOpen = false;
  private missing: string | null = null;
  private shown = false;
  private peers = false;
  private strip: import('./lib/actions-strip').ActionsStrip | null = null;
  private connected: import('./lib/connected-apps').ConnectedApps | null = null;

  private titleName = h('div.title-name');
  private titleMeta = h('div.title-meta');
  private titleBadge = h('div.title-badge');
  private backButton = this.button('left', 'Back (Backspace / Alt+Left)', () => void this.back());
  private forwardButton = this.button('right', 'Forward (Alt+Right)', () => void this.forward());
  private navigation = h('div.title-navigation.hidden', this.backButton, this.forwardButton);
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
    document.documentElement.dataset.platform = this.boot.platform;
    this.applyConfig(this.boot.config);
    this.infoOpen = this.boot.infoPanel;
    this.build();
    this.bindGlobal();

    await listen<string | null>('open', (e) => void this.open(e.payload ?? null));
    await listen('hidden', () => this.teardown());
    await listen('settings', () => void this.openSettings());
    await listen<Config>('config', (e) => this.applyConfig(e.payload));
    await listen<boolean>('link-changed', (e) => { this.peers = e.payload; void this.updatePeers(); void this.connected?.refresh(); });
    void invoke<boolean>('link_available').then((available) => { this.peers = available; void this.updatePeers(); });

    const { screen, pending } = this.boot;
    if (screen === 'settings') await this.openSettings();
    else if (screen === 'file' && pending) await this.open(pending);
    else if (screen === 'welcome') await this.open(null);
  }

  /** Apply settings that take effect immediately (startup, config.json edits, the settings screen). */
  private applyConfig(config: Config) {
    this.boot.config = config;
    const html = document.documentElement;
    if (config.theme === 'light' || config.theme === 'dark') html.dataset.theme = config.theme;
    else delete html.dataset.theme;
  }

  private windowTitle = '';
  private async updatePeers() {
    const gen = this.gen;
    const info = this.abort?.signal.aborted ? null : this.current;
    if (this.peers && info && !this.strip) {
      const { ActionsStrip } = await import('./lib/actions-strip');
      if (gen !== this.gen || !this.peers) return;
      this.strip ??= new ActionsStrip(this.actions, (path) => this.open(path), (message) => this.toast(message));
    }
    await this.strip?.update(this.peers ? info : null, this.mounted);
  }

  private setTitle(title: string) {
    document.title = title;
    // The window title is what task bars, Alt+Tab and screen readers show.
    if (title === this.windowTitle) return;
    this.windowTitle = title;
    void getCurrentWindow().setTitle(title).catch(() => {});
  }

  // ------------------------------------------------------------------ layout

  private build() {
    const isMac = this.boot.platform === 'macos';
    const winControls = isMac
      ? null
      : h('div.win-controls',
          this.button('maximize', 'Maximize', () => getCurrentWindow().toggleMaximize(), 'win-btn'),
          this.button('close', 'Close (Space / Esc)', () => this.close(), 'win-btn win-close'));

    const titleText = h('div.title-text', { 'data-tauri-drag-region': true }, this.titleName, this.titleMeta);
    this.titleName.setAttribute('data-tauri-drag-region', '');
    this.titleMeta.setAttribute('data-tauri-drag-region', '');
    const bar = h('header.titlebar', { 'data-tauri-drag-region': true },
      isMac ? h('div.traffic-spacer', { 'data-tauri-drag-region': true }) : null,
      this.navigation,
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
    this.navigation.classList.toggle('hidden', !i);
    this.backButton.disabled = !i || !(this.history.length || (i.dir && i.dir !== i.path));
    this.forwardButton.disabled = !i || !this.future.length;
    clear(this.titleBadge);
    if (!i) {
      this.titleName.textContent = 'Arcade Look';
      this.titleMeta.textContent = 'Press Space, see anything';
      this.actions.classList.add('hidden');
      this.setTitle('Arcade Look');
      return;
    }
    this.actions.classList.remove('hidden');
    this.titleBadge.append(kindBadge(i.kind, i.ext, 16));
    this.titleName.textContent = i.name;
    this.titleName.title = i.path;
    this.setTitle(`${i.name} — Arcade Look`);
    if (this.missing) {
      this.titleMeta.textContent = this.missing;
      return;
    }
    const parts = [kindLabel(i)];
    if (i.kind !== 'folder') parts.push(fmt.bytes(i.size));
    if (this.status) parts.push(this.status);
    this.titleMeta.textContent = parts.join('  ·  ');
  }

  private async renderInfo() {
    clear(this.info);
    const info = this.current;
    if (!info || !this.infoOpen) return;
    const { renderInfo } = await import('./lib/info-panel');
    if (info !== this.current || !this.infoOpen) return;
    renderInfo(this.info, info, this.details);
  }

  // ------------------------------------------------------------------ navigation

  async open(path: string | null, opts: { drill?: boolean; traversal?: 'back' | 'forward' } = {}) {
    const gen = ++this.gen;
    this.connected?.dispose();
    this.connected = null;
    this.abort?.abort();
    const ac = new AbortController();
    this.abort = ac;
    this.closeHelp();

    if (path === null) {
      this.teardown();
      this.current = null;
      this.renderTitle();
      this.renderInfo();
      await this.showWelcome();
      await this.reveal_window();
      return;
    }
    this.trace(`open ${path}`);

    const loadingTimer = window.setTimeout(() => this.loading.classList.add('on'), 120);
    let info: FileInfo;
    let inspectError: string | null = null;
    try {
      info = await api.inspect(path);
    } catch (e) {
      const { missingInfo } = await import('./lib/info-panel');
      info = missingInfo(path);
      inspectError = errorMessage(e);
    }
    if (gen !== this.gen) return clearTimeout(loadingTimer);
    this.trace(`inspected: ${info.kind}/${info.format}${inspectError ? ` error=${inspectError}` : ''}`);

    // Commit history only for the request that actually becomes the current preview.
    if (opts.traversal === 'back') {
      this.history.pop();
      if (this.current) this.future.push(this.current.path);
    } else if (opts.traversal === 'forward') {
      this.future.pop();
      if (this.current) this.history.push(this.current.path);
    } else {
      if (opts.drill && this.current) {
        if (this.current.path !== info.path) this.history.push(this.current.path);
      } else {
        this.history = [];
      }
      this.future = [];
    }
    this.current = info;
    void this.strip?.update(null, null);
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
    void this.updatePeers();
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
      if (next) await this.open(next);
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
    if (!this.current) return;
    const prev = this.history[this.history.length - 1] ?? this.current.dir;
    if (prev && prev !== this.current.path) await this.open(prev, { traversal: 'back' });
  }

  async forward() {
    if (!this.current) return;
    const next = this.future[this.future.length - 1];
    if (next) await this.open(next, { traversal: 'forward' });
  }

  /** Stop media and free memory: the window is hidden. */
  teardown() {
    this.strip?.cancel();
    this.connected?.dispose();
    this.connected = null;
    this.gen++;
    this.abort?.abort();
    this.history = [];
    this.future = [];
    this.backButton.disabled = true;
    this.forwardButton.disabled = true;
    this.mounted?.dispose?.();
    this.mounted = null;
    clear(this.stage);
    this.loading.classList.remove('on');
    this.shown = false;
    void this.strip?.update(null, null);
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

  private async showWelcome() {
    const gen = this.gen;
    const { showWelcome } = await import('./lib/welcome');
    if (gen !== this.gen) return;
    showWelcome(this.stage, this.boot, (title, body) => this.showMessage(title, body),
      (message) => this.toast(message), () => { void this.openSettings(); });
  }

  async openSettings() {
    this.teardown();
    this.current = null;
    this.renderTitle();
    this.renderInfo();
    this.titleName.textContent = 'Settings';
    this.titleMeta.textContent = 'Arcade Look';
    this.setTitle('Settings — Arcade Look');
    const gen = this.gen;
    const [autostart, integration] = await Promise.all([
      api.getAutostart().catch(() => false),
      api.integrationStatus().catch(() => this.boot.integration),
    ]);
    if (gen !== this.gen) return;
    this.boot.integration = integration;
    const [{ showSettings }, { ConnectedApps }] = await Promise.all([import('./lib/settings'), import('./lib/connected-apps')]);
    if (gen !== this.gen) return;
    showSettings(this.stage, this.boot, autostart, (patch) => this.setConfig(patch),
      (message) => this.toast(message), (title, body) => this.showMessage(title, body));
    if (gen !== this.gen) return;
    this.connected = new ConnectedApps(this.stage.querySelector<HTMLElement>('#connected-apps')!,
      () => this.boot.config, (patch) => this.setConfig(patch), (message) => this.toast(message),
      this.boot.shortcutSupported, (config) => this.applyConfig(config));
    await this.reveal_window();
  }

  private async setConfig(patch: Partial<Config>): Promise<Config> {
    const config = await api.setConfig(patch);
    this.applyConfig(config);
    return config;
  }

  private showMessage(title: string, body: string) {
    this.closeHelp();
    this.helpEl = h('div.modal-backdrop', { onclick: () => this.closeHelp() },
      h('div.modal', { onclick: (e: Event) => e.stopPropagation() },
        h('h2', title), h('pre.modal-text', body),
        h('button.btn', { onclick: () => this.closeHelp() }, 'OK')));
    this.root.append(this.helpEl);
  }

  private async showHelp() {
    if (this.helpEl) return this.closeHelp();
    const gen = this.gen;
    const { showHelp } = await import('./lib/welcome');
    if (gen !== this.gen || this.helpEl) return;
    this.helpEl = showHelp(this.root, () => this.closeHelp());
  }

  private closeHelp() {
    this.helpEl?.remove();
    this.helpEl = null;
  }

  // ------------------------------------------------------------------ global events

  private bindGlobal() {
    window.addEventListener('keydown', (e) => this.onKey(e));
    // Gaming-mouse thumb buttons are browser Back/Forward (buttons 3 and 4).
    // Cancel every phase so the webview stays on the app; navigate once on release.
    const onThumbButton = (e: MouseEvent) => {
      if (e.button !== 3 && e.button !== 4) return;
      e.preventDefault();
      e.stopPropagation();
      if (e.type === 'mouseup') void (e.button === 3 ? this.back() : this.forward());
    };
    window.addEventListener('mousedown', onThumbButton, true);
    window.addEventListener('mouseup', onThumbButton, true);
    window.addEventListener('auxclick', onThumbButton, true);
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
    // On the settings screen, Space and Enter operate the focused switch or button.
    if ((k === ' ' || k === 'Enter') && (e.target as Element | null)?.closest?.('.settings :is(button, input)')) return;
    if (this.helpEl && (k === 'Escape' || k === '?' || k === ' ' || k === 'Enter')) {
      e.preventDefault();
      this.closeHelp();
      return;
    }
    // Peer actions take A when present; model auto-rotate keeps A standalone
    // and remains available with Shift+A and the viewer toolbar.
    if (!mod && !e.altKey && !e.shiftKey && k.toLowerCase() === 'a' && this.strip?.toggle()) {
      e.preventDefault();
      return;
    }
    if (k === 'Escape' && this.strip?.close()) { e.preventDefault(); return; }
    if ((k === ' ' || k === 'Enter') && (e.target as Element | null)?.closest?.('.arcade-actions button')) return;
    if (!mod && e.altKey && (k === 'ArrowLeft' || k === 'ArrowRight')) {
      e.preventDefault();
      void (k === 'ArrowLeft' ? this.back() : this.forward());
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
        if (plain) {
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
