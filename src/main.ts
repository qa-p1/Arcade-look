import './styles/app.css';
import { invoke } from '@tauri-apps/api/core';
import { App } from './app';

// Forward problems to the backend (printed when ALOOK_DEBUG=1) — invaluable for bug reports.
const forward = (level: string, parts: unknown[]) => {
  const message = parts.map((p) => (p instanceof Error ? `${p.message}\n${p.stack ?? ''}` : typeof p === 'string' ? p : JSON.stringify(p))).join(' ');
  invoke('log', { level, message }).catch(() => {});
};
for (const level of ['error', 'warn'] as const) {
  const orig = console[level].bind(console);
  console[level] = (...a: unknown[]) => {
    orig(...a);
    forward(level, a);
  };
}
window.addEventListener('error', (e) => forward('error', [e.error ?? e.message]));
window.addEventListener('unhandledrejection', (e) => forward('rejection', [e.reason]));

const root = document.getElementById('app');
if (root) void new App(root).start();
