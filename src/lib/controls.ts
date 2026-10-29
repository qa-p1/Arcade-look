// DOM adapters for the kit's controls. Their styles live in src/design/arcade.css.
import { h } from './dom';

export function button(label: string, action: () => void, variant = '') {
  return h('button.btn', { type: 'button', class: variant, onclick: action }, label);
}

export function row(label: string, description: string, control: HTMLElement) {
  return h(
    'div.setting-row',
    h(
      'div.setting-text',
      h('div.setting-label', label),
      description ? h('div.setting-desc', description) : null,
    ),
    control,
  );
}

export function switchRow(
  label: string,
  description: string,
  checked: boolean,
  change: (on: boolean) => Promise<boolean>,
  report: (error: unknown) => void,
) {
  const input = h('input.switch', { type: 'checkbox', checked, 'aria-label': label });
  input.addEventListener('change', async () => {
    const wanted = input.checked;
    input.disabled = true;
    try {
      input.checked = await change(wanted);
    } catch (error) {
      input.checked = !wanted;
      report(error);
    } finally {
      input.disabled = false;
    }
  });
  return h(
    'label.setting-row.switch-row',
    h('div.setting-text', h('div.setting-label', label), h('div.setting-desc', description)),
    input,
  );
}

export function keyChips(combo: string | string[]): HTMLElement {
  if (Array.isArray(combo)) {
    return h(
      'span.key-chip-list',
      combo.map((keys, index) => [index ? h('span.key-or', 'or') : null, keyChips(keys)]),
    );
  }
  return h(
    'span.key-chip-list',
    combo.split('+').map((key) => h('kbd.key-chip', key)),
  );
}
