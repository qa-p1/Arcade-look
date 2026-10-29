// Shared kit tooltip behavior, delegated so lazy viewer controls use it too.
import { h } from './dom';
import { keyChips } from './controls';

export function installTooltips() {
  let active: HTMLElement | null = null;
  let tooltip: HTMLElement | null = null;
  let timer = 0;
  const hide = () => {
    clearTimeout(timer);
    if (active) active.removeAttribute('aria-describedby');
    tooltip?.remove();
    tooltip = null;
    active = null;
  };
  const show = (target: EventTarget | null) => {
    const button = target instanceof Element ? target.closest<HTMLElement>('button[title]') : null;
    if (!button || button === active) return;
    hide();
    active = button;
    const title = button.title;
    const match = title.match(/^(.*?)\s+\(([^)]+)\)$/);
    const delay =
      parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--tooltip-delay')) ||
      500;
    timer = window.setTimeout(() => {
      if (!button.isConnected || active !== button) return;
      tooltip = h(
        'div.tooltip',
        { role: 'tooltip', id: 'arcade-control-tooltip' },
        h('span', match?.[1] ?? title),
        match ? keyChips(match[2].split(' / ')) : null,
      );
      document.body.append(tooltip);
      button.setAttribute('aria-describedby', tooltip.id);
      const bounds = button.getBoundingClientRect();
      const box = tooltip.getBoundingClientRect();
      tooltip.style.left = `${Math.max(8, Math.min(innerWidth - box.width - 8, bounds.left + (bounds.width - box.width) / 2))}px`;
      tooltip.style.top = `${bounds.bottom + box.height + 8 < innerHeight ? bounds.bottom + 8 : Math.max(8, bounds.top - box.height - 8)}px`;
    }, delay);
  };
  document.addEventListener('pointerover', (event) => show(event.target));
  document.addEventListener('focusin', (event) => show(event.target));
  document.addEventListener('pointerout', (event) => {
    if (active && !(event.relatedTarget instanceof Node && active.contains(event.relatedTarget)))
      hide();
  });
  document.addEventListener('focusout', hide);
  document.addEventListener('pointerdown', hide);
  window.addEventListener('blur', hide);
  window.addEventListener('resize', hide);
}
