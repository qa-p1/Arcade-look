# Arcade Look — Build Plan

> Historical product blueprint. Unmeasured size/performance statements are
> design targets. For implemented behavior and verification, read the
> [README](../README.md) and [current status](STATUS.md).

> A universal, cross-platform Quick Look. Select a file, press **Space**, see it instantly.
> Images, video, audio, PDFs, archives, code, fonts, Markdown, JSON, Office docs, 3D models, notebooks — and anything else through plugins.

This document is the blueprint: what is being built, why each technical choice was made, and how the pieces fit together.

---

## 1. Goals (in priority order)

1. **Never breaks.** Every file gets *some* preview. Every renderer has a fallback chain that ends in a metadata card + hex view. Huge files never freeze the UI: every read is capped, chunked, or time-budgeted.
2. **Instant.** Warm previews in a few milliseconds; the app stays resident (hidden) after the first use and auto-exits when idle, so there's no permanent memory tax.
3. **Light.** Small installer, small idle footprint, no bundled browser engine, no frontend framework runtime. Heavy renderers (PDF, 3D, syntax highlighting) are **lazy-loaded chunks** that cost nothing until you open that file type.
4. **Beautiful and keyboard-first.** A calm, native-feeling UI with light and dark themes, smooth zoom and pan, and every action reachable from the keyboard.
5. **Extensible.** Drop-in plugins: either a **command plugin** (wrap any CLI tool, e.g. pandoc, LibreOffice, ffmpeg) or a **script plugin** (a JS module that renders into the preview pane).

## 2. Stack and why

| Layer | Choice | Why |
|---|---|---|
| Shell | **Tauri 2** (Rust) | Uses the OS webview (WebView2 / WKWebView / WebKitGTK), so there's no bundled Chromium. Installers are a few MB instead of 100+ MB, and idle RAM is a fraction of Electron's. |
| Backend | **Rust** | Fast, memory-safe parsing of untrusted files. Pure-Rust crates wherever possible, so it builds anywhere without system libs. |
| Frontend | **Vanilla TypeScript + Vite** | No framework runtime. A ~1 KB `h()` DOM helper. Each viewer is its own lazy chunk. |
| File transport | Custom `alook://` protocol | Streams files to `<video>`, `<img>`, pdf.js, and three.js with HTTP **Range** support. A 20 GB video previews instantly and never gets loaded into memory. No base64-over-IPC. |

## 3. Supported formats

| Category | Formats | How |
|---|---|---|
| Images (native) | png, jpg/jpeg, gif, webp, bmp, ico, svg, avif, apng, jfif | Webview `<img>`, zoom/pan/rotate, pixel-perfect at 1:1, checkerboard for alpha, EXIF in the info panel |
| Images (decoded) | tiff, tga, dds, hdr, exr, qoi, pnm/pbm/pgm/ppm, farbfeld, **psd/psb** (composite), **camera RAW** (cr2, cr3, nef, arw, dng, orf, rw2, raf, pef, srw…, via the embedded JPEG preview), heic/heif (native on macOS) | Rust decode → PNG through `alook://image`, downscaled to screen size |
| Video | mp4, m4v, mov, webm, mkv, ogv, avi*, … | `<video>` over Range-streamed protocol (*codec support depends on the OS webview, with a graceful fallback card) |
| Audio | mp3, flac, wav, ogg, opus, m4a, aac, aiff, wma*, … | Custom player UI, cover art + tags + duration/bitrate via `lofty` |
| PDF | pdf | pdf.js (legacy build for old WebKitGTK), lazy page rendering, selectable text layer, zoom, page indicator |
| Markdown | md, markdown, mdx, … | `pulldown-cmark` in Rust (GFM tables, task lists, footnotes), raw HTML neutralised, relative images resolved, code blocks highlighted |
| Code / text | ~150 extensions + well-known filenames (Dockerfile, Makefile, …) | highlight.js core + per-language lazy chunks, line numbers, wrap toggle, encoding detection (UTF-8/16 BOM), 2 MB cap with a notice |
| JSON | json, geojson, jsonc*, … | Collapsible tree (lazy expansion) + raw toggle; falls back to code view if invalid |
| Notebooks | ipynb | Markdown + highlighted code + text/image outputs |
| Tables | csv, tsv, psv | Rust `csv` with delimiter sniffing, virtualised table (smooth at 100k+ cells) |
| Spreadsheets | xlsx, xlsm, xlsb, xls, ods | `calamine` (pure Rust), sheet tabs, virtualised grid |
| Documents | docx, odt, rtf, epub | Rust XML → safe HTML (headings, emphasis, lists, tables, links, embedded images) |
| Presentations | pptx, odp | Slide cards with titles, text, and images |
| Archives | zip (+jar, apk, whl, nupkg, …), tar, tar.gz/tgz, tar.bz2, tar.xz, tar.zst, gz, bz2, xz, zst, 7z | Listing as a tree with sizes; **click an entry to preview it** (extracted to temp). Time-budgeted, so huge archives never hang |
| Fonts | ttf, otf, ttc, woff, woff2 | Live sample text (editable), size slider, glyph grid from the cmap, names, designer, version, variable-font axes |
| 3D models | glb, gltf, obj, stl, ply, fbx, dae, 3mf | three.js (lazy), orbit controls, auto-framing, PBR environment, wireframe toggle |
| Web | html, htm, xhtml | Rendered in a **sandboxed** iframe (scripts never run) + source toggle |
| Folders | any directory | Item count, recursive size (time-budgeted), file list; click to drill in |
| Everything else | any | Info card (size, dates, MIME, permissions) + **virtualised hex viewer** that reads on demand (works on 100 GB files) |

