// Shared custom controls for <video> and <audio>: consistent on every OS webview.
import './media.css';
import { h } from '../lib/dom';
import * as fmt from '../lib/format';
import { icon } from '../lib/icons';

const SPEEDS = [0.5, 0.75, 1, 1.25, 1.5, 2];

export function savedVolume(): number {
  try {
    const v = parseFloat(localStorage.getItem('alook.volume') ?? '');
    return isFinite(v) ? Math.min(1, Math.max(0, v)) : 1;
  } catch {
    return 1;
  }
}

function saveVolume(v: number) {
  try {
    localStorage.setItem('alook.volume', String(v));
  } catch {
    /* storage unavailable */
  }
}

/** Resolve once metadata is known; reject on a decode/network error. */
export function waitForMetadata(m: HTMLMediaElement, signal: AbortSignal, timeoutMs = 10000): Promise<void> {
  return new Promise((resolve, reject) => {
    if (m.readyState >= 1) return resolve();
    const t = setTimeout(() => finish(resolve), timeoutMs);
    const finish = (f: () => void) => {
      clearTimeout(t);
      m.removeEventListener('loadedmetadata', ok);
      m.removeEventListener('error', bad);
      signal.removeEventListener('abort', abort);
      f();
    };
    const ok = () => finish(resolve);
    const bad = () => finish(() => reject(new Error(mediaError(m))));
    const abort = () => finish(() => reject(new DOMException('aborted', 'AbortError')));
    m.addEventListener('loadedmetadata', ok);
    m.addEventListener('error', bad);
    signal.addEventListener('abort', abort);
  });
}

function mediaError(m: HTMLMediaElement): string {
  const code = m.error?.code;
  if (m.error?.message) console.warn(`media error ${code}: ${m.error.message}`);
  if (code === 4 || code === 3) return "This format or codec isn't supported by your system's web engine.";
  if (code === 2) return 'The file could not be read.';
  return 'The media could not be loaded.';
}

export async function autoplay(m: HTMLMediaElement, enabled: boolean) {
  if (!enabled) return;
  try {
    await m.play();
  } catch {
    // Some engines block unmuted autoplay: retry muted rather than not playing at all.
    if (m instanceof HTMLVideoElement) {
      m.muted = true;
      await m.play().catch(() => {});
    }
  }
}

export interface Controls {
  el: HTMLElement;
  togglePlay(): void;
  seekBy(s: number): void;
  toggleMute(): void;
  speed(delta: number): void;
  destroy(): void;
}

