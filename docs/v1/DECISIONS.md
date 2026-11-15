# Arcade Look 1.0 decisions

Newest last. Each entry says what was decided and why.


## D1 - AVIF decodes in the image backend on Linux only (LK-11)

WebKitGTK builds may lack an AVIF decoder (the audit's did). The `image` crate's `avif-native` decoder (system
`dav1d` >= 1.3, Ubuntu 24.04 ships 1.4) is enabled only for Linux, so the AppImage bundles `libdav1d` and CI installs
`libdav1d-dev`. WebView2 and WKWebView decode AVIF themselves, so Windows and macOS need no new native dependency.
HEIC gets no new decoder: libheif would add an HEVC decoder with patent questions. Where the webview can't show
HEIC (Linux), Look says "This HEIC image can't be decoded on this system." instead of a hex dump; any image with no
decoder does the same.

## D2 - Unknown options are errors (LK-7)

The fleet rule (unknown CLI arguments print usage and exit 2) replaces the old "ignore unknown flags for file managers"
behaviour. The desktop entries pass only `%U`/`%F` and known flags; `--keep-download` (read by the installer) and the
legacy `--install-dialog` are accepted. A path that names nothing exits 1 with `arcade-look: not found: <path>`, without
opening a window; a dangling symlink still opens, so its preview can say where it points.

## D3 - Unreadable files are explained, not hex-dumped (LK-3)

`inspect` adds `problem` ("Permission denied." or "Broken link to <target>."). The viewer chain is then only the info
card, which shows the reason in place of the generic "Couldn't render a rich preview" banner and skips the hex view.

## D4 - Preview failures in the support log, without paths (LK-8)

The frontend's `warn` and `error` lines go to stderr, which the app already captures into its log; one line, at most
500 characters, control characters removed. A failed viewer logs its name, the file's extension and the reason, never
the path (logs must not leak what the user looked at).

## D5 - Settings window is wider than the kit's collapse breakpoint (LK-9)

The kit collapses the sidebar to icons at <= 760 px, and the window opened at exactly 760. It now opens at 880x600 with
a minimum of 800x520, so labels always show; tabs use `aria-label` instead of native `title` tooltips. About says
"Updates are managed by Arcade Tools" with "Open Tools" or, when Tools isn't installed, "Get Arcade Tools".

## D6 - Zip dates (LK-5)

Zip entries use the UTC extended-timestamp field when present, else the DOS time read as local time (it was read as
UTC, which put entries hours off, so they showed as absolute dates next to tar's relative ones). Windows has no local
offset lookup here and keeps the old reading; zips made on Windows usually carry the NTFS or UT field anyway.

## D7 - Text and Markdown previews read 1 MiB by default (LK1)

A 5 MB text or Markdown file peaked near 880 RSS / 654 PSS because up to 4 MiB went into one DOM. The read limit
(`textLimitMb`, never shown in Settings) is now 1 MiB; the "Showing the first ... Open the file to see everything"
banner already explains the cut, so the UX cost is none (AUDIT/memory.md LK1). A saved 4, which was only ever 0.4's
default, is read as 1; any other saved value is kept. Virtualizing the code view was not needed for the budget.

## D8 - Memory: W1, arena cap and W0 from Link 1.0.7; page-out (W3b) not taken

Once no WebKit window is left (the 30 s preview release, or Settings closed), Look calls Link
`runtime::release_webkit()`, which ends WebKit's network process and trims (W1 + W0); the next window starts a new
one, as on first open. `runtime::cap_malloc_arenas()` runs first in `main` (Tools D18: arena count made memory depend
on timing). The 30 s keep-warm (0.15 s re-show) is unchanged. W3b (page out the hidden process) is not applied: the
owner listed paging-out as a UX trade (LOCAL.md item 4) to decide explicitly. U1 (unpacked runtime) comes with the
packaging step.

## D9 - Untangle (step 2): `--remove-data` missed the WebView data after the identifier change

Hidden link: Tauri names the WebView's storage and cache after `tauri.conf.json`'s identifier, which became
`dev.arcade.Look` in 0.4.0 (491d46a), while uninstall kept its own copy, still `app.arcadelook` and still described
as "Tauri's app identifier". So `--uninstall --remove-data` left `~/.cache/dev.arcade.Look`,
`~/.local/share/dev.arcade.Look` and, on macOS, `~/Library/WebKit/dev.arcade.Look`. Uninstall now removes the folders
of both the current identifier (`DESKTOP_ID`) and the old one, and a test checks that `DESKTOP_ID` is
`tauri.conf.json`'s identifier, so the two can't drift again.

## D10 - Untangle (step 2): one place for Look's identity

Look's ids and name lived in the 1,100-line `installer` and were repeated as literals: "Arcade Look" in eight places
(titles, error messages, notifications, the tray tooltip), `dev.arcade.Look` in GTK's program name and the Linux and
macOS entry paths, and `arcade-look` as the log name and the cache folder, which uninstall's data list also names.
`identity.rs` holds them and everything reads them there. Left literal: icon-theme names and the old ids, which
mean something else.

## D11 - Dead code (step 4)

Removed, each in its own commit with the proof in its message:

- `util::file_url` and `util::protocol_base`: with every module in `lib.rs` made private (so `pub` no longer hides
  anything), rustc's `dead_code` lint on the Linux and `x86_64-pc-windows-gnu` builds names them, and a grep of
  `src-tauri` (cfg(windows) and macOS branches included), `scripts/` and `tests/` finds no caller. The frontend has
  its own `protocolBase`. `util::url_segments_from_path` is only called by three unix tests, so it is now
  `#[cfg(all(test, unix))]`.
- The `reload_plugins` command and `api.reloadPlugins`: the command was registered, but the only wrapper is never
  called (no `invoke('reload_plugins')` anywhere, `mock.js` included).
- `rafThrottle` and `nextFrame` in `src/lib/dom.ts`: `grep -w` matches only the definitions.
- CSS `.info-row`, `.info-key` and `.title-navigation .icon-btn:disabled`: no markup, script or Rust string builds
  those classes.

Every other `#[tauri::command]` is both registered and invoked by name from `src/` (checked with
`invoke(<T>)('name'`, and `about_info` through `settings.ts`). No unregistered commands exist. No unused npm
dependency (each is imported in `src/`, `vite.config.ts`, a script or a workflow) and no unused Cargo dependency or
feature (`cargo machete` reports only `tauri-build`, a build dependency used by `build.rs`).

Kept on purpose: `Consumer::invoke` (called only by the ignored integration tests, which are CI code); `Source::Explorer`
and `Source::Finder` and the Windows/macOS-only items the Linux build flags (constructed under cfg); the `#[allow]`s
in `rtf.rs` (removing `unused_assignments` brings back two warnings from `emit_char!`), `util.rs` and
`integration/mod.rs`; no `#[allow(dead_code)]` exists; exports used only inside their own module (types in
`backend.ts`, `chainFor`, `parseExact`, ...) because they are live code with a redundant `export`; `footnote-definition`,
`hljs-*`, `markdown-alert-*`, `jv-*`, `lvl-N` (generated class names); `src/design`, vendored files and `docs/v1/audit`
evidence.

## D12 - Memory (step 5): the resident runs unpacked and gives back what windows leave

`AUDIT/memory.md` items for Look, modelled on Box's D14 and Tools' D10/D18 (`src-tauri/src/memory.rs`). Nothing
changes how Look feels, and no timer runs while Look has no window. It also replaces D8's two deferrals: W3b was held back on a misreading (LOCAL.md item 4
is about paging out an *open* window, OPEN3, which stays off), and U1 was waiting for this step.
- **U1 (RT-7):** a resident start from the AppImage runs from its unpacked copy under `<data>/arcade/runtime/look-*`
  (`runtime::relaunch_unpacked(LOOK, version, false)`), so no FUSE helper stays resident and pages come from the
  real disk; `$APPIMAGE` still names the installed file (the launcher path, autostart and restart are unchanged).
  The call sits after everything that never starts a resident (`--status`, `--arcade-invoke`, `--arcade-manifest`,
  `--restart`, install and uninstall, help, a missing path), after the hand-over to an instance that is already
  running (that one is a short D-Bus call from the mount) and after the install prompt. `--quit` and `--hide` skip
  it, since they never keep a process (Box's rule, `memory::starts_resident`). The launcher always passes `wait =
  false` and exits 0 at once; the copy skips the install prompt the launcher already showed, so Continue does not
  ask twice. Look has no one-shot mode that needs an exit code from the copy.
- **Cleanup that U1 needs:** each version unpacks about 440 MB, outside Look's data folders and the receipt, so
  `--uninstall` removes Look's `look-*` copies (and an interrupted `.look-*`) from Link's runtime folder and from
  the environment's, whether or not `--remove-data` is given; other apps' copies stay (Link's
  `remove_unpacked_except` would remove them all). The resident also removes `look-*` copies other than its own 15 s
  after it starts, unless a process still runs a program from one. Box left both to its step 7 (D19).
- **W1:** unchanged in effect, now in `memory.rs`: once no window is left, Link `runtime::release_webkit()` ends
  WebKit's network process and trims. The delay is 3 s (was 1 s), the audit's minimum: the network process holds
  localStorage, which Look uses for the media volume. The 30 s keep-warm, and so the 0.15 s re-show, stay.
- **W3b:** `runtime::page_out_hidden()` follows the release, as in Box: the idle heap goes to swap only if memory is
  short. It applies only with no window; an open window is never paged out (OPEN3 stays off).
- **W0:** the same release 15 s after the resident starts (a trim if a window is open by then). Look serves no
  Link request that works on its own while hidden (`inspect` reads metadata; previews open a window), so Box's
  "trim after hidden Link work" has no counterpart. Not done after each hide: Look destroys the hidden webview 30 s
  later and W1 trims then, and a trim in between only drops pages the 0.15 s hot re-show reads again.
- **OPEN1:** while a window is open Look trims itself 2 s after a page loads and every 20 s, and WebKit's helper
  processes get the same from `libarcade-trim.so` (vendored from Link v1.0.7 into `packaging/linux`, built by
  `scripts/prepare-trimmer.py`, which `scripts/package` and `build.yml` run before the AppImage, put in `usr/lib` by
  `tauri.conf.json`, preloaded from there by the copy). The launcher does not set it: its mount is gone once the
  copy runs, and a stale `LD_PRELOAD` would reach the helpers.
- **Arena cap:** already in `main` (D8); the copy sets it itself, as Link's relaunch intends.
- **Not taken:** OPEN2 (compositing off), because Look shows video and PDFs that Box's motion probe never covered
  and the owner decides it; OPEN3 (page out the open window); LK1 is D7. The 50 PSS / 70 RSS background budget is
  checked by the fleet gate (step 8), not here.
- **Tests:** `memory::tests` (which arguments start a resident, the `LD_PRELOAD` value, which copies are stale and
  in use), the installer test (uninstall removes only Look's copies, with and without data), and the packaged
  checks, which now follow the resident instead of the launcher: `verify-appimage.py` fails unless Look runs from
  `arcade/runtime/look-*` with `libarcade-trim.so` loaded in WebKit's helpers and the image carries `libarcade-trim.so`;
  `verify-resident.py` takes the resident's PID from its endpoint, checks it is the copy, and that WebKit's network
  process is gone after every close and after the 30 s preview release.

## D13 - Release gates (step 7): Link's app gates on the published rc, and an upgrade gate

`.github/workflows/gates.yml` runs PLAN's exit gates on the published files of an rc through Link's reusable
`app-gates.yml@v1.0.9` (published files and SHA256SUMS, the rc's manifest, conformance, N-1, budgets, the delta
through Tools' updater, the Windows and macOS smoke on the published installers), with Look's Linux checks on the
published AppImage (the package smoke, close/reopen and Restart of the resident, the upgrade, the tray's lifecycle)
and the Tier 2 desktop smoke. "Promote" runs only after a green Gates run on the rc it promotes.
- **Upgrade:** Look had no upgrade test. `scripts/verify-upgrade.py` installs the real previous release from a
  download folder, gives it a user's settings (non-default values, a text limit the user chose), the window state with
  the first-launch setup done and a plugin, runs it, installs the candidate over it, runs that, and checks every
  setting value, the window state and the plugin are kept, there is one launcher and one login item, and an uninstall
  without `--remove-data` keeps the data. Connected apps stay on in the seeded settings: Look finds its resident
  through Link, so `--status` cannot see a resident with it off. CI runs it from 0.4.3 on every Linux build.
- No Tier 1 script yet; the previews' behaviour on a real compositor is the owner's desktop harness run.

## D14 - Link 1.0.11 (step 8 prep)

Look moves from Link 1.0.7 to 1.0.11. What touches Look:
- 1.0.11 gives the unpacked copy's marker (`ARCADE_UNPACKED`) its own folder and drops a marker that does not
  contain the running executable, so a restart's successor or a Look started from another app's unpacked copy
  relaunches into its own copy instead of staying on the FUSE mount. Look read the marker in three places as "set
  means I am the copy": the first-run prompt, the WebKit trimmer preload (D12, OPEN1) and the stale-copy cleanup. An
  inherited marker would have skipped the install prompt, preloaded the trimmer from the launcher's mount (gone once
  the copy runs) and cleaned copies from a process that is not one. All three now ask
  `runtime::is_unpacked_copy()`. 1.0.11 also publishes release files under names GitHub keeps.
- 1.0.10: uninstall removes Look's own copies under `<data>/arcade/runtime/look-*` (D12 relied on Look's own
  cleanup at startup; `--uninstall` now clears them too).
- 1.0.9 is the gates workflow D13 already uses; 1.0.8's changes are in the Qt/QML and Dart kits Look does not use.
The design kit is byte-identical at v1.0.11 (only the recorded tag moves); the WebKit trimmer sources did not
change since v1.0.7, so their record stays. Every Link workflow and checkout is pinned to `v1.0.11`.
