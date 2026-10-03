import { api, type AudioInfo } from '../lib/backend';
import { h } from '../lib/dom';
import * as fmt from '../lib/format';
import { icon } from '../lib/icons';
import { coverUrl, mediaUrl } from '../lib/urls';
import { autoplay, mediaControls, mediaKeys, waitForMetadata } from './media';
import type { Mounted, ViewCtx } from './types';

const WAVEFORM_MAX_BYTES = 80 * 1024 * 1024;

export async function mount(host: HTMLElement, ctx: ViewCtx): Promise<Mounted> {
  const { info, signal } = ctx;
  const audio = new Audio();
  audio.preload = 'metadata';
  audio.src = mediaUrl(info.path);
  const tagsP = api.audioInfo(info.path).catch(() => null);
  try {
    await waitForMetadata(audio, signal);
  } catch (e) {
    audio.removeAttribute('src');
    audio.load();
    throw e;
  }
  const tags: AudioInfo | null = await Promise.race([tagsP, new Promise<null>((r) => setTimeout(() => r(null), 600))]);

  const title = tags?.title || info.name.replace(/\.[^.]+$/, '');
  const sub = [tags?.artist, tags?.album].filter(Boolean).join(' — ');
  const tech = [
    info.format.toUpperCase(),
    tags?.sampleRate ? fmt.hz(tags.sampleRate) : '',
    tags?.bitDepth ? `${tags.bitDepth}-bit` : '',
    tags?.channels ? (tags.channels === 1 ? 'mono' : tags.channels === 2 ? 'stereo' : `${tags.channels} ch`) : '',
    tags?.bitrate ? `${tags.bitrate} kbps` : '',
  ].filter(Boolean).join(' · ');

  const art = h('div.audio-art', icon('audio', 64));
  const backdrop = h('div.audio-backdrop');
  if (tags?.hasCover) {
    const img = new Image();
    img.src = coverUrl(info.path);
    img.onload = () => {
      art.replaceChildren(img);
      art.classList.add('has-cover');
      backdrop.style.backgroundImage = `url("${img.src}")`;
      backdrop.classList.add('on');
    };
  }

  const canvas = h('canvas.waveform') as HTMLCanvasElement;
  const wave = h('div.waveform-wrap', canvas);
  const controls = mediaControls(audio);
  host.append(
    h('div.audio-view',
      backdrop,
      h('div.audio-card',
        art,
        h('div.audio-meta',
          h('div.audio-title.selectable', title),
          sub ? h('div.audio-sub.selectable', sub) : null,
          h('div.audio-tech', tech)),
        wave,
        controls.el)),
  );

  // ------------------------------------------------------------ waveform
  let peaks: Float32Array | null = null;
  const drawWave = () => {
    const dpr = devicePixelRatio || 1;
    const w = wave.clientWidth;
    const hh = wave.clientHeight;
    if (!w || !hh) return;
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(hh * dpr);
    const g = canvas.getContext('2d');
    if (!g) return;
    g.scale(dpr, dpr);
    const css = getComputedStyle(wave);
    const base = css.getPropertyValue('--wave').trim() || '#888';
    const accent = css.getPropertyValue('--accent').trim() || '#7c6cff';
    const bar = 3;
    const gap = 2;
    const n = Math.floor(w / (bar + gap));
    const progress = isFinite(audio.duration) && audio.duration ? audio.currentTime / audio.duration : 0;
    for (let i = 0; i < n; i++) {
      let v = 0.08;
      if (peaks) {
        const from = Math.floor((i / n) * peaks.length);
        const to = Math.max(from + 1, Math.floor(((i + 1) / n) * peaks.length));
        for (let j = from; j < to; j++) v = Math.max(v, peaks[j]);
      }
      const bh = Math.max(2, v * (hh - 4));
      g.fillStyle = i / n < progress ? accent : base;
      const x = i * (bar + gap);
      const y = (hh - bh) / 2;
      g.beginPath();
      if (g.roundRect) g.roundRect(x, y, bar, bh, 1.5);
      else g.rect(x, y, bar, bh);
      g.fill();
    }
  };
  let raf = 0;
  const tick = () => {
    drawWave();
    raf = audio.paused ? 0 : requestAnimationFrame(tick);
  };
  audio.addEventListener('play', () => !raf && (raf = requestAnimationFrame(tick)));
  audio.addEventListener('seeked', drawWave);
  audio.addEventListener('pause', drawWave);
  wave.addEventListener('click', (e) => {
    const r = wave.getBoundingClientRect();
    if (isFinite(audio.duration)) audio.currentTime = ((e.clientX - r.left) / r.width) * audio.duration;
    drawWave();
  });
  const ro = new ResizeObserver(drawWave);
  ro.observe(wave);

  if (info.size <= WAVEFORM_MAX_BYTES) {
    void (async () => {
      try {
        const buf = await (await fetch(mediaUrl(info.path), { signal })).arrayBuffer();
        const Ctor = window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
        const ac = new Ctor();
        const decoded = await ac.decodeAudioData(buf);
        void ac.close();
        if (signal.aborted) return;
        const data = decoded.getChannelData(0);
        const buckets = 1200;
        const step = Math.max(1, Math.floor(data.length / buckets));
        const p = new Float32Array(buckets);
        let max = 0;
        for (let b = 0; b < buckets; b++) {
          let m = 0;
          const end = Math.min(data.length, (b + 1) * step);
          for (let i = b * step; i < end; i += 4) m = Math.max(m, Math.abs(data[i]));
          p[b] = m;
          max = Math.max(max, m);
        }
        if (max > 0) for (let b = 0; b < buckets; b++) p[b] /= max;
        peaks = p;
        wave.classList.add('ready');
        drawWave();
      } catch {
        /* waveform is decorative */
      }
    })();
  }

  ctx.setStatus([tags?.artist ?? '', isFinite(audio.duration) ? fmt.duration(audio.duration) : ''].filter(Boolean).join('  ·  '));
  void tagsP.then((t) => {
    if (!t) return;
    ctx.setDetails([
      ['Title', t.title ?? ''], ['Artist', t.artist ?? ''], ['Album', t.album ?? ''], ['Album artist', t.albumArtist ?? ''],
      ['Year', t.year ?? ''], ['Genre', t.genre ?? ''],
      ['Track', t.track ? `${t.track}${t.trackTotal ? ` of ${t.trackTotal}` : ''}` : ''],
      ['Disc', t.disc ? String(t.disc) : ''], ['Composer', t.composer ?? ''],
      ['Duration', t.durationMs ? fmt.duration(t.durationMs / 1000) : ''],
      ['Sample rate', t.sampleRate ? fmt.hz(t.sampleRate) : ''], ['Bit depth', t.bitDepth ? `${t.bitDepth}-bit` : ''],
      ['Channels', t.channels ? String(t.channels) : ''], ['Bitrate', t.bitrate ? `${t.bitrate} kbps` : ''],
      ['Comment', t.comment ?? ''],
    ]);
  });

  void autoplay(audio, ctx.boot.config.autoplay);

  return {
    keydown: (e) => mediaKeys(controls, e),
    dispose() {
      cancelAnimationFrame(raf);
      ro.disconnect();
      controls.destroy();
      audio.pause();
      audio.removeAttribute('src');
      audio.load();
    },
  };
}
