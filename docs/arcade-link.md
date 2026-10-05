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

## Settings

`linkEnabled` ("Connect with other Arcade apps") and `linkDisabledPeers`
(the per-app toggles) in `config.json`. With the switch off, Look's manifest
lists no actions and nothing listens.

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
