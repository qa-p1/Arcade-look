// The native Settings window owns this chunk and releases it when it closes.
import { invoke } from '@tauri-apps/api/core';
import { isPermissionGranted, requestPermission } from '@tauri-apps/plugin-notification';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { api, errorMessage, type Bootstrap, type Config, type Plugin } from './backend';
import { h, nextFrame } from './dom';
import { copyText } from './clipboard';
import { icon } from './icons';
import { arcadeGlyph } from './arcade-glyphs';
import { button, row, switchRow, keyChips } from './controls';
import { ShortcutRecorder } from './shortcut-recorder';
import { ConnectedApps } from './connected-apps';
import shortcuts from '../../docs/user/shortcuts.json';

const GUIDE = 'https://github.com/qa-p1/Arcade-look/blob/main/docs/user/';
const sections = [
  ['General', 'settings'],
  ['Shortcuts', 'keyboard'],
  ['Previews', 'eye'],
  ['File manager', 'folder'],
  ['Plugins', 'engine'],
  ['Connected apps', 'link'],
  ['About', 'info'],
] as const;

export class Settings {
  private boot!: Bootstrap;
  private autostart = false;
  private page = h('main.shell-main', { id: 'settings-page', tabindex: -1 });
  private nav = h('nav.tab-list', {
    role: 'tablist',
    'aria-label': 'Settings',
    'aria-orientation': 'vertical',
  });
  private current = 'General';
  private connected: ConnectedApps | null = null;
  private recorder: ShortcutRecorder | null = null;
  private generation = 0;
  private toastEl = h('div.toast', { role: 'status', 'aria-live': 'polite', hidden: true });
  private toastTimer = 0;
  constructor(private root: HTMLElement) {}

