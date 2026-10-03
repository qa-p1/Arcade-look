import { defineConfig } from 'vite';
import { cpSync, existsSync } from 'node:fs';

// pdf.js needs its CMaps, standard fonts and WASM decoders served next to the app.
const pdfAssets = 'public/vendor/pdfjs';
for (const dir of ['cmaps', 'standard_fonts', 'wasm']) {
  const from = `node_modules/pdfjs-dist/${dir}`;
  if (existsSync(from) && !existsSync(`${pdfAssets}/${dir}`)) {
    // Skip the QuickJS engine: it only powers PDF form scripting, which we never enable.
    cpSync(from, `${pdfAssets}/${dir}`, { recursive: true, filter: (src) => !src.includes('quickjs') });
  }
}

export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: {
    // Oldest engines we support: Safari 15 (macOS 12) and WebKitGTK 2.38+.
    target: ['es2021', 'safari15'],
    sourcemap: false,
    chunkSizeWarningLimit: 2500,
    modulePreload: { polyfill: false },
  },
});
