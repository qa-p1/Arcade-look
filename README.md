# Arcade Look

Arcade Look is a quick file previewer for Windows, macOS and Linux. Preview images, media, documents, code, archives, fonts, 3D models and more without opening a full editor.

## Install

Install or update Look from **Arcade Tools → Apps**, or install it by hand using the [getting-started guide](docs/user/getting-started.md). Tools can share Look's tray controls through the Arcade tray icon; Look keeps its own tray icon when Tools is closed.

## Works with other Arcade apps

Use Look to preview files, then send compatible content to Box, Clipboard, Lens, Wheel or Shelf. See [how the Arcade apps connect](docs/user/with-other-apps.md).

Read the [Arcade Look user guide](docs/user/) for first-run steps, features, shortcuts and help.

## Development

### Build from source

Requirements: [Rust](https://rustup.rs) (stable, 1.88+), Node.js 20+, and on Linux the [Tauri system dependencies](https://tauri.app/start/prerequisites/) (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `librsvg2-dev`, …).

```sh
npm install
npm run tauri dev            # run with hot reload
npm run tauri build          # installers in src-tauri/target/release/bundle/
```

CI (`.github/workflows`) checks every pull request, and packages every push to `main` (`build.yml`: NSIS setup `.exe`, AppImage, universal `.dmg`). When that succeeds, `release.yml` publishes the installers: as release `v<version>` if `version` in `src-tauri/tauri.conf.json` hasn't been released yet, otherwise as the rolling `nightly` pre-release. To ship a new version, bump `version` (also in `src-tauri/Cargo.toml` and `package.json`) and push. A draft release for that version is filled in and published rather than skipped. No secrets are required; if the built-in token can't create a tag (it can't tag an older commit whose workflow files differ from `main`'s), add a `RELEASE_TOKEN` secret with the `workflow` scope and `release.yml` uses it instead.

Checks:

```sh
python3 scripts/verify-link.py
```

Checks use `src-tauri/target/link-tests` and an empty temporary `ARCADE_HOME`.
Use `python3 scripts/verify-link.py --build-e2e` to also build the native test
driver there. Keep the normal debug binary ready to run with its embedded
frontend: `npx tauri build --debug --no-bundle`.

`npm run fixtures` writes a sample of every supported format to `fixtures/` for manual testing. Set `ALOOK_DEBUG=1` to print lifecycle, protocol and UI diagnostics to stderr.

### Configuration reference

Settings live in `config.json` in your config folder: `~/.config/arcade-look/` on Linux, `~/Library/Application Support/arcade-look/` on macOS, and `%APPDATA%\arcade-look\` on Windows. The welcome screen links to it.

```jsonc
{
  "theme": "system",            // "system" | "light" | "dark"
  "idleMinutes": 10,            // release memory after the window has been hidden this long (0 = never)
  "globalShortcut": "Ctrl+Alt+Shift+Space", // Windows default; macOS Ctrl+Alt+Space; Linux ""
  "explorerSpace": true,        // Windows: Space in File Explorer
  "nautilusPreviewer": true,    // Linux: Space in GNOME Files
  "textLimitMb": 4,             // text previews read at most this much
  "highlightLimitKb": 512,      // no syntax colors above this size
  "autoplay": true,             // start video/audio automatically
  "plugins": true,
  "showHidden": false           // include dotfiles when pressing ←/→
}
```

After you close a preview, the window stays warm so the next one is instant. Once hidden for `idleMinutes`, the web view is released. The process then exits unless it is needed in the background for start-at-login, the tray, the Windows Explorer hook or a global shortcut. The final ecosystem benchmark measured about 74.5 MiB for Look at its three-second idle sample; it did not measure the post-timeout state separately.

### Plugins

Command plugins turn a CLI into a previewer. Output can be HTML, text, Markdown, an image, or a file that is opened by the matching built-in viewer. Script plugins are ES modules that render into the preview pane.

```json
{
  "name": "Legacy Office via LibreOffice",
  "type": "command",
  "extensions": ["doc", "ppt"],
  "command": ["soffice", "--headless", "--convert-to", "pdf", "--outdir", "{outdir}", "{path}"],
  "output": "file"
}
```

Ready-made examples are in [`examples/plugins`](examples/plugins) (LibreOffice, Pandoc, ffmpeg transcoding fallback, SQLite schema, color palettes). The full reference is in [docs/PLUGINS.md](docs/PLUGINS.md).

### Architecture

```
file manager ──Space──▶ OS integration ──▶ Rust core ──IPC / alook://──▶ web UI (lazy viewers)
                         Linux: D-Bus     detect · parse · stream          registry → fallback chain
                         Windows: hook    (Range-aware protocol)           image · pdf · code · …
                         macOS: shortcut
```

- **Rust core** (`src-tauri/src`): magic-byte and extension detection, parsers (Office XML, archives, fonts, tags, EXIF, PSD, RAW previews), a streaming `alook://` protocol with HTTP Range, plugins, and window lifecycle.
- **Web UI** (`src/`): vanilla TypeScript with no framework runtime. Each viewer is its own lazily loaded chunk with `mount()`/`dispose()`. A viewer that throws falls through to the next one.
- On Linux, `<video>`/`<audio>` stream from a token-protected loopback HTTP server, because WebKitGTK's media stack cannot stream from custom URL schemes.

The full design rationale is in [docs/PLAN.md](docs/PLAN.md).

### Known limits and platform verification

- Video and audio codecs are whatever the OS web engine supports: WebView2 on Windows, AVFoundation on macOS, GStreamer on Linux (install `gstreamer1.0-libav` for H.264/AAC). Unsupported media falls back to the info card; the ffmpeg example plugin can transcode it.
- Legacy binary `.doc`/`.ppt` need a plugin, such as the optional LibreOffice example; Look does not require or install LibreOffice. (`.xls` works natively.)
- Wayland does not allow global shortcuts or reading another app's selection, so on Linux Space works through GNOME Files' previewer protocol, and other file managers use **Open With** or custom actions.
- The Windows Explorer hook and macOS Finder integration are built, linted and unit-tested in CI on their platforms but have not been run interactively. The Linux path is exercised end to end, including Arcade Link's isolated cross-app runner. See [docs/STATUS.md](docs/STATUS.md).

### License

[MIT](LICENSE)
