# Arcade Link

Arcade Look works with the other Arcade apps (Box, Lens, Wheel, Clipboard)
through [Arcade Link](https://github.com/qa-p1/Arcade-link). Look works
exactly the same when no other Arcade app is installed.

## What Look exposes

| Action | Accepts | Returns | Notes |
|---|---|---|---|
| `look.preview` | `file/*`, `file/*[]`, `folder/reference`, `text/url` (`file://` only) | — | Opens the preview. A batch of files is navigable with ←/→ in the order given. |
| `look.inspect` | `file/*`, `folder/reference` | `structured/file-info` | Headless: Look's own detection (`kind`, `format`, `mime`, Link `type`, `size`, `modified`) plus `width`/`height` for images, `durationMs` for audio and video, `tags` for audio and `family` for fonts, within Look's read budgets. Also served one-shot (`arcade-look --arcade-invoke`). |
| `look.preview_selection` | — | — | Previews what is selected in Windows Explorer or macOS Finder. Not offered on Linux: GNOME Files can't be asked for its selection (it calls Look over the previewer D-Bus interface instead). |

Look's sandbox is unchanged: previews never execute content, whoever asked.
`structured/file-info` follows SPEC §5.4, including its optional metadata fields.

## Server verification

Run `python3 ../Arcade-link/tools/e2e.py --only look` from this repository.
The generated fixtures check resident and one-shot image dimensions and folder
inspection, single and batch previews, encoded `file://` URLs, rejected remote
URLs, and the absence of `look.preview_selection` on Linux. Preview assertions
check the real `app::open` trace; they do not claim WebKitGTK rendered a window.

## Settings

`linkEnabled` ("Connect with other Arcade apps") and `linkDisabledPeers`
(the per-app toggles) in `config.json`. With the switch off, Look's manifest
lists no actions and nothing listens.

## Connected actions

The title bar offers **A** when an enabled peer contributes actions for the
current file. Box contributes up to three featured presets and **More in
Arcade Box…**. Its progress chip includes Cancel, and a returned file opens
in Look. **Send to my devices ↗** includes the file name and path as a payload
preview and is disabled with the standard reason above 16 MiB. **Analyze with
Lens** accepts images and the current PDF page: pdf.js renders a bounded PNG,
Rust writes a private handoff file, and Look removes it after the action.
**Add to Wheel** sends the current `file/<kind>` so Wheel opens its confirmed
file-action editor. No action edits a Wheel deck or bypasses peer safety rules.

Discovery uses directory notifications plus live `app.changed` subscriptions;
opening the menu does no disk access or IPC. Discovery, capability checks,
invocation and handoff writes run on Rust workers. Jobs have a 30-minute limit
and cancellation closes their connection. Stopped peers use one-shot mode for
headless actions and launch on demand for interactive actions.

The strip, its CSS and glyphs are lazy chunks and are absent with no matching
peers. The existing 3D viewer already used A for auto-rotate: it keeps A in
standalone mode; with connected actions, use Shift+A or its toolbar. The ? sheet
lists these keys. Missing peers and disabled connections contribute no entries;
unavailable actions remain disabled with their reason.

Build `cargo test --test link_consumer --no-run` in `src-tauri` before the
isolated `--only look` run. The consumer tests exercise actual mock processes
for discovery in both orders, filtering, size limits, progress, output paths,
cancel, timeout, crashes, stopped-peer lifecycle, Private mode and secret errors.
They also exercise live `app.changed` over a real local socket. Pixel rendering
and PDF page rasterization are build-only until verified in a desktop webview.

## Command line

```sh
arcade-look --arcade-manifest     # Look's manifest (no side effects)
arcade-look --arcade-invoke       # one Link request on stdin (look.inspect), no window
```

## Platforms

| | Linux X11 | Linux Wayland | Windows | macOS |
|---|---|---|---|---|
| `look.preview`, `look.inspect` | tested (the request reaches the previewer; the headless Xvfb test machine can't render WebKitGTK windows) | build only | build only | build only |
| `look.preview_selection` | not offered | not offered | build only | build only |
