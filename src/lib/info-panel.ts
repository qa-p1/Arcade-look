import { clear, h, append } from './dom';
import * as fmt from './format';
import { kindLabel } from './kinds';
import { kindBadge, icon } from './icons';
import { copyText } from './clipboard';
import type { FileInfo } from './backend';

export function renderInfo(
  host: HTMLElement,
  i: FileInfo,
  details: [string, string][],
  toast: (message: string) => void,
) {
  clear(host);
  const row = (key: string, value: string | null | undefined, cls = '') =>
    value
      ? h(
          'div.row',
          h(
            'div.row-main',
            h('div.row-sub', key),
            h(`div.info-val.selectable${cls ? `.${cls}` : ''}`, value),
          ),
          h(
            'button.btn.ghost.icon-only',
            {
              type: 'button',
              title: `Copy ${key.toLowerCase()}`,
              'aria-label': `Copy ${key.toLowerCase()}`,
              onclick: () => {
                void copyText(value)
                  .then(() => toast('Copied'))
                  .catch(() => toast("Couldn't copy the value. Try again."));
              },
            },
            icon('copy'),
          ),
        )
      : null;
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
    h('div.info-head', kindBadge(i.kind, i.ext, 22), h('div.info-name', { title: i.name }, i.name)),
    h('section.info-section', h('h3', 'General'), general),
    details.length
      ? h(
          'section.info-section',
          h('h3', 'Details'),
          details.map(([k, v]) => row(k, v)),
        )
      : null,
  ]);
}

export function missingInfo(path: string): FileInfo {
  const name = path.split(/[\\/]/).pop() || path;
  return {
    path,
    name,
    dir: null,
    ext: '',
    size: 0,
    modified: null,
    created: null,
    accessed: null,
    readonly: false,
    hidden: false,
    symlink: null,
    mode: null,
    kind: 'binary',
    format: '',
    lang: null,
    mime: 'application/octet-stream',
    plugin: null,
    fallbackPlugin: null,
  };
}
