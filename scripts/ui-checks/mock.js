// Native IPC fixture for the real compiled UI. Native integration has its own runner.
(() => {
  const callbacks = new Map(), listeners = new Map();
  let next = 1;
  const config = { theme: 'system', idleMinutes: 10, keepReady: true, notifications: false, globalShortcut: 'Ctrl+Alt+Shift+Space', explorerSpace: true, nautilusPreviewer: true, textLimitMb: 4, highlightLimitKb: 512, autoplay: true, plugins: true, showHidden: false, linkEnabled: true, linkDisabledPeers: [] };
  const longName = 'Getting started — a deliberately long file name to verify truncation and keyboard access.md';
  const file = { path: '/fixtures/' + longName, name: longName, dir: '/fixtures', ext: 'md', size: 12400, modified: Date.now(), created: null, accessed: null, readonly: false, hidden: false, symlink: null, mode: null, kind: 'markdown', format: 'markdown', lang: null, mime: 'text/markdown', plugin: null, fallbackPlugin: null };
  const send = (name, payload) => { for (const id of listeners.get(name) ?? []) callbacks.get(id)?.({ event: name, id: 1, payload }); };
  window.__mock = { calls: [], fail: [], config, file, send };
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: location.search.includes('settings') ? 'settings' : 'main' }, currentWebview: { label: location.search.includes('settings') ? 'settings' : 'main' } },
    transformCallback(callback) { const id = next++; callbacks.set(id, callback); return id; },
    unregisterCallback(id) { callbacks.delete(id); },
    convertFileSrc(path) { return location.origin + '/' + path; },
    async invoke(command, args = {}) {
      window.__mock.calls.push([command, args]);
      if (window.__mock.fail.includes(command)) throw new Error('Fixture error');
      if (command === 'plugin:event|listen') { const ids = listeners.get(args.event) ?? []; ids.push(args.handler); listeners.set(args.event, ids); return next++; }
      if (command.startsWith('plugin:')) return null;
      if (command === 'bootstrap') return { platform: 'linux', version: '0.4.0', config: { ...config }, configPath: '/home/user/.config/arcade/look/settings.json', pluginsDir: '/home/user/.config/arcade/look/plugins', infoPanel: false, service: true, debug: false, mediaBase: null, integration: [['GNOME Files', true], ['Dolphin', true], ['Nemo', false]], shortcutSupported: true };
      if (command === 'get_autostart') return false;
      if (command === 'frontend_ready') { if (!location.search.includes('settings')) setTimeout(() => send('open', file.path), 0); return; }
      if (command === 'inspect') return { ...file, path: args.path };
      if (command === 'render_markdown') return { html: '<h1>Getting started</h1><p>Press Space, see anything.</p><h2>One file, one preview</h2><p>Select a file in your file manager and press <kbd>Space</kbd>. Browse images, documents, media, code and archives without opening a full editor.</p><h2>Keep your place</h2><ul><li>Use the arrow keys to move between files.</li><li>Press I to inspect metadata.</li><li>Press A for connected actions.</li></ul><pre><code>arcade-look --settings</code></pre>', truncated: false };
      if (command === 'link_available') return true;
      if (command === 'link_actions') return [];
      if (command === 'neighbor') return null;
      if (command === 'set_config') { Object.assign(config, args.patch); send('config', { ...config }); return { ...config }; }
      if (command === 'link_save_shortcut') { config.globalShortcut = args.accelerator; send('config', { ...config }); return { ...config }; }
      if (command === 'link_shortcut_owner') return args.accelerator === 'Ctrl+Alt+F' ? 'Arcade Find' : null;
      if (command === 'integration_status') return [['GNOME Files', true], ['Dolphin', true], ['Nemo', false]];
      if (command === 'list_plugins') return [];
      if (command === 'about_info') return { channel: 'stable', tools: false, logs: '/home/user/.local/share/arcade/look/logs' };
      if (command === 'link_connected') return { peers: ['box', 'find', 'lens', 'shelf', 'clipboard', 'wheel'].map((name, index) => ({ id: 'arcade.' + name, name: 'Arcade ' + name[0].toUpperCase() + name.slice(1), state: index === 0 ? 'Connected' : 'Not running', installed: index < 3, enabled: true, pitch: 'Work with Look using private, selected-file handoffs.' })), registry: '/home/user/.local/share/arcade/link/apps', endpoint: 'Listening', lastError: null, activity: [] };
      if (command === 'diagnostics') return 'Arcade Look 0.4.0 (stable)\nLink: protocol 1\n';
      return null;
    },
  };
})();
