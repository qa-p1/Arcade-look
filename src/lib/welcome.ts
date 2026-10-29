// Welcome and shortcut help load only when requested.
import { open as chooseFile } from '@tauri-apps/plugin-dialog';
import { h, clear } from './dom';
import { icon } from './icons';
import { arcadeGlyph } from './arcade-glyphs';
import { api, errorMessage, type Bootstrap } from './backend';
import { button } from './controls';

export function showWelcome(
  host: HTMLElement,
  boot: Bootstrap,
  showMessage: (title: string, body: string) => void,
  toast: (message: string) => void,
  openSettings: () => void,
  preview: (path: string) => Promise<void>,
) {
  clear(host);
  const integration = h(
    'div.welcome-integration',
    boot.integration.map(([label, on]) =>
      h(`span.badge${on ? '.ok' : ''}`, on ? icon('check') : null, label),
    ),
  );
  const setup = button(
    'Set up file manager integration',
    () => {
      setup.disabled = true;
      void api
        .installIntegration()
        .then((message) => showMessage('File manager integration', message))
        .catch(() =>
          showMessage(
            "Couldn't set up file manager integration.",
            'Check the user guide and try again.',
          ),
        )
        .finally(() => {
          setup.disabled = false;
        });
    },
    'primary',
  );
  host.append(
    h(
      'div.welcome',
      h(
        'div.empty-state',
        h('span.glyph-tile.lg', arcadeGlyph('arcade.look')),
        h('h1', 'Arcade Look'),
        h('p', 'Press Space, see anything.'),
        integration,
        h(
          'div.btn-group',
          setup,
          button('Open a file…', () => {
            void chooseFile({ multiple: false, directory: false })
              .then((path) => (path ? preview(path) : undefined))
              .catch((error) => toast(`Couldn't open the file. ${errorMessage(error)}`));
          }),
        ),
        h('p', 'Drop any file or folder here to preview it.'),
        h('div.welcome-foot', `Version ${boot.version}`, button('Settings', openSettings, 'ghost')),
      ),
    ),
  );
}

export function showHelp(root: HTMLElement, closeHelp: () => void): HTMLElement {
  const groups: [string, [string, string][]][] = [
    [
      'Everywhere',
      [
        ['Space / Esc', 'Close the preview'],
        ['← / →', 'Previous / next file in the folder'],
        ['Backspace', 'Go back'],
        ['Enter', 'Open with default app'],
        ['Ctrl/⌘ R', 'Show in folder'],
        ['Ctrl/⌘ C', 'Copy path'],
        ['A', 'Connected actions'],
        ['I', 'Info panel'],
        ['F', 'Fullscreen'],
        ['Ctrl/⌘ ,', 'Settings'],
        ['Ctrl/⌘ Q', 'Quit Arcade Look'],
      ],
    ],
    [
      'Images · PDF · Text',
      [
        ['+ / −', 'Zoom in / out'],
        ['0', 'Fit / reset zoom'],
        ['1', 'Actual size'],
        ['R', 'Rotate image'],
        ['W', 'Toggle line wrap'],
      ],
    ],
    [
      'Video · Audio',
      [
        ['K', 'Play / pause'],
        ['J / L', 'Back / forward 10 s'],
        ['M', 'Mute'],
        [', / .', 'Slower / faster'],
      ],
    ],
    [
      '3D models',
      [
        ['Drag', 'Orbit'],
        ['Scroll', 'Zoom'],
        ['W', 'Wireframe'],
        ['A / Shift+A', 'Auto-rotate'],
      ],
    ],
  ];
  const help = h(
    'div.modal-backdrop',
    { onclick: closeHelp },
    h(
      'div.dialog.help',
      {
        role: 'dialog',
        'aria-label': 'Keyboard shortcuts',
        onclick: (event: Event) => event.stopPropagation(),
      },
      h('h2', 'Keyboard shortcuts'),
      h(
        'div.help-grid',
        groups.map(([name, rows]) =>
          h(
            'section',
            h('h3', name),
            rows.map(([key, label]) => h('div.key-row', h('kbd.key-chip', key), h('span', label))),
          ),
        ),
      ),
      h('div.dialog-actions', button('Close', closeHelp, 'primary')),
    ),
  );
  root.append(help);
  return help;
}