## 4. Architecture

```
┌──────────────────────── OS integration ─────────────────────────┐
│ Linux:   implements org.gnome.NautilusPreviewer2 over D-Bus     │
│          → Space in GNOME Files opens Arcade Look (Sushi swap)  │
│          + Open-With entries for Dolphin / Thunar / Nemo / Caja │
│ Windows: low-level Space hook, only when Explorer's item view   │
│          has focus → selected item via COM (IShellWindows)      │
│ macOS:   global shortcut → Finder selection via AppleScript     │
│ All:     `arcade-look <path>` CLI, drag & drop, global shortcut │
└───────────────┬─────────────────────────────────────────────────┘
                │ single-instance: 2nd launch forwards args, exits
┌───────────────▼────────────── Rust core ────────────────────────┐
│ detect.rs   magic bytes + extension → Kind (never fails)        │
│ commands    inspect, read_text, markdown, archive, sheet, doc,  │
│             font, audio, image info, hex chunks, dir, siblings  │
│ protocol    alook://  /f/<path> (Range), /image, /cover, /plugin│
│ plugins.rs  command plugins (timeout, size cap, cache)          │
│ lifecycle   hide-on-close, resident, idle auto-exit             │
└───────────────┬─────────────────────────────────────────────────┘
                │ typed IPC (invoke + binary responses)
┌───────────────▼────────────── Web UI ───────────────────────────┐
│ app.ts      titlebar, info panel, keyboard, navigation, loading │
│ registry    Kind → lazy viewer module, fallback chain           │
│ viewers/*   image, video, audio, pdf, code, markdown, json, …   │
│             every viewer: mount(ctx) → dispose()                │
└─────────────────────────────────────────────────────────────────┘
```

**Viewer contract.** `mount(host, ctx) → { dispose(), keydown?(e), toolbar? }`. Navigation aborts in-flight work via an `AbortSignal` and always calls `dispose()` (pauses media, frees WebGL contexts, destroys pdf documents). A viewer that throws or rejects automatically falls through to the next viewer in its chain, ending at Info + Hex.

**Fast path.** `inspect()` is a single `stat` + an 8 KB header read. The window is pre-created hidden, so a warm preview is just an event plus a paint.

## 5. UX

- Frameless window with a custom title bar (native traffic lights on macOS): file icon, name, kind · size · dimensions, and actions (Open, Reveal, Copy path, Info, Close).
- `Space` / `Esc` close · `←` `→` previous/next file in the folder · `Enter` open with the default app · `I` info panel · `F` fullscreen · `+` `-` `0` zoom · `W` wrap · `R` rotate · `?` shortcut sheet · `Ctrl+Q` quit.
- Follows the system light/dark theme, with no white flash on open (the window background matches the theme).
- Empty state: drop zone, shortcut sheet, and a one-click "Install file manager integration".

