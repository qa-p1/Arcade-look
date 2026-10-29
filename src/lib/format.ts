// UI values use the shared kit; file-specific permissions and sample rates stay local.
import { formatSize, formatCount, formatDuration, formatRelative } from '../design/format';

export function bytes(n: number | null | undefined, exact = false): string {
  if (n == null || !isFinite(n)) return '—';
  const label = formatSize(n);
  return exact ? `${label} (${formatCount(n)} bytes)` : label;
}
export function count(n: number, singular: string, plural = `${singular}s`): string {
  return `${formatCount(n)} ${n === 1 ? singular : plural}`;
}
export function date(ms: number | null | undefined): string {
  return ms == null || !isFinite(ms) ? '—' : formatRelative(ms);
}
export function shortDate(ms: number | null | undefined): string {
  return ms == null || !isFinite(ms) ? '' : formatRelative(ms);
}
export function duration(seconds: number | null | undefined): string {
  return seconds == null || !isFinite(seconds) ? '--:--' : formatDuration(seconds);
}

export function permissions(mode: number | null): string {
  if (mode === null) return '—';
  const bits = 'rwxrwxrwx';
  let s = '';
  for (let i = 0; i < 9; i++) s += mode & (1 << (8 - i)) ? bits[i] : '-';
  return `${s} (${(mode & 0o777).toString(8)})`;
}

export function hz(n: number | null): string {
  if (!n) return '—';
  return n >= 1000 ? `${(n / 1000).toFixed(n % 1000 ? 1 : 0)} kHz` : `${n} Hz`;
}

export function plural(n: number, word: string) {
  return `${formatCount(n)} ${word}${n === 1 ? '' : 's'}`;
}
