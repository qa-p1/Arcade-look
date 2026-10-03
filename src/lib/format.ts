// Human-friendly formatting helpers.

export function bytes(n: number | null | undefined, exact = false): string {
  if (n === null || n === undefined || !isFinite(n)) return '—';
  if (n < 1000) return `${n} ${n === 1 ? 'byte' : 'bytes'}`;
  const units = ['KB', 'MB', 'GB', 'TB', 'PB'];
  let v = n;
  let i = -1;
  do {
    v /= 1000;
    i++;
  } while (v >= 1000 && i < units.length - 1);
  const s = `${v >= 100 ? v.toFixed(0) : v.toFixed(1).replace(/\.0$/, '')} ${units[i]}`;
  return exact ? `${s} (${n.toLocaleString()} bytes)` : s;
}

export function count(n: number, singular: string, plural = `${singular}s`): string {
  return `${n.toLocaleString()} ${n === 1 ? singular : plural}`;
}

const dateFmt = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' });
const dayFmt = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' });

export function date(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return '—';
  try {
    return dateFmt.format(new Date(ms));
  } catch {
    return '—';
  }
}

export function shortDate(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return '';
  const d = new Date(ms);
  const now = new Date();
  if (d.toDateString() === now.toDateString()) {
    return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  }
  return dayFmt.format(d);
}

export function duration(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined || !isFinite(seconds)) return '--:--';
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const mm = h ? String(m).padStart(2, '0') : String(m);
  return `${h ? `${h}:` : ''}${mm}:${String(sec).padStart(2, '0')}`;
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
  return `${n.toLocaleString()} ${word}${n === 1 ? '' : 's'}`;
}
