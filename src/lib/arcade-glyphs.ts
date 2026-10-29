// Trusted, unchanged app marks copied by scripts/sync-design.py.
import { svg } from './dom';
const glyphs = import.meta.glob('../design/glyphs/*.svg', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;

export function arcadeGlyph(app: string): SVGElement {
  const el = svg(
    glyphs[`../design/glyphs/${app}.svg`] ?? glyphs['../design/glyphs/arcade.look.svg'],
  );
  el.setAttribute('width', '16');
  el.setAttribute('height', '16');
  el.setAttribute('aria-hidden', 'true');
  return el;
}
