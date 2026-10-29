# Arcade Look: status

## 2026-10-29: 0.4.0

The overhaul retains every built-in viewer and fallback, plugin support, history,
info panel, file-manager integration, idle release and connected actions. Settings
is a separate native window (760×560, minimum 720×520), created on demand and
destroyed on close. Identifiers and folders move to `dev.arcade.Look` and
`arcade/look/` with copy, verify, then remove; saved shortcuts are kept. The
design kit is Arcade Link `v0.4.4` (readable small text, 4.5:1 in both themes).

Checked on Linux in Link's isolated D-Bus/Xvfb runner (`scripts/check`):

- formatting, TypeScript, build, docs, version, vendored hashes, the design
  inventory, syntax contrast (4.5:1) and file-type badge contrast (3:1), 12 Python tests, Clippy and 77 Rust unit tests;
- the Settings UI matrix: 28 states at minimum size in light and dark, at 100 %
  and 125 % text;
- the nine real-peer Link consumer tests and the real Shelf test;
- `scripts/check-native.py` on the real app: Settings opens from the CLI and
  closes back to a lean idle service; two open/close cycles leave the same
  idle memory (64.3 MB both times, below the 113 MB first start). Live themes,
  and every viewer type with real sample files at 720×520 and 1920×1080 in
  both themes, with no overflow.

Fixed while checking: Settings never appeared (it waited for an animation
frame, which WebKit doesn't run in a hidden window); the preview could close
itself on the focus-out that mapping a window delivers (focus loss is now
ignored for 400 ms after showing); error messages read "Couldn't …"; each
Settings page has its own description.

Not checkable here: a real Hyprland desktop (global shortcut, file-manager
Space key, focus-loss timing, tray clicks), Windows/macOS login items, the
macOS permission re-grant after the bundle id change, and Tools updating a
real 0.3.5 install.

## Release status (2026-10-18)

- Latest release: [v0.3.5](https://github.com/qa-p1/Arcade-look/releases/tag/v0.3.5), published 2026-10-19 (a long day of previews no longer grows Look; see CHANGELOG.md). The CI and e2e notes below were recorded for the earlier v0.2.0.
- Previously released: [v0.3.2](https://github.com/qa-p1/Arcade-look/releases/tag/v0.3.2), published 2026-10-19 (released every inherited descriptor at start, which also cut off helpers from the app's own image).
- Previously released: [v0.3.1](https://github.com/qa-p1/Arcade-look/releases/tag/v0.3.1), published 2026-10-18 (fixes for what verifying the release before it found).
- Previously released: [v0.3.0](https://github.com/qa-p1/Arcade-look/releases/tag/v0.3.0), published 2026-10-18 (fixes from the October audit and stress test).
- Previously released: [v0.2.1](https://github.com/qa-p1/Arcade-look/releases/tag/v0.2.1), published 2026-10-16.
- Previously released: [v0.2.0](https://github.com/qa-p1/Arcade-look/releases/tag/v0.2.0) with a Linux self-installing AppImage, a per-user Windows installer, a macOS dmg, `arcade-release.json` and `SHA256SUMS.txt`.
- Merged to `main` from PR #5 (1640e28) after CI passed on Linux, Windows and macOS: [run 38600222192](https://github.com/qa-p1/Arcade-look/actions/runs/38600222192). That covers the build, tests, clippy or analyzers, installer smoke tests on each OS, and the Linux AppImage install, repair and uninstall check in a temporary profile.
- Ecosystem e2e (Arcade Link `tools/e2e.py`, real CI builds of all eight apps): install, tray hosting (hosted by Tools, own icon back within 1 s after Tools is killed, no flash on restart) and shortcuts pass. Tools 0.2.0 adopts, updates, rolls back, uninstalls and backs up Look.
- Not yet verified: a real Windows or macOS desktop (CI runners only), and the owner's real-desktop checks. Builds are unsigned, so SmartScreen and Gatekeeper ask for approval.

Current source version: 0.3.5, Arcade Link commit `3bbbc5d`. The verification below records 0.1.2
with Arcade Link `v0.2.0`: the `verify-link.py` checks, the consumer tests,
Look's own ecosystem checks and the benchmark were rerun on 2026-10-10; the
cross-app flagship row is from 0.1.0 (2026-10-08, Link `v0.1.0`). This page
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
| [PLAN](dev/history/PLAN.md) | Design rationale, budgets, decisions made during the build |
| [plugins](dev/plugins.md) | Command and script plugin reference |
| [link](dev/link.md) | Exposed actions, connected actions, isolated UI tests, CI dependency |
