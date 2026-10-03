import type { Bootstrap, FileInfo } from '../lib/backend';

export interface ToolItem {
  icon?: string;
  label?: string;
  title: string;
  onClick?: (btn: HTMLButtonElement) => void;
  active?: boolean;
  /** A non-button element (slider, page counter…). */
  el?: HTMLElement;
  separator?: boolean;
}

export interface ViewCtx {
  info: FileInfo;
  boot: Bootstrap;
  signal: AbortSignal;
  /** True when rendered inside another viewer (an archive entry). */
  nested: boolean;
  /** Error from a previous viewer in the fallback chain, if any. */
  previousError: string | null;
  /** Short status for the title bar ("1920 × 1080", "12 pages"). */
  setStatus(text: string): void;
  /** Extra rows for the info panel. */
  setDetails(rows: [string, string][]): void;
  /** Floating toolbar at the bottom of the viewer. */
  toolbar(items: ToolItem[]): HTMLElement;
  /** Preview another file (drill into folders, follow local links). */
  open(path: string): void;
  /** Show a transient message. */
  toast(text: string): void;
  /** Mount the full viewer chain for another file inside `host` (archive entries). */
  mountNested(host: HTMLElement, info: FileInfo): Promise<Mounted>;
}

export interface Mounted {
  dispose?(): void;
  /** Return true if the key was handled. */
  keydown?(e: KeyboardEvent): boolean;
}

export interface ViewerModule {
  mount(host: HTMLElement, ctx: ViewCtx): Promise<Mounted> | Mounted;
}