## 6. Performance and safety budgets

| Thing | Budget |
|---|---|
| Text read | first 4 MB by default (`textLimitMb`), then a notice + hex view |
| Syntax highlighting | ≤ 512 KB, otherwise plain monospace |
| JSON tree | ≤ 8 MB parsed |
| CSV / sheets | first 10 000 rows × 256 columns |
| Archive listing | 50 000 entries or 2 s |
| Folder size | 200 000 entries or 1.5 s, shown as "≥" |
| Decoded images | downscaled to ≤ 4096 px |
| Embedded doc images | ≤ 8 MB each |
| Plugin commands | 15 s timeout, 64 MB output |

Security: previews of untrusted content never execute it. HTML runs in a `sandbox` iframe with no scripts; Markdown/DOCX/EPUB HTML is generated from an allow-list; `javascript:` links are stripped; and a strict CSP is set.

## 7. Plugins

`<config>/arcade-look/plugins/<name>/plugin.json`

```jsonc
// Command plugin: turn any CLI into a previewer
{ "name": "Legacy Office via LibreOffice", "type": "command",
  "extensions": ["doc", "ppt", "xls"],
  "command": ["soffice", "--headless", "--convert-to", "pdf", "--outdir", "{outdir}", "{path}"],
  "output": "file" }        // "html" | "text" | "markdown" | "image" | "file"
```
`"file"` means "preview whatever the command produced", which re-enters the full pipeline (so a DOC → PDF conversion gets the PDF viewer). Script plugins export `render(host, ctx)` from an ES module. Details are in `docs/PLUGINS.md`.

## 8. Delivery

- `src-tauri/` Rust core, `src/` web UI, `packaging/` desktop integration files, `examples/plugins/`.
- Rust unit tests for every parser, with fixtures generated in tests (zip, tar, docx, xlsx, …).
- An end-to-end smoke run of the real app under Xvfb that screenshots each viewer.
- GitHub Actions: CI (fmt, clippy, tests, typecheck, build) on Linux/macOS/Windows; `build.yml` packages an NSIS setup `.exe`, an AppImage and a universal `.dmg` for every push to `main`, and `release.yml` publishes them (a new `v<version>`, or the rolling `nightly`) with `arcade-release.json` and `SHA256SUMS.txt`.

## 9. Decisions made during the build

- **Linux media streams over loopback HTTP.** WebKitGTK's GStreamer pipeline fails on custom URL schemes (it reads one small range and gives up), so `<video>`/`<audio>` use a tiny token-protected server on `127.0.0.1`. Everything else stays on `alook://`.
- **One WebGL renderer and one pdf.js worker per session.** WebKit doesn't release WebGL contexts promptly, so creating one per preview leaked ~35 MB each time. Reusing them keeps memory flat (verified over 10+ cycles).
- **Ignore auto-repeat on close keys.** The Space that opens a preview may still be held when the window takes focus; its repeats must not close the preview.
- **No `requestAnimationFrame` before showing the window.** WebKit doesn't run it for hidden windows, which would block the first show.

## 10. Honest limits

- Codec support for video/audio is whatever the OS webview supports (e.g. MKV/HEVC varies). Unsupported media falls back to the info card; a command plugin with ffmpeg can transcode.
- Legacy binary Office (`.doc`/`.ppt`) needs a plugin (for example the optional LibreOffice one); Look never requires it. `.xls` works natively.
- Wayland does not allow global shortcuts or reading another app's selection, so on Linux the Space integration works through GNOME Files' previewer protocol; other file managers use "Open With" or custom actions.
- The Windows Explorer hook and macOS Finder integration are built, linted and unit-tested in CI, but were written without access to those OSes and have not been run interactively.

## 11. Arcade Link

Look joins the other Arcade apps through Arcade Link (`v0.1.0`): it exposes `look.preview`, `look.inspect` and, on Windows and macOS, `look.preview_selection`, and shows peer actions in its `A` strip. See [arcade-link.md](arcade-link.md) and the verified state in [STATUS.md](STATUS.md).
