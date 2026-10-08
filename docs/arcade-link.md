# Arcade Link

Arcade Look works with the other Arcade apps (Box, Lens, Wheel, Clipboard)
through [Arcade Link](https://github.com/qa-p1/Arcade-link). Look works
exactly the same when no other Arcade app is installed.

## What Look exposes

| Action | Accepts | Returns | Notes |
|---|---|---|---|
| `look.preview` | `file/*`, `file/*[]`, `folder/reference`, `text/url` (`file://` only) | — | Opens the preview. A batch of files is navigable with ←/→ in the order given. |
| `look.inspect` | `file/*`, `folder/reference` | `structured/file-info` | Headless: Look's own detection (`kind`, `format`, `mime`, Link `type`, `size`, `modified`) plus `width`/`height` for images, `durationMs` for audio and video, `tags` for audio and `family` for fonts, within Look's read budgets. Also served one-shot (`arcade-look --arcade-invoke`). |
| `look.preview_selection` | — | `file/*[]` in resolve-only mode | Previews what is selected in Windows Explorer or macOS Finder. Not offered on Linux: GNOME Files can't be asked for its selection (it calls Look over the previewer D-Bus interface instead). |

`look.preview_selection` accepts `options.resolveOnly: true`: it returns all
selected paths as individual typed outputs using Look’s detection, with the
message `N files selected`, and opens no preview. An empty selection returns
`unavailable` with the reason `Nothing is selected in the file manager.`
Windows uses Explorer’s selected Shell items; macOS uses Finder’s selection.
Linux keeps this action hidden: GNOME Files can request a preview but provides
no API for Look to resolve its selection on demand. Wheel hides file-selection
inputs on Linux for this reason. Windows/macOS resolver execution is not run
on this Linux machine.

`app.status.status.mode` records how this instance started (`background` for
`--background`/`--service`, otherwise `foreground`). Opening a preview or
settings later does not change it, so Arcade Tools can preserve the mode.

Look's sandbox is unchanged: previews never execute content, whoever asked.
`structured/file-info` follows SPEC §5.4, including its optional metadata fields.

## Server verification

Build with `python3 scripts/verify-link.py --build-e2e`, then run
`python3 ../Arcade-link/tools/e2e.py --only look` from this repository.
The shared runner uses the separate test binary and consumer-test executables.
The generated fixtures check resident and one-shot inspection, batch/file-URL
previews, startup mode, and Linux’s unavailable selection resolver. Native
webview checks render images and PDFs under isolated Xvfb, exercise Connected
apps and shortcut warnings, and run real Box video/PDF jobs and saved pipelines.
The real Clipboard action is checked before and after video compression.

## Settings

`linkEnabled` ("Connect with other Arcade apps") and `linkDisabledPeers`
(the per-app toggles) in `config.json`. With the switch off, Look's manifest
lists no actions and nothing listens.
Settings includes a lazy **Connected apps** section with the master switch,
four peer rows, Running/Installed/Not installed states and **Use with Arcade
Look** toggles for installed peers. Missing peers get a short description and
Get, without a toggle; Get opens
`tools.install` with `options.app` in an available Arcade Tools manager, or
the canonical GitHub releases page.
Diagnostics shows the registry location, endpoint state and last Link error.
Changes to these connection switches apply without restarting Look.

## Connected actions

The title bar offers **A** when an enabled peer contributes actions for the
current file. Box contributes up to three featured presets and **More in
Arcade Box…**. Its progress chip includes Cancel, and a returned file opens
in Look. **Send to my devices ↗** includes the file name and size as a payload
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
unavailable actions are hidden. Clipboard’s size limit keeps its available
Send action visible but disabled with the standard reason.

All test artifacts live in `src-tauri/target/link-tests`.
The consumer tests exercise actual mock processes
for discovery in both orders, filtering, size limits, progress, output paths,
cancel, timeout, crashes, stopped-peer lifecycle, Private mode and secret errors.
They also exercise live `app.changed` over a real local socket. Saved Box
pipelines are fetched on workers when Box’s manifest changes, then cached.
The strip lists matching input types and skips interactive-first pipelines;
opening it never queries Box. Pipeline invocation passes `options.pipeline`.
Images, video, PDF rendering and page rasterization are verified in the native
Linux X11 webview under isolated Xvfb.

## Command line

```sh
arcade-look --arcade-manifest     # Look's manifest (no side effects)
arcade-look --arcade-invoke       # one Link request on stdin (look.inspect), no window
```

## Platforms

| | Linux X11 | Linux Wayland | Windows | macOS |
|---|---|---|---|---|
| `look.preview`, `look.inspect` | tested with native Xvfb rendering | build only | build only | build only |
| `look.preview_selection` | not offered | not offered | build only | build only |

## Isolated UI tests

`ALOOK_E2E_MAP_EARLY=1` is a test-only switch, honored only inside the
ecosystem runner (`ARCADE_E2E_INNER=1`). It maps the window before the page
loads so WebKitGTK can boot under Xvfb without a window manager. Normal
startup still reveals the preview after painting. Never set this in a login
profile or desktop configuration.

For native-webview assertions, run `python3 scripts/verify-link.py --build-e2e`
from the repository root. It sets `CARGO_TARGET_DIR` to
`src-tauri/target/link-tests`, checks the frontend and Rust with an empty
`ARCADE_HOME`, then invokes `npx tauri build --debug --no-bundle --features e2e`.
Tauri embeds the frontend in the test binary. The normal `target/debug/arcade-look`
may be used by start on login and must always come from
`npx tauri build --debug --no-bundle`, without test features.

The `e2e` feature adds a private control socket only when the isolated runner
supplies `ALOOK_E2E_CONTROL` beneath its temporary root and `ALOOK_DEBUG=1`.
It evaluates test scripts in the actual webview; production builds contain no
control listener. The shared runner selects these test artifacts.
Run `python3 ../Arcade-link/tools/e2e.py --only failure` after the Look group
to check peer crashes, cancellation, busy quit, corrupt manifests and restart.

Check the normal debug binary itself through the second-instance channel:

```sh
python3 ../Arcade-link/tools/e2e.py run -- python3 scripts/verify-rendering.py
python3 ../Arcade-link/tools/e2e.py run -- python3 scripts/verify-rendering.py --peers
```

Both checks open an image, PDF, text file, folder and settings, with screenshots
of the rendered content. They use the normal binary without the native test
socket, and terminate only processes started by their isolated session.

## CI dependency

Check and installer jobs check out Look and a pinned Arcade Link revision as
siblings, matching the relative Cargo dependency. The source defaults to
`qa-p1/Arcade-link` at `5e1b916b62beae60997ffe5fb2e9b31ac70b3f5e`; the
`ARCADE_LINK_REPOSITORY` and `ARCADE_LINK_REF` repository variables override
it. Windows and macOS are compiled and unit-tested in the CI matrix; they
were not run interactively. Before publishing Look, replace the
local path dependency with the tagged Link dependency described in the plan.
