# Plugins

Plugins add previews for formats Arcade Look doesn't handle, or replace a built-in viewer. Each plugin is a folder inside the plugins directory:

| OS | Plugins directory |
|---|---|
| Linux | `~/.config/arcade-look/plugins/` |
| macOS | `~/Library/Application Support/arcade-look/plugins/` |
| Windows | `%APPDATA%\arcade-look\plugins\` |

```
plugins/
└── my-plugin/
    ├── plugin.json
    └── index.js        (script plugins only)
```

Plugins are loaded at startup. Restart Arcade Look from its tray menu after adding or editing one, or run `arcade-look --quit` and then launch it again. Run with `ALOOK_DEBUG=1` to see manifest errors and plugin failures.

## Manifest (`plugin.json`)

| Field | Type | Default | Meaning |
|---|---|---|---|
| `name` | string | *(required)* | Shown in the info panel |
| `type` | `"command"` \| `"script"` | `"command"` | See below |
| `extensions` | string[] | `[]` | Lower-case extensions without the dot (`"doc"`, `"tar.gz"`) |
| `kinds` | string[] | `[]` | Built-in kinds to match: `image`, `video`, `audio`, `pdf`, `markdown`, `code`, `text`, `json`, `csv`, `spreadsheet`, `document`, `presentation`, `archive`, `font`, `model`, `html`, `folder`, `binary`, … |
| `mode` | `"override"` \| `"fallback"` | `"override"` | `override` runs *before* the built-in viewer; `fallback` runs only if the built-in viewer fails (e.g. an unsupported video codec) |
| `command` | string[] | | Command plugins: program and arguments (no shell) |
| `output` | `"html"` \| `"text"` \| `"markdown"` \| `"image"` \| `"file"` | `"text"` | Command plugins: how to show the result |
| `language` | string | | For `text` output: highlight.js language (`"sql"`, `"python"`…) |
| `timeoutMs` | number | `15000` | Command plugins: kill the command after this long |
| `entry` | string | | Script plugins: module file, relative to the plugin folder |

## Command plugins

The command runs with the plugin folder as its working directory, no stdin, and these placeholders substituted in every argument:

| Placeholder | Value |
|---|---|
| `{path}` | Absolute path of the file being previewed |
| `{outdir}` | An empty, private output folder for this file |
| `{name}` / `{stem}` / `{ext}` | `report.final.docx` / `report.final` / `docx` |
| `{plugin}` | The plugin's own folder |

Output types:

- **`html`**: stdout is shown in a sandboxed frame, where scripts never run.
- **`text`**: stdout is shown in the code viewer (optionally highlighted via `language`).
- **`markdown`**: stdout is rendered as GitHub-flavored Markdown.
- **`image`**: stdout is image bytes (PNG/JPEG/…).
- **`file`**: the command writes a file into `{outdir}`; the newest file there is previewed with the matching built-in viewer. This is the most powerful mode: convert anything into PDF, MP4, PNG, HTML…

Results are cached per file version (path + size + modification time), so flipping back to a file is instant. A non-zero exit code or a timeout shows the error, with stderr, on the info card.

### Examples

Legacy Office documents through LibreOffice (rendered by the built-in PDF viewer):

```json
{
  "name": "Legacy Office via LibreOffice",
  "type": "command",
  "extensions": ["doc", "ppt", "pps", "vsd", "pub"],
  "command": ["soffice", "--headless", "--convert-to", "pdf", "--outdir", "{outdir}", "{path}"],
  "output": "file",
  "timeoutMs": 60000
}
```

Transcode any video the system can't play, but only when the built-in player fails:

```json
{
  "name": "Transcode unsupported video",
  "type": "command",
  "kinds": ["video"],
  "mode": "fallback",
  "command": ["ffmpeg", "-v", "error", "-t", "300", "-i", "{path}", "-c:v", "libx264", "-preset", "veryfast", "-c:a", "aac", "{outdir}/{stem}.mp4"],
  "output": "file",
  "timeoutMs": 120000
}
```

On Windows, use the full path to the executable if it isn't on `PATH`, e.g. `"C:\\Program Files\\LibreOffice\\program\\soffice.exe"`.

## Script plugins

A script plugin is an ES module that exports `render(host, ctx)`:

```js
// plugins/hex-colors/index.js
export async function render(host, ctx) {
  const text = await ctx.readText(ctx.file.path);
  const colors = text.match(/#[0-9a-fA-F]{6}\b/g) ?? [];
  for (const c of colors) {
    const swatch = document.createElement('div');
    swatch.style.cssText = `width:96px;height:96px;border-radius:12px;background:${c}`;
    host.append(swatch);
  }
  ctx.setStatus(`${colors.length} colors`);
  return { dispose() { /* stop timers, free resources */ } };
}
```

`ctx` contains:

| Member | Description |
|---|---|
| `file` | File info: `path`, `name`, `ext`, `size`, `modified`, `kind`, `mime`, … |
| `fileUrl(path)` | URL for `<img>`, `<video>`, `fetch()`: streams any local file (Range supported) |
| `readText(path, maxBytes?)` | Decoded text (UTF-8/16 and legacy encodings detected) |
| `readBytes(path, offset, length)` | `ArrayBuffer`, up to 1 MiB per call |
| `setStatus(text)` | Short text in the title bar |
| `setDetails([[label, value], …])` | Rows for the info panel |
| `signal` | `AbortSignal`, aborted when the user moves to another file |

`render` may return `{ dispose() }`, which is called when the preview closes. If `render` throws, Arcade Look falls back to the next viewer.

Script plugins run inside the app with the same privileges as the built-in viewers, so only install plugins you trust.

More examples are in [`examples/plugins`](../examples/plugins).
