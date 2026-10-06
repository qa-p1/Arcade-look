// Loaded only after the backend discovers peer entries.
import { Channel, invoke } from '@tauri-apps/api/core';
import { h } from './dom';
import { bytes } from './format';
import { arcadeGlyph } from './arcade-glyphs';
import { errorMessage, type FileInfo } from './backend';
import tokens from './arcade-tokens.json';
import type { Mounted } from '../viewers/types';
import './arcade.css';

interface Offer {
  app: string; action: string; title: string; reason: string | null;
  outbound: boolean; pdfPage: boolean;
  pipeline: string | null;
}
interface Progress { fraction?: number; message: string }
interface Result { outputs: { type: string; path?: string; paths?: string[] }[]; message?: string }

export class ActionsStrip {
  private root = h('div.arcade-actions.arcade-surface');
  private button = h('button.icon-btn', { title: 'Actions (A)', 'aria-label': 'Actions (A)', 'aria-haspopup': 'menu', 'aria-expanded': 'false' }, 'A');
  private menu = h('div.arcade-menu', { role: 'menu', hidden: true });
  private chip = h('div.arcade-job', { role: 'status', 'aria-live': 'polite', hidden: true });
  private info: FileInfo | null = null;
  private viewer: Mounted | null = null;
  private offers: Offer[] = [];
  private generation = 0;
  private request: string | null = null;
  private pending = false;
  private cancelJob: (() => void) | null = null;
  private closed = true;
  private outside = (event: PointerEvent) => {
    if (!this.root.contains(event.target as Node)) this.close();
  };

  constructor(private host: HTMLElement, private open: (path: string) => Promise<void>, private toast: (message: string) => void) {
    this.root.style.setProperty('--arcade-accent', tokens.accent['arcade.look']);
    this.root.append(this.button, this.menu, this.chip);
    this.button.addEventListener('click', () => this.toggle());
    this.root.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') { this.close(); this.button.focus(); e.preventDefault(); e.stopPropagation(); }
      else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        const buttons = [...this.menu.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')];
        if (!buttons.length) return;
        const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
        buttons[(index + (e.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length].focus();
        e.preventDefault(); e.stopPropagation();
      }
    });
    document.addEventListener('pointerdown', this.outside);
  }

  async update(info: FileInfo | null, viewer: Mounted | null) {
    const generation = ++this.generation;
    const changedFile = this.info?.path !== info?.path;
    this.info = info;
    this.viewer = viewer;
    if (changedFile || !info) {
      this.close();
      this.offers = [];
      this.render();
    }
    if (!info) return;
    try {
      const offers = await invoke<Offer[]>('link_actions', { path: info.path, pdfPage: !!viewer?.capturePage });
      if (generation !== this.generation) return;
      this.offers = offers;
      this.render();
    } catch (e) { if (generation === this.generation) this.toast(errorMessage(e)); }
  }

  private render() {
    this.menu.replaceChildren();
    const info = this.info;
    for (const offer of this.offers) {
      const reason = offer.reason;
      const title = offer.title + (offer.outbound ? ' ↗' : '');
      const payload = `${info?.name} · ${bytes(info?.size)}`;
      const hint = reason ?? (offer.outbound ? payload : offer.pdfPage ? 'Current page' : null);
      const button = h('button.arcade-offer', { type: 'button', role: 'menuitem', disabled: !!reason || this.pending,
        title: reason ?? (offer.pdfPage ? `Current page of ${info?.name}` : info?.path), 'aria-label': title },
        arcadeGlyph(offer.app), h('span', h('span.arcade-offer-title', title),
          hint ? h('small', hint) : null));
      button.addEventListener('click', () => void this.run(offer));
      this.menu.append(button);
    }
    this.button.hidden = !this.offers.length;
    if (this.offers.length || this.pending) {
      if (!this.root.parentElement) this.host.prepend(this.root);
    } else this.root.remove();
  }

  toggle(): boolean {
    if (!this.offers.length) return false;
    this.closed = !this.closed;
    this.menu.hidden = this.closed;
    this.button.setAttribute('aria-expanded', String(!this.closed));
    if (!this.closed) this.menu.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
    return true;
  }

  close(): boolean {
    const open = !this.closed;
    this.closed = true;
    this.menu.hidden = true;
    this.button.setAttribute('aria-expanded', 'false');
    return open;
  }

  private async run(offer: Offer) {
    if (this.pending || offer.reason || !this.info) return;
    const info = this.info;
    const viewer = this.viewer;
    const generation = this.generation;
    this.pending = true;
    this.close();
    this.render();
    const label = h('span', offer.title);
    const progressBar = h('progress', { max: 1, 'aria-label': offer.title });
    const cancel = h('button', { type: 'button', 'aria-label': 'Cancel action' }, 'Cancel');
    this.chip.replaceChildren(label, progressBar, cancel);
    this.chip.hidden = false;
    let cancelled = false;
    let backendReady = false;
    const requestId = crypto.randomUUID();
    this.request = requestId;
    this.cancelJob = () => {
      cancelled = true;
      cancel.disabled = true;
      label.textContent = 'Cancelling…';
      if (backendReady) void invoke('link_cancel', { requestId });
    };
    cancel.addEventListener('click', this.cancelJob);
    const progress = new Channel<Progress>();
    progress.onmessage = (p) => {
      backendReady = true;
      if (cancelled) { void invoke('link_cancel', { requestId }); return; }
      label.textContent = p.message || offer.title;
      if (p.fraction !== undefined) progressBar.value = Math.max(0, Math.min(1, p.fraction));
    };
    try {
      let pdfPng: number[] | null = null;
      if (offer.pdfPage) {
        if (!viewer?.capturePage) throw new Error('This PDF page could not be rendered.');
        pdfPng = Array.from(await viewer.capturePage());
        if (generation !== this.generation) return;
      }
      if (cancelled) return;
      const result = await invoke<Result>('link_invoke', { appId: offer.app, actionId: offer.action,
        path: info.path, requestId, pdfPng, pipeline: offer.pipeline, progress });
      const output = result.outputs.find((o) => o.type.startsWith('file/') || o.type === 'folder/reference');
      const path = output?.path ?? output?.paths?.[0];
      if (offer.app === 'arcade.box' && path) await this.open(path);
      else if (result.message) this.toast(result.message);
    } catch (e) { this.toast(errorMessage(e)); }
    finally {
      this.request = null;
      this.cancelJob = null;
      this.pending = false;
      this.chip.hidden = true;
      this.render();
    }
  }

  dispose() {
    this.generation++;
    if (this.request) void invoke('link_cancel', { requestId: this.request });
    document.removeEventListener('pointerdown', this.outside);
    this.root.remove();
  }

  cancel() { this.cancelJob?.(); }
}
