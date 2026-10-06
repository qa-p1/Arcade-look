import { clear, h, append } from './dom';
import * as fmt from './format';
import { kindLabel } from './kinds';
import { kindBadge } from './icons';
import type { FileInfo } from './backend';

export function renderInfo(host: HTMLElement, i: FileInfo, details: [string, string][]) {
  clear(host);
  const row = (k: string, v: string | null | undefined, cls = '') =>
    v ? h('div.info-row', h('div.info-key', k), h(`div.info-val${cls ? `.${cls}` : ''}`, v)) : null;
  const general = [
    row('Kind', kindLabel(i)),
    i.kind !== 'folder' ? row('Size', fmt.bytes(i.size, true)) : null,
    row('Created', i.created ? fmt.date(i.created) : null),
    row('Modified', fmt.date(i.modified)),
    row('Where', i.dir, 'mono'),
    row('Type', i.mime, 'mono'),
    i.mode !== null ? row('Permissions', fmt.permissions(i.mode), 'mono') : null,
    i.symlink !== null ? row('Link to', i.symlink, 'mono') : null,
    i.readonly ? row('Access', 'Read-only') : null,
    i.hidden ? row('Visibility', 'Hidden') : null,
    i.plugin ? row('Plugin', i.plugin.name) : null,
  ];
  append(host, [
    h('div.info-head', kindBadge(i.kind, i.ext, 22), h('div.info-name', i.name)),
    h('section.info-section', h('h3', 'General'), general),
    details.length
      ? h('section.info-section', h('h3', 'Details'), details.map(([k, v]) => row(k, v)))
      : null,
  ]);
  }

export function missingInfo(path: string): FileInfo {
  const name = path.split(/[\\/]/).pop() || path;
  return {
  path, name, dir: null, ext: '', size: 0, modified: null, created: null, accessed: null,
  readonly: false, hidden: false, symlink: null, mode: null, kind: 'binary', format: '',
  lang: null, mime: 'application/octet-stream', plugin: null, fallbackPlugin: null,
  };
}
