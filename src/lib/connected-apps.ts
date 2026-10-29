// Settings-only chunk: promotion never appears in previews or their actions.
import { invoke } from '@tauri-apps/api/core';
import { h } from './dom';
import { arcadeGlyph } from './arcade-glyphs';
import { api, errorMessage, type Config } from './backend';
import { button, switchRow } from './controls';

interface Peer {
  id: string;
  name: string;
  state: string;
  installed: boolean;
  enabled: boolean;
  pitch: string;
}
interface Snapshot {
  peers: Peer[];
  registry: string;
  endpoint: string;
  lastError: string | null;
  activity: string[];
}

export class ConnectedApps {
  private generation = 0;
  private rows = h('div');
  constructor(
    private host: HTMLElement,
    private config: () => Config,
    private save: (patch: Partial<Config>) => Promise<Config>,
    private toast: (message: string) => void,
  ) {
    this.host.append(this.rows);
    void this.refresh();
  }

  async refresh() {
    const generation = ++this.generation;
    this.rows.replaceChildren(
      h('div.skeleton.skeleton-row', { 'aria-label': 'Loading connected apps', role: 'status' }),
    );
    try {
      const snapshot = await invoke<Snapshot>('link_connected');
      if (generation !== this.generation) return;
      const config = this.config();
      const report = (error: unknown) =>
        this.toast(`Couldn't update connections. ${errorMessage(error)}`);
      const master = switchRow(
        config.linkEnabled ? 'Connect with other Arcade apps' : 'Connections are off in this app',
        'Choose which apps can work with Look.',
        config.linkEnabled,
        async (on) => {
          await this.save({ linkEnabled: on });
          await this.refresh();
          return on;
        },
        report,
      );
      const peers = h(
        'div.list',
        snapshot.peers.map((peer) => {
          const state = !peer.installed
            ? 'Not installed'
            : peer.state === 'Connected' && config.linkEnabled && peer.enabled
              ? 'Connected'
              : 'Not running';
          const action = peer.installed
            ? button('Open', () => void invoke('link_open', { appId: peer.id }).catch(report), 'sm')
            : button(
                'Install',
                () => {
                  void invoke<string | null>('link_get', { appId: peer.id })
                    .then((url) => (url ? api.openUrl(url) : undefined))
                    .catch(report);
                },
                'sm',
              );
          const input = h('input.switch', {
            type: 'checkbox',
            checked: peer.enabled,
            disabled: !config.linkEnabled,
            'aria-label': `Use ${peer.name} with Arcade Look`,
          });
          input.addEventListener('change', async () => {
            const disabled = this.config().linkDisabledPeers.filter((id) => id !== peer.id);
            if (!input.checked) disabled.push(peer.id);
            input.disabled = true;
            try {
              await this.save({ linkDisabledPeers: disabled });
            } catch (error) {
              report(error);
            } finally {
              await this.refresh();
            }
          });
          return h(
            'div.row',
            h('span.glyph-tile', arcadeGlyph(peer.id)),
            h(
              'div.row-main',
              h('div.row-title', { title: peer.name }, peer.name),
              h('div.row-sub', peer.pitch),
            ),
            h(
              'div.row-end',
              h(`span.badge${state === 'Connected' ? '.ok' : ''}`, state),
              input,
              action,
            ),
          );
        }),
      );
      const activity = h(
        'details.section',
        h('summary', 'Recent activity'),
        snapshot.activity?.length
          ? h(
              'ul.activity-list',
              snapshot.activity.map((text) => h('li', text)),
            )
          : h(
              'div.empty-state',
              h('p', 'No activity yet'),
              h('p', 'Use a connected action to see it here.'),
            ),
      );
      this.rows.replaceChildren(
        h('div.list', master),
        snapshot.peers.length
          ? h('section.section', peers)
          : h(
              'div.empty-state',
              h('h2', 'No connected apps yet'),
              h('p', 'Install another Arcade app to work with Look.'),
            ),
        activity,
        button(
          'How it works',
          () =>
            void api.openUrl(
              'https://github.com/qa-p1/Arcade-look/blob/main/docs/user/with-other-apps.md',
            ),
          'ghost',
        ),
        h(
          'details.section',
          h('summary', 'Diagnostics'),
          h(
            'dl.selectable',
            h('dt', 'Registry'),
            h('dd.path', snapshot.registry),
            h('dt', 'Endpoint'),
            h('dd', snapshot.endpoint),
            h('dt', 'Last error'),
            h('dd.path', snapshot.lastError ?? 'None'),
          ),
        ),
      );
    } catch {
      if (generation === this.generation)
        this.rows.replaceChildren(
          h(
            'div.notice.danger',
            h('div.row-main', "Couldn't load connected apps. Try again."),
            button('Retry', () => void this.refresh()),
          ),
        );
    }
  }

  dispose() {
    this.generation++;
    this.host.replaceChildren();
  }
}
