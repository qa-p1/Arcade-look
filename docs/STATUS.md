# Arcade Look: status

Version 0.1.2, Arcade Link `v0.2.0`. The `verify-link.py` checks, the
consumer tests, Look's own ecosystem checks and the benchmark were rerun on
2026-10-10 for this version; the cross-app flagship row is from 0.1.0
(2026-10-08, Link `v0.1.0`). This page records what is implemented and how
it was checked; the other documents describe how it works.

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
| `python3 scripts/verify-link.py` (typecheck, Vite build, `cargo fmt`, clippy `-D warnings`, tests) | passes: 43 unit tests, 9 consumer tests run through the isolated runner (25 repeated runs, no failures) |
| `tests/real_shelf.rs` against the real Arcade Shelf 0.1.0 (`ARCADE_SHELF_BIN`, ignored by default) | passes: Look offers exactly one "Add to Shelf" for a previewed image; Shelf answers "Added 1 item to Quick Shelf" and lists the file by its path; the original stays put |
| CI (Linux, Windows, macOS checks and installers) | checks passing on [#1](https://github.com/qa-p1/Arcade-look/pull/1), [#2](https://github.com/qa-p1/Arcade-look/pull/2) and [#3](https://github.com/qa-p1/Arcade-look/pull/3) |
| Arcade Link e2e, `look` group: Look's own checks (native Xvfb rendering) | all 10 pass, 4 runs in a row, with mock peers (Link [#2](https://github.com/qa-p1/Arcade-Link/pull/2) expects six Connected apps rows and nine consumer tests) |
| Arcade Link e2e, cross-app flagship flow with real Box, Clipboard, Lens and Wheel | last passing on 0.1.0 (74/74 ecosystem checks); not rerun for 0.1.2, which needs all four apps built |
| Benchmark (`benchmarks/bench.py --apps look --runs 5`), 0.1.0 and 0.1.2 alternated on one machine | no difference: startup median 131/132 ms (0.1.0) vs 127/132 ms (0.1.2), warm invoke 68–72 vs 68–77 ms, idle RSS 97 MiB both, idle CPU 0. That machine is slower than the one behind the 2026-10-05 baseline (77.4 ms, 38.2 ms, 74 MiB), so the baseline comparison reports a regression the alternated runs do not show |

Look runs daily on the owner's Hyprland desktop.

## Limits

- Windows and macOS (Explorer hook, Finder shortcut, selection resolver) are
  built and tested in CI but have not been run interactively.
- Video and audio codecs are those of the OS web engine.
- Legacy `.doc`/`.ppt` need a plugin.
- Wayland: no global shortcut or file-manager selection; Space works through
  GNOME Files, other file managers use Open With.
- Builds are not code-signed.
- The e2e check `native_connected_settings_and_shortcut_warning` fails now
  and then (about 2 runs in 15, with 0.1.0 as well as 0.1.2; once with the
  Settings window not found in time). Not yet diagnosed.

## Documents

| Document | Contents |
|---|---|
| [README](../README.md) | Formats, install, shortcuts, configuration, plugins, building |
| [PLAN](PLAN.md) | Design rationale, budgets, decisions made during the build |
| [PLUGINS](PLUGINS.md) | Command and script plugin reference |
| [arcade-link](arcade-link.md) | Exposed actions, connected actions, isolated UI tests, CI dependency |