export function mediaControls(m: HTMLMediaElement, opts: { fullscreen?: () => void } = {}): Controls {
  const playBtn = h('button.mc-btn.mc-play', { title: 'Play / pause (K)' });
  const cur = h('span.mc-time', '0:00');
  const total = h('span.mc-time.mc-total', '--:--');
  const buffered = h('div.seek-buffered');
  const played = h('div.seek-played');
  const thumb = h('div.seek-thumb');
  const hoverTip = h('div.seek-tip');
  const seek = h('div.seek', h('div.seek-track', buffered, played), thumb, hoverTip);
  const speedBtn = h('button.mc-btn.mc-speed', { title: 'Playback speed (, / .)' }, '1×');
  const loopBtn = h('button.mc-btn', { title: 'Loop' }, icon('repeat', 16));
  const volBtn = h('button.mc-btn', { title: 'Mute (M)' });
  const vol = h('input.mc-vol', { type: 'range', min: '0', max: '1', step: '0.01', title: 'Volume' }) as HTMLInputElement;
  const fsBtn = opts.fullscreen ? h('button.mc-btn', { title: 'Fullscreen (F)' }, icon('fullscreen', 16)) : null;
  const el = h('div.media-controls', playBtn, cur, seek, total, speedBtn, loopBtn, h('div.mc-volume', volBtn, vol), fsBtn);
  for (const b of el.querySelectorAll('button')) b.tabIndex = -1;

  m.volume = savedVolume();
  vol.value = String(m.volume);

  const setPlayIcon = () => playBtn.replaceChildren(icon(m.paused ? 'play' : 'pause', 18));
  const setVolIcon = () => volBtn.replaceChildren(icon(m.muted || m.volume === 0 ? 'muted' : 'volume', 16));
  setPlayIcon();
  setVolIcon();

  let scrubbing = false;
  const dur = () => (isFinite(m.duration) ? m.duration : 0);
  function paint() {
    const d = dur();
    const p = d ? m.currentTime / d : 0;
    played.style.width = `${p * 100}%`;
    thumb.style.left = `${p * 100}%`;
    cur.textContent = fmt.duration(m.currentTime);
    if (m.buffered.length && d) buffered.style.width = `${(m.buffered.end(m.buffered.length - 1) / d) * 100}%`;
  }
  let raf = 0;
  const loop = () => {
    paint();
    raf = m.paused ? 0 : requestAnimationFrame(loop);
  };
  const onPlay = () => {
    setPlayIcon();
    if (!raf) raf = requestAnimationFrame(loop);
  };
  const onPause = () => {
    setPlayIcon();
    paint();
  };
  const onMeta = () => {
    total.textContent = fmt.duration(dur());
    paint();
  };
  m.addEventListener('play', onPlay);
  m.addEventListener('pause', onPause);
  m.addEventListener('ended', onPause);
  m.addEventListener('loadedmetadata', onMeta);
  m.addEventListener('durationchange', onMeta);
  m.addEventListener('progress', paint);
  m.addEventListener('timeupdate', () => !raf && paint());
  m.addEventListener('volumechange', () => {
    setVolIcon();
    vol.value = String(m.muted ? 0 : m.volume);
  });
  onMeta();

  const togglePlay = () => (m.paused ? void m.play().catch(() => {}) : m.pause());
  playBtn.addEventListener('click', togglePlay);

  const timeAt = (clientX: number) => {
    const r = seek.getBoundingClientRect();
    return Math.min(1, Math.max(0, (clientX - r.left) / r.width)) * dur();
  };
  seek.addEventListener('pointerdown', (e) => {
    scrubbing = true;
    seek.setPointerCapture(e.pointerId);
    m.currentTime = timeAt(e.clientX);
    paint();
  });
  seek.addEventListener('pointermove', (e) => {
    const t = timeAt(e.clientX);
    hoverTip.textContent = fmt.duration(t);
    const r = seek.getBoundingClientRect();
    hoverTip.style.left = `${Math.min(r.width, Math.max(0, e.clientX - r.left))}px`;
    if (scrubbing) {
      m.currentTime = t;
      paint();
    }
  });
  seek.addEventListener('pointerup', () => (scrubbing = false));

  let speedIdx = SPEEDS.indexOf(1);
  const speed = (delta: number) => {
    speedIdx = Math.min(SPEEDS.length - 1, Math.max(0, speedIdx + delta));
    m.playbackRate = SPEEDS[speedIdx];
    speedBtn.textContent = `${SPEEDS[speedIdx]}×`;
  };
  speedBtn.addEventListener('click', () => {
    speedIdx = (speedIdx + 1) % SPEEDS.length;
    speed(0);
  });
  loopBtn.addEventListener('click', () => {
    m.loop = !m.loop;
    loopBtn.classList.toggle('active', m.loop);
  });
  const toggleMute = () => {
    m.muted = !m.muted;
  };
  volBtn.addEventListener('click', toggleMute);
  vol.addEventListener('input', () => {
    m.volume = parseFloat(vol.value);
    m.muted = m.volume === 0;
    saveVolume(m.volume);
  });
  fsBtn?.addEventListener('click', () => opts.fullscreen?.());

  return {
    el,
    togglePlay,
    seekBy: (s) => {
      m.currentTime = Math.min(dur(), Math.max(0, m.currentTime + s));
      paint();
    },
    toggleMute,
    speed,
    destroy() {
      cancelAnimationFrame(raf);
    },
  };
}

/** Keyboard shortcuts shared by both media viewers. */
export function mediaKeys(c: Controls, e: KeyboardEvent): boolean {
  if (e.ctrlKey || e.metaKey || e.altKey) return false;
  switch (e.key) {
    case 'k': case 'K': case 'p': case 'P': c.togglePlay(); return true;
    case 'j': case 'J': c.seekBy(-10); return true;
    case 'l': case 'L': c.seekBy(10); return true;
    case 'ArrowUp': c.seekBy(5); return true;
    case 'ArrowDown': c.seekBy(-5); return true;
    case 'm': case 'M': c.toggleMute(); return true;
    case ',': case '<': c.speed(-1); return true;
    case '.': case '>': c.speed(1); return true;
  }
  return false;
}
