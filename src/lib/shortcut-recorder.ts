import { invoke } from '@tauri-apps/api/core';
import { h } from './dom';
import { errorMessage, type Config } from './backend';

export class ShortcutRecorder {
  readonly element = h('section.arcade-shortcut');
  private input = h('input', { type: 'text', readonly: true, 'aria-label': 'Global shortcut', placeholder: 'Click to record a shortcut' });
  private warning = h('p', { role: 'status', 'aria-live': 'polite' });
  private save = h('button.btn', { type: 'button', disabled: true }, 'Save shortcut');
  private value: string;
  private generation = 0;
  private recording = false;

  constructor(private config: () => Config, private changed: (config: Config) => void, private toast: (message: string) => void) {
    this.value = config().globalShortcut;
    this.input.value = this.value;
    const clear = h('button.btn', { type: 'button' }, 'Clear');
    this.input.addEventListener('focus', () => {
      this.recording = true;
      void invoke('link_shortcut_recording', { recording: true });
    });
    this.input.addEventListener('blur', () => {
      this.recording = false;
      void invoke('link_shortcut_recording', { recording: false });
    });
    this.input.addEventListener('keydown', (e) => {
      e.stopPropagation();
      if (e.key === 'Tab') return;
      e.preventDefault();
      if (e.key === 'Escape') { this.input.blur(); return; }
      if (e.repeat || ['Control', 'Alt', 'Shift', 'Meta'].includes(e.key) || !e.code) return;
      const key = e.code.replace(/^(Key|Digit)/, '');
      this.value = [e.ctrlKey ? 'Ctrl' : '', e.altKey ? 'Alt' : '', e.shiftKey ? 'Shift' : '', e.metaKey ? 'Super' : '', key].filter(Boolean).join('+');
      this.input.value = this.value;
      this.save.disabled = this.value === this.config().globalShortcut;
      void this.check();
    });
    clear.addEventListener('click', () => { this.value = ''; this.input.value = ''; this.save.disabled = this.config().globalShortcut === ''; void this.check(); });
    this.save.addEventListener('click', async () => {
      this.save.disabled = true;
      this.input.disabled = true;
      clear.disabled = true;
      try {
        const config = await invoke<Config>('link_save_shortcut', { accelerator: this.value });
        this.changed(config);
        this.toast('Shortcut saved. Restart Arcade Look to apply it.');
      } catch (e) { this.toast(errorMessage(e)); }
      finally { this.input.disabled = false; clear.disabled = false; this.save.disabled = this.value === this.config().globalShortcut; }
    });
    this.element.append(h('h3', 'Global shortcut'),
      h('p.setting-detail', 'Preview the file manager’s selection. Saved changes apply after restarting Arcade Look.'),
      h('div.setting-actions', this.input, this.save, clear), this.warning);
    void this.check();
  }

  refresh() { if (!this.recording) void this.check(); }
  private async check() {
    const generation = ++this.generation;
    try {
      const owner = await invoke<string | null>('link_shortcut_owner', { accelerator: this.value });
      if (generation === this.generation) this.warning.textContent = owner ? `Used by ${owner}` : '';
    } catch (e) { if (generation === this.generation) this.warning.textContent = errorMessage(e); }
  }
  dispose() { this.generation++; void invoke('link_shortcut_recording', { recording: false }); }
}
