// Settings are loaded on demand, keeping the preview shell small.
import { h, clear } from './dom';
import { icon } from './icons';
import { api, errorMessage, type Bootstrap, type Config } from './backend';

export function showSettings(host: HTMLElement, boot: Bootstrap, autostart: boolean,
  setConfig: (patch: Partial<Config>) => Promise<Config>, toast: (text: string) => void,
  showMessage: (title: string, body: string) => void) {
  function switchRow(title: string, detail: string, checked: boolean, change: (on: boolean) => Promise<boolean>) {
    const toggle = h('input.switch', { type: 'checkbox', checked, 'aria-label': title }) as HTMLInputElement;
    toggle.addEventListener('change', async () => {
      const want = toggle.checked;
      toggle.disabled = true;
      try {
        toggle.checked = await change(want);
      } catch (e) {
        toggle.checked = !want;
        toast(errorMessage(e));
      } finally {
        toggle.disabled = false;
      }
    });
    return h('label.setting-row',
      h('div.setting-text', h('div.setting-title', title), h('div.setting-detail', detail)),
      toggle);
  }

  clear(host);
  const themes: [Config['theme'], string][] = [['system', 'System'], ['light', 'Light'], ['dark', 'Dark']];
  const theme = h('div.segmented', { role: 'radiogroup', 'aria-label': 'Theme' });
  const paintTheme = () => {
    for (const b of theme.querySelectorAll<HTMLButtonElement>('button')) {
      const on = b.dataset.value === boot.config.theme;
      b.classList.toggle('active', on);
      b.setAttribute('aria-checked', String(on));
    }
  };
  for (const [value, label] of themes) {
    const b = h('button.seg-btn', { type: 'button', role: 'radio', 'data-value': value }, label);
    b.addEventListener('click', async () => {
      await setConfig({ theme: value }).catch((e) => toast(errorMessage(e)));
      paintTheme();
    });
    theme.append(b);
  }
  paintTheme();

  const chips = () => boot.integration.map(([label, on]) => h(`span.chip${on ? '.on' : ''}`, on ? icon('check', 13) : null, label));
  const integration = h('div.welcome-integration', chips());
  const setup = h('button.btn', 'Set up file manager integration');
  setup.addEventListener('click', async () => {
    setup.disabled = true;
    try {
      showMessage('Integration installed', await api.installIntegration());
    } catch (e) {
      showMessage('Integration failed', errorMessage(e));
    } finally {
      setup.disabled = false;
    }
    // Setting up also turns on start on login.
    const [status, on] = await Promise.all([api.integrationStatus().catch(() => null), api.getAutostart().catch(() => null)]);
    if (status) {
      boot.integration = status;
      integration.replaceChildren(...chips());
    }
    if (on !== null) startOnLogin.querySelector('input')!.checked = on;
  });
  const startOnLogin = switchRow('Start on login',
    'Keep Arcade Look ready in the background, with its icon in the system tray.',
    autostart, (on) => api.setAutostart(on));
  const openPath = (path: string) => api.openDefault(path).catch((err) => toast(errorMessage(err)));
  host.append(
    h('div.settings',
      h('h1', 'Settings'),
      h('section.settings-group',
        h('h3', 'General'),
        startOnLogin,
        h('div.setting-row',
          h('div.setting-text', h('div.setting-title', 'Theme')),
          theme)),
      h('section.settings-group',
        h('h3', 'Previews'),
        switchRow('Play video and audio automatically', 'Start playback as soon as a media file opens.',
          boot.config.autoplay, async (on) => (await setConfig({ autoplay: on })).autoplay),
        switchRow('Show hidden files', 'Include hidden files when flipping through a folder with ← and →.',
          boot.config.showHidden, async (on) => (await setConfig({ showHidden: on })).showHidden)),
      h('section.settings-group',
        h('h3', 'File manager'),
        integration,
        h('div.setting-actions', setup)),
      h('section.settings-group',
        h('h3', 'Advanced'),
        h('div.setting-detail', 'More options (size limits, plugins) live in the config file. Changes apply the next time a preview opens; integration options apply after a restart.'),
        h('div.setting-path', boot.configPath),
        h('div.setting-actions',
          h('button.btn', { onclick: () => void openPath(boot.configPath) }, 'Edit config file'),
          h('button.btn', { onclick: () => void openPath(boot.pluginsDir) }, 'Open plugins folder'))),
      h('section.settings-group#connected-apps'),
      h('div.settings-foot',
        h('span', `Arcade Look v${boot.version}`),
        h('button.btn', { onclick: () => void api.quit() }, 'Quit Arcade Look')),
    ),
  );
}
