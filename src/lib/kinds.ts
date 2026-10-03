import type { FileInfo } from './backend';
import { LANG_NAMES } from './highlight';

/** Human-readable kind, e.g. "PNG image", "Rust source". */
export function kindLabel(i: FileInfo): string {
  const F = i.format.toUpperCase();
  switch (i.kind) {
    case 'folder': return 'Folder';
    case 'image': case 'image-decode': return `${F} image`;
    case 'image-raw': return `${F} camera RAW`;
    case 'image-psd': return 'Photoshop document';
    case 'svg': return 'SVG image';
    case 'video': return `${F} video`;
    case 'audio': return `${F} audio`;
    case 'pdf': return 'PDF document';
    case 'markdown': return 'Markdown';
    case 'code': return i.lang && i.lang !== 'plaintext' ? `${LANG_NAMES[i.lang] ?? i.lang} source` : `${i.ext.toUpperCase() || 'Text'} file`;
    case 'text': return 'Plain text';
    case 'json': return i.format === 'jsonl' || i.format === 'ndjson' ? 'JSON Lines' : 'JSON';
    case 'notebook': return 'Jupyter notebook';
    case 'csv': return `${F} table`;
    case 'spreadsheet': return `${F} spreadsheet`;
    case 'document': return ({ docx: 'Word document', odt: 'OpenDocument text', rtf: 'Rich Text document' } as Record<string, string>)[i.format] ?? 'Document';
    case 'epub': return 'EPUB book';
    case 'presentation': return i.format === 'odp' ? 'OpenDocument presentation' : 'PowerPoint presentation';
    case 'archive': return `${F} archive`;
    case 'font': return `${F} font`;
    case 'model': return `${F} 3D model`;
    case 'html': return 'HTML document';
    default: return i.ext ? `${i.ext.toUpperCase()} file` : 'Binary file';
  }
}
