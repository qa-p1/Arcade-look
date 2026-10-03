import { getCurrentWindow } from '@tauri-apps/api/window';
import { h } from '../lib/dom';
import * as fmt from '../lib/format';
import { mediaUrl } from '../lib/urls';
import { autoplay, mediaControls, mediaKeys, waitForMetadata } from './media';
import type { Mounted, ViewCtx } from './types';

export async function mount(host: HTMLElement, ctx: ViewCtx): Promise<Mounted> {
  const video = h('video.video-el', { preload: 'metadata', playsInline: true }) as HTMLVideoElement;
  video.src = mediaUrl(ctx.info.path);
  try {
    await waitForMetadata(video, ctx.signal);
  } catch (e) {
    video.removeAttribute('src');
    video.load();
    throw e;
  }

  const fullscreen = async () => {
    const w = getCurrentWindow();
    await w.setFullscreen(!(await w.isFullscreen()));
  };
  const controls = mediaControls(video, { fullscreen: () => void fullscreen() });
  const wrap = h('div.video-wrap', video, h('div.video-controls', controls.el));
  host.append(wrap);

  video.addEventListener('click', () => controls.togglePlay());
  video.addEventListener('dblclick', () => void fullscreen());

  // Hide controls and cursor while playing and idle.
  let idleTimer = 0;
  const wake = () => {
    wrap.classList.remove('idle');
    clearTimeout(idleTimer);
    idleTimer = window.setTimeout(() => !video.paused && wrap.classList.add('idle'), 2400);
  };
  wrap.addEventListener('pointermove', wake);
  video.addEventListener('pause', wake);
  video.addEventListener('play', wake);

  const w = video.videoWidth;
  const hgt = video.videoHeight;
  const dur = isFinite(video.duration) ? video.duration : null;
  ctx.setStatus([w && hgt ? `${w} × ${hgt}` : '', dur ? fmt.duration(dur) : ''].filter(Boolean).join('  ·  '));
  ctx.setDetails([
    ['Resolution', w && hgt ? `${w} × ${hgt}` : 'Audio only'],
    ['Duration', dur ? fmt.duration(dur) : ''],
  ]);

  void autoplay(video, ctx.boot.config.autoplay);
  wake();

  return {
    keydown: (e) => mediaKeys(controls, e),
    dispose() {
      clearTimeout(idleTimer);
      controls.destroy();
      video.pause();
      video.removeAttribute('src');
      video.load();
    },
  };
}
