// Typed wrappers around the Rust commands (src-tauri/src/commands.rs).
import { invoke } from '@tauri-apps/api/core';

export type Kind =
  | 'folder' | 'image' | 'image-decode' | 'image-raw' | 'image-psd' | 'svg' | 'video' | 'audio'
  | 'pdf' | 'markdown' | 'code' | 'text' | 'json' | 'notebook' | 'csv' | 'spreadsheet'
  | 'document' | 'presentation' | 'epub' | 'archive' | 'font' | 'model' | 'html' | 'binary';

export interface Plugin {
  id: string;
  name: string;
  type: 'command' | 'script';
  extensions: string[];
  kinds: string[];
  output: 'html' | 'text' | 'markdown' | 'image' | 'file';
  entry?: string;
  language?: string;
  mode: 'override' | 'fallback';
  dir: string;
}

export interface FileInfo {
  path: string;
  name: string;
  dir: string | null;
  ext: string;
  size: number;
  modified: number | null;
  created: number | null;
  accessed: number | null;
  readonly: boolean;
  hidden: boolean;
  symlink: string | null;
  mode: number | null;
  kind: Kind;
  format: string;
  lang: string | null;
  mime: string;
  plugin: Plugin | null;
  fallbackPlugin: Plugin | null;
}

export interface Config {
  theme: 'system' | 'light' | 'dark';
  idleMinutes: number;
  globalShortcut: string;
  explorerSpace: boolean;
  nautilusPreviewer: boolean;
  textLimitMb: number;
  highlightLimitKb: number;
  autoplay: boolean;
  plugins: boolean;
  showHidden: boolean;
  linkEnabled: boolean;
  linkDisabledPeers: string[];
}

export interface Bootstrap {
  /** What to show first: the file in `pending`, the welcome or settings screen, or nothing (background start). */
  screen: 'file' | 'welcome' | 'settings' | null;
  pending: string | null;
  platform: 'linux' | 'windows' | 'macos' | string;
  version: string;
  config: Config;
  configPath: string;
  pluginsDir: string;
  infoPanel: boolean;
  service: boolean;
  debug: boolean;
  /** Linux: loopback HTTP base for <video>/<audio> (WebKitGTK can't stream custom schemes). */
  mediaBase: string | null;
  integration: [string, boolean][];
}

export interface TextData { text: string; encoding: string; truncated: boolean; size: number; lines: number }
export interface ArchiveEntry { path: string; size: number | null; packed: number | null; dir: boolean; modified: number | null; encrypted: boolean }
export interface ArchiveListing { format: string; entries: ArchiveEntry[]; totalSize: number; packedSize: number; truncated: boolean; note: string | null }
export interface TableData { sheets: string[]; sheet: number; rows: string[][]; columns: number; truncatedRows: boolean; truncatedCols: boolean; delimiter: string | null; encoding: string | null }
export interface DocOut { html: string; title: string | null; meta: [string, string][]; truncated: boolean }
export type SlideItem = { type: 'text'; text: string; level: number } | { type: 'image'; src: string } | { type: 'table'; rows: string[][] };
export interface Slide { title: string | null; subtitle: string | null; items: SlideItem[]; notes: string | null }
export interface Deck { slides: Slide[]; aspect: number; title: string | null; meta: [string, string][]; truncated: boolean }
export interface FontInfo {
  family: string | null; subfamily: string | null; fullName: string | null; version: string | null;
  designer: string | null; manufacturer: string | null; copyright: string | null; license: string | null;
  glyphs: number; unitsPerEm: number; faces: number; monospaced: boolean; variable: boolean;
  axes: { tag: string; name: string; min: number; default: number; max: number }[];
  codepoints: number[]; coverage: number; format: string;
}
export interface AudioInfo {
  title: string | null; artist: string | null; album: string | null; albumArtist: string | null;
  genre: string | null; year: string | null; track: number | null; trackTotal: number | null; disc: number | null;
  composer: string | null; comment: string | null; durationMs: number | null; bitrate: number | null;
  sampleRate: number | null; channels: number | null; bitDepth: number | null; hasCover: boolean;
}
export interface ImageInfo { width: number | null; height: number | null; exif: [string, string][] }
export interface DirEntry { name: string; path: string; dir: boolean; size: number | null; modified: number | null; ext: string; kind: string; hidden: boolean }
export interface DirListing { entries: DirEntry[]; total: number; truncated: boolean }
export interface DirSize { bytes: number; files: number; dirs: number; complete: boolean }
export interface PluginOutput { output: Plugin['output']; text: string | null; path: string | null; language: string | null }

export const api = {
  bootstrap: () => invoke<Bootstrap>('bootstrap'),
  inspect: (path: string) => invoke<FileInfo>('inspect', { path }),
  readText: (path: string, maxBytes?: number) => invoke<TextData>('read_text', { path, maxBytes }),
  renderMarkdown: (path: string) => invoke<{ html: string; truncated: boolean }>('render_markdown', { path }),
  markdownBatch: (sources: string[]) => invoke<string[]>('markdown_batch', { sources }),
  readBytes: (path: string, offset: number, length: number) => invoke<ArrayBuffer>('read_bytes', { path, offset, length }),
  listArchive: (path: string, format: string) => invoke<ArchiveListing>('list_archive', { path, format }),
  extractEntry: (path: string, format: string, entry: string) => invoke<string>('extract_entry', { path, format, entry }),
  readTable: (path: string, format: string, sheet?: number) => invoke<TableData>('read_table', { path, format, sheet }),
  readDocument: (path: string, format: string) => invoke<DocOut>('read_document', { path, format }),
  readSlides: (path: string, format: string) => invoke<Deck>('read_slides', { path, format }),
  fontInfo: (path: string, ext: string) => invoke<FontInfo>('font_info', { path, ext }),
  audioInfo: (path: string) => invoke<AudioInfo>('audio_info', { path }),
  imageInfo: (path: string, format: string) => invoke<ImageInfo>('image_info', { path, format }),
  listDir: (path: string) => invoke<DirListing>('list_dir', { path }),
  dirSize: (path: string) => invoke<DirSize>('dir_size', { path }),
  neighbor: (path: string, delta: number) => invoke<string | null>('neighbor', { path, delta }),
  runPlugin: (id: string, path: string) => invoke<PluginOutput>('run_plugin', { id, path }),
  openDefault: (path: string) => invoke<void>('open_default', { path }),
  openUrl: (url: string) => invoke<void>('open_url', { url }),
  reveal: (path: string) => invoke<void>('reveal', { path }),
  log: (level: string, message: string) => invoke<void>('log', { level, message }),
  showWindow: () => invoke<void>('show_window'),
  hideWindow: () => invoke<void>('hide_window'),
  quit: () => invoke<void>('quit'),
  setInfoPanel: (open: boolean) => invoke<void>('set_info_panel', { open }),
  navigateExternal: (delta: number) => invoke<boolean>('navigate_external', { delta }),
  installIntegration: () => invoke<string>('install_integration'),
  getAutostart: () => invoke<boolean>('get_autostart'),
  setAutostart: (enabled: boolean) => invoke<boolean>('set_autostart', { enabled }),
  integrationStatus: () => invoke<[string, boolean][]>('integration_status'),
  setConfig: (patch: Partial<Config>) => invoke<Config>('set_config', { patch }),
  reloadPlugins: () => invoke<number>('reload_plugins'),
};

/** Normalise any thrown value (Tauri rejects with strings) into a message. */
export function errorMessage(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}
