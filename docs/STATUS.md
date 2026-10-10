# Arcade Look: status

Version 0.1.1, Arcade Link `v0.2.0`. The `verify-link.py` checks and the
consumer tests below were rerun on 2026-10-10 for this version; the ecosystem
and benchmark rows are from 0.1.0 (2026-10-08, Link `v0.1.0`). This page
records what is implemented and how it was checked; the other documents
describe how it works.

## Implemented

- Previews for the formats listed in the [README](../README.md), each with a
  fallback chain ending at an info card and hex view; command and script
  plugins; file-manager integration (GNOME Files previewer protocol, Dolphin
  and Nemo actions, Windows Explorer Space hook, Finder shortcut); idle
  release of the web view; the standard tray menu.
- Arcade Link: `look.preview`, `look.inspect` (also one-shot) and, on Windows
  and macOS, `look.preview_selection`; the `A` actions strip with Box presets
  and pipelines, Send to my devices, Analyze with Lens, Add to Wheel and Add
  to Shelf; the Connected apps section (Box, Lens, Wheel, Clipboard, Shelf,
  Find).

## Verification

| Check | Result |
|---|---|
| `python3 scripts/verify-link.py` (typecheck, Vite build, `cargo fmt`, clippy `-D warnings`, tests) | passes: 43 unit tests, 8 consumer tests run through the isolated runner |
| CI (Linux, Windows, macOS checks and installers) | passing at `90d95f6`; checks passing on [#1](https://github.com/qa-p1/Arcade-look/pull/1) |
| Arcade Link e2e, `look` group and cross-app flows (native Xvfb rendering) | all passing (74/74 ecosystem checks) |
| Benchmark against the 2026-10-05 baseline | startup 77.4 → 74.7 ms, warm invoke 38.2 → 37.7 ms, idle RSS 74 → 75 MiB, idle CPU 0 |

Look runs daily on the owner's Hyprland desktop.

## Limits

- Windows and macOS (Explorer hook, Finder shortcut, selection resolver) are
  built and tested in CI but have not been run interactively.
- Video and audio codecs are those of the OS web engine.
- Legacy `.doc`/`.ppt` need a plugin.
- Wayland: no global shortcut or file-manager selection; Space works through
  GNOME Files, other file managers use Open With.
- Builds are not code-signed.

## Documents

| Document | Contents |
|---|---|
| [README](../README.md) | Formats, install, shortcuts, configuration, plugins, building |
| [PLAN](PLAN.md) | Design rationale, budgets, decisions made during the build |
| [PLUGINS](PLUGINS.md) | Command and script plugin reference |
| [arcade-link](arcade-link.md) | Exposed actions, connected actions, isolated UI tests, CI dependency |
