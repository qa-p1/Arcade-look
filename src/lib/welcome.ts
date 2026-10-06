// Welcome and shortcut help load only when requested.
import { h, clear } from './dom';
import { icon } from './icons';
import { api, errorMessage, type Bootstrap } from './backend';

export function showWelcome(host: HTMLElement, boot: Bootstrap,
  showMessage: (title: string, body: string) => void, toast: (message: string) => void, openSettings: () => void) {
  clear(host);
  const integration = h('div.welcome-integration',
    boot.integration.map(([label, on]) => h(`span.chip${on ? '.on' : ''}`, on ? icon('check', 13) : null, label)),
  );
  const setup = h('button.btn.primary', 'Set up file manager integration');
  setup.addEventListener('click', async () => {
    setup.disabled = true;
    try {
      const msg = await api.installIntegration();
      showMessage('Integration installed', msg);
    } catch (e) {
      showMessage('Integration failed', errorMessage(e));
    } finally {
      setup.disabled = false;
    }
  });
  const keys: [string, string][] = [
    ['Space / Esc', 'Close'], ['← →', 'Previous / next file'], ['Enter', 'Open with default app'],
    ['I', 'Info panel'], ['F', 'Fullscreen'], ['+ − 0', 'Zoom'], ['?', 'All shortcuts'], ['Ctrl Q', 'Quit'],
  ];
  const link = (label: string, path: string) =>
    h('a.link', { href: '#', onclick: (e: Event) => { e.preventDefault(); api.openDefault(path).catch((err) => toast(errorMessage(err))); } }, label);
  host.append(
    h('div.welcome',
      h('div.welcome-logo', icon('eye', 44)),
      h('h1', 'Arcade Look'),
      h('p.welcome-tag', 'Select a file, press Space, see it instantly.'),
      h('div.dropzone', icon('open', 20), h('span', 'Drop any file or folder here')),
      h('div.keys', keys.map(([k, v]) => h('div.key-row', h('kbd', k), h('span', v)))),
      integration,
      setup,
      h('div.welcome-foot',
        h('span', `v${boot.version}`), ' · ',
        h('a.link', { href: '#', onclick: (e: Event) => { e.preventDefault(); void openSettings(); } }, 'Settings'), ' · ',
        link('Plugins folder', boot.pluginsDir)),
    ),
  );
  }

export function showHelp(root: HTMLElement, closeHelp: () => void): HTMLElement {
  const groups: [string, [string, string][]][] = [
    ['Everywhere', [['Space / Esc', 'Close the preview'], ['← / →', 'Previous / next file in the folder'], ['Backspace', 'Back (after opening an item)'], ['Enter', 'Open with the default app'], ['Ctrl/⌘ R', 'Show in folder'], ['Ctrl/⌘ C', 'Copy path'], ['A', 'Connected actions (when available)'], ['I', 'Toggle info panel'], ['F', 'Toggle fullscreen'], ['Ctrl/⌘ Q', 'Quit Arcade Look']]],
    ['Images · PDF · Text', [['+ / −', 'Zoom in / out'], ['0', 'Fit / reset zoom'], ['1', 'Actual size'], ['R', 'Rotate image'], ['W', 'Toggle line wrap']]],
    ['Video · Audio', [['K', 'Play / pause'], ['J / L', 'Back / forward 10 s'], ['M', 'Mute'], [', / .', 'Slower / faster']]],
    ['3D models', [['Drag', 'Orbit'], ['Scroll', 'Zoom'], ['W', 'Wireframe'], ['A / Shift+A', 'Auto-rotate (Shift+A with connected actions)']]],
  ];
  const help = h('div.modal-backdrop', { onclick: () => closeHelp() },
    h('div.modal.help', { onclick: (e: Event) => e.stopPropagation() },
      h('h2', icon('keyboard', 20), 'Keyboard shortcuts'),
      h('div.help-grid', groups.map(([g, rows]) => h('section', h('h3', g), rows.map(([k, v]) => h('div.key-row', h('kbd', k), h('span', v))))))));
  root.append(help);
  return help;
}
