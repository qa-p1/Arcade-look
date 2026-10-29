import { invoke } from '@tauri-apps/api/core';
import { h } from './dom';
import { errorMessage, type Config } from './backend';
import { button, keyChips } from './controls';

// A valid capture applies immediately; conflicts wait for an explicit choice.
export class ShortcutRecorder {
  readonly element = h('div');
  private record = h('button.shortcut-recorder', {
    type: 'button',
    'aria-label': 'Record global shortcut',
  });
  private warning = h('div', { role: 'status', 'aria-live': 'polite' });
  private generation = 0;
  private recording = false;
  private value = '';

  constructor(
    private config: () => Config,
    private changed: (config: Config) => void,
    private toast: (message: string) => void,
  ) {
    this.record.addEventListener('click', () => this.begin());
    this.record.addEventListener('keydown', (event) => {
      if (!this.recording) return;
      event.stopPropagation();
      if (event.key === 'Tab') {
        this.end();
        return;
      }
      event.preventDefault();
      if (event.key === 'Escape') {
        this.end();
        return;
      }
      if (event.key === 'Backspace') {
        this.value = '';
        this.end();
        void this.apply();
        return;
      }
      if (event.repeat || ['Control', 'Alt', 'Shift', 'Meta'].includes(event.key)) return;
      if (
        !event.ctrlKey &&
        !event.altKey &&
        !event.metaKey &&
        !/^F(?:[1-9]|1\d|2[0-4])$/.test(event.key)
      ) {
        this.warning.textContent = 'Add Ctrl, Alt or Super, or use F1–F24.';
        return;
      }
      const key = event.code.replace(/^(Key|Digit)/, '');
      this.value = [
        event.ctrlKey ? 'Ctrl' : '',
        event.altKey ? 'Alt' : '',
        event.shiftKey ? 'Shift' : '',
        event.metaKey ? 'Super' : '',
        key,
      ]
        .filter(Boolean)
        .join('+');
      this.end();
      void this.check();
    });
    this.record.addEventListener('blur', () => this.end());
    this.element.append(
      h(
        'div.btn-group',
        this.record,
        button(
          'Reset to default',
          () => {
            this.value = 'Ctrl+Alt+Shift+Space';
            void this.check();
          },
          'ghost',
        ),
      ),
      this.warning,
    );
    this.refresh();
  }
  private begin() {
    this.recording = true;
    this.warning.replaceChildren();
    this.record.classList.add('recording');
    this.record.textContent = 'Press keys…';
    void invoke('link_shortcut_recording', { recording: true });
  }
  private end() {
    this.recording = false;
    this.record.classList.remove('recording');
    void invoke('link_shortcut_recording', { recording: false });
    this.paint();
  }
  private paint() {
    this.record.replaceChildren(this.value ? keyChips(this.value) : h('span', 'No shortcut'));
  }
  refresh() {
    if (!this.recording) {
      this.value = this.config().globalShortcut;
      this.paint();
    }
  }
  private async check() {
    const generation = ++this.generation;
    this.record.disabled = true;
    try {
      const owner = this.value
        ? await invoke<string | null>('link_shortcut_owner', { accelerator: this.value })
        : null;
      if (generation !== this.generation) return;
      this.paint();
      if (owner)
        this.warning.replaceChildren(
          h(
            'div.notice.warn',
            h('div.row-main', `Used by ${owner}`),
            button('Use anyway', () => void this.apply(), 'sm'),
            button('Choose another', () => this.begin(), 'ghost sm'),
          ),
        );
      else await this.apply();
    } catch (error) {
      this.toast(`Couldn't check the shortcut. ${errorMessage(error)}`);
    } finally {
      this.record.disabled = false;
    }
  }
  private async apply() {
    this.record.disabled = true;
    try {
      this.changed(await invoke<Config>('link_save_shortcut', { accelerator: this.value }));
      this.warning.replaceChildren();
      this.refresh();
    } catch (error) {
      this.toast(`Couldn't change the shortcut. ${errorMessage(error)}`);
      this.refresh();
    } finally {
      this.record.disabled = false;
    }
  }
  dispose() {
    this.generation++;
    this.end();
  }
}
