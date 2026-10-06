// Settings-only chunk: promotion never appears in previews or their actions.
import { invoke } from '@tauri-apps/api/core';
import { h } from './dom';
import { arcadeGlyph } from './arcade-glyphs';
import { api, errorMessage, type Config } from './backend';
import { ShortcutRecorder } from './shortcut-recorder';
import './arcade.css';

interface Peer { id: string; name: string; state: string; installed: boolean; enabled: boolean; pitch: string }
interface Snapshot { peers: Peer[]; registry: string; endpoint: string; lastError: string | null }

export class ConnectedApps {
  private generation = 0;
  private saving = false;
  private rows = h('div');
  private shortcut: ShortcutRecorder | null = null;
  constructor(private host: HTMLElement, private config: () => Config,
    private save: (patch: Partial<Config>) => Promise<Config>, private toast: (message: string) => void,
    shortcutSupported: boolean, changed: (config: Config) => void) {
    this.host.classList.add('arcade-surface');
    this.host.append(this.rows);
    if (shortcutSupported) {
      this.shortcut = new ShortcutRecorder(config, changed, toast);
      this.host.append(this.shortcut.element);
    }
    void this.refresh();
  }

  async refresh() {
    const generation = ++this.generation;
    try {
      const snapshot = await invoke<Snapshot>('link_connected');
      if (generation !== this.generation) return;
      const config = this.config();
      const master = this.toggle('Connect with other Arcade apps', config.linkEnabled, false,
        async (on) => { await this.save({ linkEnabled: on }); });
      master.classList.add('arcade-master');
      this.rows.replaceChildren(h('h3', 'Connected apps'), master,
        ...snapshot.peers.map((peer) => {
          const get = h('button.btn', { type: 'button' }, 'Get');
          get.addEventListener('click', async () => {
            get.disabled = true;
            try {
              const url = await invoke<string | null>('link_get', { appId: peer.id });
              if (url) await api.openUrl(url);
            } catch (e) { this.toast(errorMessage(e)); }
            finally { get.disabled = false; }
          });
          const use = peer.installed ? this.toggle('Use with Arcade Look', peer.enabled, !config.linkEnabled,
            async (on) => {
              const disabled = this.config().linkDisabledPeers.filter((id) => id !== peer.id);
              if (!on) disabled.push(peer.id);
              await this.save({ linkDisabledPeers: disabled });
            }, `Use ${peer.name} with Arcade Look`) : null;
          return h('div.arcade-peer', arcadeGlyph(peer.id),
            h('div.arcade-peer-text', h('strong', peer.name), h('small', peer.state),
              !peer.installed ? h('p', peer.pitch) : null),
            h('div.arcade-peer-controls', use, !peer.installed ? get : null));
        }),
        h('details.arcade-diagnostics', h('summary', 'Diagnostics'),
          h('dl', h('dt', 'Registry'), h('dd', snapshot.registry),
            h('dt', 'Endpoint'), h('dd', snapshot.endpoint),
            h('dt', 'Last error'), h('dd', snapshot.lastError ?? 'None'))));
      this.shortcut?.refresh();
    } catch (e) { if (generation === this.generation) this.toast(errorMessage(e)); }
  }

  private toggle(title: string, checked: boolean, disabled: boolean, change: (on: boolean) => Promise<void>, label = title) {
    const input = h('input.switch', { type: 'checkbox', checked, disabled: disabled || this.saving, 'aria-label': label });
    input.addEventListener('change', async () => {
      const want = input.checked;
      if (this.saving) { input.checked = checked; return; }
      this.saving = true;
      for (const toggle of this.rows.querySelectorAll<HTMLInputElement>('input')) toggle.disabled = true;
      try { await change(want); }
      catch (e) { input.checked = !want; this.toast(errorMessage(e)); }
      finally { this.saving = false; await this.refresh(); }
    });
    return h('label.setting-row', h('span', title), input);
  }

  dispose() { this.generation++; this.shortcut?.dispose(); this.host.replaceChildren(); }
}