  async start() {
    try {
      [this.boot, this.autostart] = await Promise.all([api.bootstrap(), api.getAutostart()]);
      this.apply(this.boot.config);
      document.documentElement.dataset.platform = this.boot.platform;
      document.body.classList.add('settings-window');
      this.root.classList.add('app-shell');
      for (const [name, glyph] of sections) {
        const tab = h(
          'button.tab-button',
          {
            type: 'button',
            role: 'tab',
            title: name,
            'aria-controls': 'settings-page',
            'aria-selected': false,
            tabindex: -1,
            onclick: () => void this.show(name),
          },
          icon(glyph),
          h('span.label', name),
        );
        this.nav.append(tab);
      }
      this.nav.addEventListener('keydown', (event) => {
        if (!['ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.key)) return;
        event.preventDefault();
        const tabs = [...this.nav.querySelectorAll<HTMLButtonElement>('button')];
        const old = tabs.indexOf(document.activeElement as HTMLButtonElement);
        const index =
          event.key === 'Home'
            ? 0
            : event.key === 'End'
              ? tabs.length - 1
              : (old + (event.key === 'ArrowDown' ? 1 : -1) + tabs.length) % tabs.length;
        tabs[index].focus();
        tabs[index].click();
      });
      this.root.replaceChildren(
        h(
          'aside.sidebar',
          h(
            'div.brand',
            h('span.brand-mark', arcadeGlyph('arcade.look')),
            h('span.brand-name', 'Arcade Look'),
          ),
          this.nav,
          h('div.sidebar-foot', h('div.sidebar-meta.selectable', `Version ${this.boot.version}`)),
        ),
        this.page,
        this.toastEl,
      );
      await listen<Config>('config', (event) => {
        this.apply(event.payload);
        this.recorder?.refresh();
      });
      await listen('link-changed', () => void this.connected?.refresh());
      window.addEventListener('keydown', (event) => {
        const mod = event.ctrlKey || event.metaKey;
        if (event.key === 'Escape' || (mod && event.key.toLowerCase() === 'w')) {
          event.preventDefault();
          void getCurrentWindow().close();
        }
        if (mod && event.key.toLowerCase() === 'q') {
          event.preventDefault();
          void api.quit();
        }
        if (event.key === 'F1') {
          event.preventDefault();
          this.openGuide();
        }
      });
      await this.show('General');
      await document.fonts.ready;
      await nextFrame();
      await api.frontendReady();
      await api.showWindow();
    } catch (error) {
      this.root.replaceChildren(
        h(
          'div.empty-state',
          h('h1', "Couldn't load Settings."),
          h('p', 'Try again to connect to Arcade Look.'),
          button('Retry', () => void this.start()),
        ),
      );
      this.report(error);
    }
  }

  private apply(config: Config) {
    this.boot.config = config;
    if (config.theme === 'system') delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = config.theme;
  }
  private async save(patch: Partial<Config>) {
    const config = await api.setConfig(patch);
    this.apply(config);
    return config;
  }
  private report = (error: unknown) => {
    const details = errorMessage(error);
    void api.log('error', details).catch(() => {});
    this.toast("Couldn't finish the action. Try again.", details);
  };
  private toast(text: string, details?: string) {
    this.toastEl.replaceChildren(
      h('span.message-text', text),
      details
        ? button(
            'Copy details',
            () =>
              void copyText(details)
                .then(() => this.toast('Copied'))
                .catch(this.report),
            'ghost sm',
          )
        : document.createTextNode(''),
      button(
        'Close',
        () => {
          this.toastEl.hidden = true;
        },
        'ghost sm',
      ),
    );
    this.toastEl.hidden = false;
    clearTimeout(this.toastTimer);
    if (!text.startsWith("Couldn't"))
      this.toastTimer = window.setTimeout(
        () => {
          this.toastEl.hidden = true;
        },
        text === 'Copied' ? 1500 : 5000,
      );
  }
  private openGuide() {
    void api.openUrl(`${GUIDE}index.md`).catch(this.report);
  }
  private section(title: string, ...items: HTMLElement[]) {
    return h('section.section', h('div.section-head', h('h2', title)), h('div.list', items));
  }
  private toggle(
    label: string,
    description: string,
    key: 'autoplay' | 'showHidden' | 'plugins' | 'keepReady',
  ) {
    return switchRow(
      label,
      description,
      this.boot.config[key],
      async (on) => (await this.save({ [key]: on }))[key],
      this.report,
    );
  }
  private openPath(path: string) {
    void api.openDefault(path).catch(this.report);
  }

  private async show(name: string) {
    const generation = ++this.generation;
    this.current = name;
    this.connected?.dispose();
    this.connected = null;
    this.recorder?.dispose();
    this.recorder = null;
    for (const tab of this.nav.querySelectorAll<HTMLButtonElement>('button')) {
      const active = tab.title === name;
      tab.classList.toggle('active', active);
      tab.setAttribute('aria-selected', String(active));
      tab.tabIndex = active ? 0 : -1;
    }
    const content = h(
      'div.page-main',
      h('header.page-head', h('div', h('h1', name), h('p.sub', 'Press Space, see anything.'))),
    );
    this.page.replaceChildren(content);
    this.page.scrollTop = 0;
    if (name === 'General') {
      const themes = h('div.segmented', { role: 'radiogroup', 'aria-label': 'Theme' });
      const paint = () => {
        for (const b of themes.querySelectorAll('button')) {
          const active = b.dataset.value === this.boot.config.theme;
          b.classList.toggle('active', active);
          b.setAttribute('aria-checked', String(active));
        }
      };
      for (const [value, label] of [
        ['system', 'System'],
        ['light', 'Light'],
        ['dark', 'Dark'],
      ] as const) {
        themes.append(
          h(
            'button',
            {
              type: 'button',
              role: 'radio',
              'data-value': value,
              onclick: () => {
                void this.save({ theme: value }).then(paint).catch(this.report);
              },
            },
            label,
          ),
        );
      }
      paint();
      content.append(
        this.section('Appearance', row('Theme', 'Follow your system or choose a theme.', themes)),
        this.section(
          'Background',
          switchRow(
            'Start at login',
            'Open in the background, with no window.',
            this.autostart,
            async (on) => {
              this.autostart = await api.setAutostart(on);
              return this.autostart;
            },
            this.report,
          ),
          this.toggle(
            'Keep Arcade Look ready in the background',
            'Keep previews and connections ready between uses.',
            'keepReady',
          ),
        ),
        this.section(
          'Notifications',
          switchRow(
            'Notifications',
            'Notify when a connected app action finishes while Arcade Look is hidden.',
            this.boot.config.notifications,
            async (on) => {
              if (
                on &&
                !(await isPermissionGranted()) &&
                (await requestPermission()) !== 'granted'
              ) {
                throw new Error('Allow notifications in your system settings, then try again.');
              }
              return (await this.save({ notifications: on })).notifications;
            },
            this.report,
          ),
        ),
        this.section(
          'Welcome',
          row(
            'Welcome screen',
            'Set up file manager integration again.',
            button('Open welcome', () => void invoke('open_welcome').catch(this.report)),
          ),
        ),
      );
    } else if (name === 'Shortcuts') {
      this.recorder = new ShortcutRecorder(
        () => this.boot.config,
        (config) => this.apply(config),
        (message) => this.toast(message),
      );
      content.append(
        this.section(
          'Preview selection',
          row(
            'Global shortcut',
            'Record a new shortcut or reset the default.',
            this.recorder.element,
          ),
        ),
      );
      if (!this.boot.shortcutSupported)
        content.append(
          h(
            'div.notice.warn',
            h(
              'p',
              'This desktop needs a keyboard binding. Bind arcade-look --toggle in your keyboard settings.',
            ),
          ),
        );
      for (const group of shortcuts.groups.filter((group) => group.context !== 'global')) {
        content.append(
          this.section(
            group.title,
            ...group.shortcuts.map((shortcut) => {
              const keys = shortcut.keys as Record<string, string | string[] | null>;
              const combo = this.boot.platform in keys ? keys[this.boot.platform] : keys.default;
              return row(shortcut.title, '', combo ? keyChips(combo) : h('span', 'Not available'));
            }),
          ),
        );
      }
    } else if (name === 'Previews') {
      content.append(
        this.section(
          'Files',
          this.toggle(
            'Play video and audio automatically',
            'Start playback when a media file opens.',
            'autoplay',
          ),
          this.toggle(
            'Show hidden files',
            'Include hidden files when browsing with the arrow keys.',
            'showHidden',
          ),
        ),
      );
    } else if (name === 'File manager') {
      const list = h('div.skeleton.skeleton-row', {
        role: 'status',
        'aria-label': 'Loading file manager integration',
      });
      content.append(list);
      try {
        const status = await api.integrationStatus();
        if (generation !== this.generation) return;
        list.className = 'list';
        list.replaceChildren(
          ...status.map(([label, on]) =>
            row(label, '', h(`span.badge${on ? '.ok' : ''}`, on ? 'Connected' : 'Not installed')),
          ),
        );
        content.append(
          button(
            'Set up file manager integration',
            () => {
              void api
                .installIntegration()
                .then(() => this.show(this.current))
                .catch(this.report);
            },
            'primary',
          ),
        );
      } catch {
        if (generation === this.generation)
          list.replaceWith(this.retry('file manager integration', name));
      }
    } else if (name === 'Plugins') {
      content.append(
        this.section(
          'Plugin support',
          this.toggle(
            'Enable plugins',
            'Use installed plugins for additional file formats.',
            'plugins',
          ),
          row(
            'Plugins folder',
            this.boot.pluginsDir,
            button('Open plugins folder', () => this.openPath(this.boot.pluginsDir)),
          ),
        ),
      );
      const list = h('div.skeleton.skeleton-row', {
        role: 'status',
        'aria-label': 'Loading plugins',
      });
      content.append(list);
      try {
        const plugins = await invoke<Plugin[]>('list_plugins');
        if (generation !== this.generation) return;
        list.className = 'section';
        list.replaceChildren(
          plugins.length
            ? h(
                'div.list',
                plugins.map((plugin) =>
                  row(
                    plugin.name,
                    plugin.extensions.join(', '),
                    h(
                      `span.badge${this.boot.config.plugins ? '.ok' : ''}`,
                      this.boot.config.plugins ? 'Enabled' : 'Disabled',
                    ),
                  ),
                ),
              )
            : h(
                'div.empty-state',
                h('h2', 'No plugins yet'),
                h('p', 'Add a plugin to the plugins folder to preview more formats.'),
              ),
        );
      } catch {
        if (generation === this.generation) list.replaceWith(this.retry('plugins', name));
      }
    } else if (name === 'Connected apps') {
      this.connected = new ConnectedApps(
        content,
        () => this.boot.config,
        (patch) => this.save(patch),
        (message) => this.toast(message),
      );
    } else if (name === 'About') {
      const about = await invoke<{ channel: string; tools: boolean; logs: string }>(
        'about_info',
      ).catch(() => null);
      if (generation !== this.generation) return;
      if (!about) {
        content.append(this.retry('About', name));
        return;
      }
      content.append(
        this.section(
          'Arcade Look',
          row(
            `Version ${this.boot.version}`,
            about.channel,
            h('span.selectable', 'Press Space, see anything.'),
          ),
          row(
            about.tools ? 'Updates are managed by Arcade Tools' : 'Updates',
            '',
            button(about.tools ? 'Open Tools' : 'Check for updates', () => {
              if (about.tools)
                void invoke('link_open', { appId: 'arcade.tools' }).catch(this.report);
              else
                void api
                  .openUrl('https://github.com/qa-p1/Arcade-look/releases/latest')
                  .catch(this.report);
            }),
          ),
          row(
            'User guide',
            '',
            button('Open user guide', () => this.openGuide()),
          ),
          row(
            'Logs',
            about.logs,
            button('Open logs folder', () => this.openPath(about.logs)),
          ),
          row(
            'Diagnostics',
            'App, platform and connection status. No file content.',
            button('Copy diagnostics', () => {
              void invoke<string>('diagnostics')
                .then((text) => navigator.clipboard.writeText(text))
                .then(() => this.toast('Copied'))
                .catch(this.report);
            }),
          ),
          row(
            'Licences',
            'MIT · Inter SIL Open Font License · third-party notices',
            button('Open licences', () => {
              void api
                .openUrl('https://github.com/qa-p1/Arcade-look/blob/main/LICENSE')
                .catch(this.report);
            }),
          ),
          row(
            'Config file',
            this.boot.configPath,
            button('Edit config file', () => this.openPath(this.boot.configPath)),
          ),
          row(
            'Arcade Look',
            'Close the background service and all its windows.',
            button('Quit Arcade Look', () => void api.quit(), 'danger'),
          ),
        ),
      );
    }
  }
  private retry(thing: string, name: string) {
    return h(
      'div.notice.danger',
      h('div.row-main', `Couldn't load ${thing}. Try again.`),
      button('Retry', () => void this.show(name)),
    );
  }
}
