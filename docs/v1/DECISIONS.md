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

## D15 - Tier 1 on headless Hyprland (step 7)

D13 left the previews' behaviour on a real compositor to the owner's desktop run. `tests/desktop/look_desktop.py`
runs on Link's headless Hyprland harness (`desktop-e2e.yml`) in CI on every build and in the gates on the published
rc, from the AppImage as a user starts it (without the harness's `ARCADE_UNPACKED`, so Look hands over to its
unpacked copy): the window's class is `dev.arcade.Look` (ID-1); the preview has keyboard focus when it opens; with
Info open the first Escape closes only Info and the second the preview (RD-LOOK-1, LK-10, so far only checked in
the browser by `behaviour.mjs`); Space closes it; it stays open when the pointer moves into another application's
window (fleet popup rule; `tests/desktop/other_window.py` is that window); a second file reuses the running Look
(same process for both windows); `--quit` stops it. Tier 1 blocks the gates on any failure.

## D16 - Security, licences and notices (Round 5)

Round 5 of the v1 plan: `SECURITY.md`, `cargo deny`, `THIRD_PARTY_NOTICES.md`, Dependabot. Every claim in `SECURITY.md`
was checked against the code, and each row names a test that exists or says it has none.
- **Advisories with a fix were fixed, not accepted:** `calamine` 0.36 → 0.38 (CFB/OLE2 parser, RUSTSEC-2026-0436) and `zip`
  8 → 9 (four `extract` advisories; Look does not call `extract`, but the fix is a version away), `pulldown-cmark`
  0.13.6 (wikilinks), `ruzstd` 0.9.1 (decompression bomb), `tokio` 1.54 (`AsyncFd`), `rustix` 1.1.6, `async-executor`
  1.14.1 (yanked). zip 9 reports an entry name it cannot decode as an error: the archive listing shows the raw bytes.
- **Accepted, each with its reason in `deny.toml`:** twelve advisories, none a known vulnerability Look can reach.
  glib 0.18 `VariantStrIter` (RUSTSEC-2024-0429, unsound): the fix is glib 0.20, Tauri 2's Linux stack is on gtk-rs 0.18,
  and nothing in the tree calls `Variant::array_iter_str`. Unmaintained notices: proc-macro-error (gtk-rs macros), `paste`
  (build-time, through lofty and exr), `unic-*` (urlpattern in tauri-utils), `zvariant`/`zbus_names`/`zvariant_derive`
  (zbus 5, merged into zbus 6; Look's D-Bus code, ksni, notify-rust and the Tauri plugins are on 5), and `ttf-parser`
  (Look's font reader; `forbid(unsafe_code)`, panics become a preview error). Replacing `ttf-parser` with `skrifa` is a
  rewrite of `font.rs` and is not part of this round.
- **Licences:** the allow list is what the graph needs: Box's list minus CDLA-Permissive-2.0, NCSA and OFL-1.1 (a font,
  not a crate), plus `bzip2-1.0.6` (libbz2-rs-sys, the `bzip2` crate's pure-Rust backend). `arcade-link` carries
  `version = "1.0.11"` next to its git tag because `cargo deny` treats a git dependency without a version as a wildcard.
- **CI:** the `deny` job in `ci.yml` (so a release waits for it too) uses `cargo-deny-action` pinned to a commit like the
  other actions, with `manifest-path: src-tauri/Cargo.toml`.
- **Notices:** `THIRD_PARTY_NOTICES.md` is generated by `scripts/notices.py` (`--check` in `scripts/check`, so on all three
  OSes): the shipped crates for every target, the production npm packages, pdf.js's CMaps, fonts and decoders, Inter, the
  files vendored from Link with the tag and commit `VENDORED.md` names, dav1d (built from source and linked statically
  into the Linux build), the AppImage's runtime and hooks, Tauri's NSIS template and the native libraries. The file and
  `LICENSE` are Tauri resources in every package, `verify-appimage.py` checks the image has them, and Settings → About
  opens them.
- **Native libraries:** `packaging/linux/native/` (package, source package and copyright text of everything linuxdeploy
  bundles) is checked by `scripts/native-notices.py --check` in `build.yml`, on the 22.04 runner right after the build.
  The committed list was **not** produced on that runner: it was derived from the published 0.4.3 AppImage (built on 22.04, so
  not this build: it predates the trimmer) and the Ubuntu jammy archive's file index (`Contents`,
  release and updates), with the source packages' copyright files (not the installed binary packages'). The first CI run is expected
  to fail the check once; its log prints the corrected `packages.json` and the regenerated folder is uploaded as the
  `native-notices` artifact to commit. Link's gates run on 24.04 and do not run this check.
- **Tests added for surfaces that had none:** the window's CSP and permissions (`tests/security_config.rs`), the plugin
  asset route (`plugin_routes_refuse_parent_folders`), which links open (`open_url_takes_web_and_mail_links_only`), and
  the sanitiser and the HTML frame in a real browser engine (`scripts/ui-checks/sanitize.mjs`).
- **Known and documented, not changed:** a previewed document's `https` images are fetched (the window's policy allows
  `https:` images), which tells that server the file was opened. Blocking them would break READMEs with badges; it is
  the owner's call and `SECURITY.md` says so.

## D17 - Accessibility pass (Round 5)

Modelled on Box's D21. `scripts/ui-checks/a11y.mjs` (`npm run test:a11y`; in `scripts/check` next to the other browser
checks, so in CI) runs against the built UI with the same IPC fixture (`mock.js`) and checks, in light and dark:
- axe-core's WCAG 2.1 A and AA rules (`wcag2a`, `wcag2aa`, `wcag21a`, `wcag21aa`; axe-core 4.14.0, the version Box
  uses, a dev dependency only) on 30 surfaces: the preview window for each viewer (Markdown, code, JSON, image, table,
  PDF, archive, font, audio, video and the info card, each file served the way the app's protocol serves it) with the
  Info panel closed and open, the shortcuts overlay (`?`) and every Settings section. A viewer that does not mount, or
  falls back to the info card, fails the run, so a surface cannot silently go unchecked;
- every control Tab reaches on those surfaces looks different when focused (outline, ring, border or background, on the
  control or the field around it), compared by blurring and refocusing the same element;
- with reduced motion requested, nothing animates or transitions for longer than 10 ms.
Not covered: 3D models (WebGL), HTML, Office documents, notebooks, slides and plugin output (no fixture yet), text at
125% (`check.mjs` covers Settings overflow at 125%), and whether Tab stays inside the shortcuts overlay (it does not:
Tab leaves the dialog for the toolbar behind it; no WCAG A/AA rule axe runs covers it, so it is left as it is).

On the sources before this change it found 12 problems (4 causes, each in light, dark or both), all fixed. Contrast,
focus rings and motion passed already:
- **A toolbar button with visible text was named from its tooltip only.** The image viewer's "1:1" button was named
  "Actual size (1)" (WCAG 2.5.3, label in name). Toolbar buttons that show text are now named "<text> <tooltip>"
  ("1:1 Actual size (1)").
- **The shortcut recorder had the same mismatch.** It showed the keys but was named "Record global shortcut". Its name
  is now the keys it shows followed by visually hidden "Record global shortcut"; while recording it reads "Press keys…".
- **The PDF page area scrolled but had no keyboard focus of its own** (axe `scrollable-region-focusable`). It is now a focusable region named "PDF pages", so the arrow keys and Page Up/Down scroll it.
- **Links in rendered documents were told apart by colour alone** (axe `link-in-text-block`; the dark accent on dark
  text is under 3:1). They are underlined now (they were underlined only on hover): the one visible change.

Count after each fix, in the order made: 12 before, 8 after the toolbar names, 4 after the PDF region, 2 after the
underlined links, 0 after the recorder's name.

Beyond axe: Tab left the shortcuts overlay for the toolbar behind it, and focus stayed where it was when a dialog opened.
Look's dialogs (the shortcuts overlay and messages) are now modal (`aria-modal`): focus starts on their first control,
Tab and Shift+Tab stay inside, and focus returns to where it was when they close. `behaviour.mjs` "the shortcuts dialog
is modal" failed before the change.

## D18 - Link 1.0.12's workflows

Link 1.0.11's `setup-arcade-link` cached its CLI under `runner.os` only, so a gates job on ubuntu-22.04 could restore a
CLI built on ubuntu-24.04 ("GLIBC_2.39 not found"; Box's rc.2 gates hit it). Every reusable Link workflow and
`link-ref` is pinned to `v1.0.12`, which keys the cache by runner image. The crate, the build's Link checkout and the
vendored files stay at `v1.0.11`: 1.0.12 changes no Rust code or design file.

## D19 - Round 5: data files, schema versions and safe downgrade

From the Round 5 audit (modelled on Box's D19). Link's FS-2: every data file Look writes carries a schema version,
migrations are tested from released versions, and an older Look never loses or corrupts a newer file. Released shape =
0.4.1, 0.4.2 and 0.4.3 (the three write the same files; the fixtures are 0.4.3's, in `src-tauri/tests/data/v0.4.3/`).
- **Inventory** (files Look writes and reads back): `settings.json` (`config::load`/`try_load`/`update`),
  `state.json` (`load_state`/`save_state`), `window-geometry.json` (`window_geometry`), `install-dismissed.json`
  (`installer::dismiss`). Not data files of Look's own: the portable settings export (`portable_settings`, already a
  document with `schema: 1`, `app` and `version`, strict on purpose), the Link receipt and manifest (written through
  arcade-link, its schema), `plugins/<id>/plugin.json` (written by the person, read only), and the caches, temp files
  and the AppImage's unpacked copy (rebuilt on demand).
- **`schema` is written only above 1.** No field means 1, the shape every release wrote. 0.4.x reads these files without
  a `schema` field in mind, and `window-geometry.json` and `install-dismissed.json` have a shape (a map of maps, a bare
  array) where a new top-level key would make 0.4.x discard the whole file. So a v1 file stays byte for byte what 0.4.x
  writes (`state.json` is rewritten with exactly the same bytes when it holds nothing unknown). The number is kept once
  a newer Look wrote it.
- **Unknown fields survive a save:** `settings.json` (top level; it has no nested objects) as before through `update`;
  `state.json` now merges what it doesn't know back in; `window-geometry.json` keeps unknown fields in each rectangle,
  other windows and other monitors. `install-dismissed.json` is a list of records and rewrites its own entries only.
- **A newer schema is read and never written:** one log line, "made by a newer Arcade Look (schema N ...)", then the
  fields this build knows are used. `config::update` returns "settings.json was made by a newer Arcade Look" (Settings
  shows it), `save_state`, geometry saves and the dismissed list do nothing. A newer file this build can't map to its
  structs is left in place and Look runs on defaults, without writing. A portable settings file above schema 1 is
  refused with the same words.
- **A file that can't be parsed** (not JSON, wrong types, not an object) is moved to `<name>.unreadable-<unix seconds>`
  (a counter is added on a clash), logged, and Look starts with defaults. Before, an invalid `settings.json` fell back
  to defaults and the next `update` replaced the file. `load()` writes fresh defaults after moving the file away;
  `update`, `save_state`, geometry and the dismissed list do the same before writing. A file that exists but can't be
  read for another reason (permissions) is not moved.
- `upgrade()` (`textLimitMb` 4 to 1, LK1) is unchanged; it runs in memory on load, so the file stays as it was until
  the next change. Tested from the 0.4.3 fixture, with a chosen 8 kept.
- Tests: 0.4.3 `settings.json`, `state.json` and `window-geometry.json` fixtures load with every value kept; a v1 file
  stays free of `schema`; unknown fields survive; a newer schema is kept and not overwritten (each file); an unreadable
  file is moved aside (each file); missing fields default; `upgrade()`. They use a private home folder, never the real
  one. Five of the settings and state tests failed on the code before this change (newer settings overwritten by
  `update`, newer state overwritten, unknown state fields dropped, two unreadable files not set aside).
- Not done: a migration from a schema above 1 (there is none yet; the first one adds its fixture and test), and
  `settings.json` is still written by `update` without a lock against another Look process.

## D20 - Round 5: clean uninstall, and 1.0.0

- `scripts/verify-appimage.py` now checks a clean uninstall the way Arcade Box's does (Box D19). It takes an inventory
  of the disposable profile before installing. After `--uninstall --silent`, nothing may be new except Look's own kept
  folders (`arcade/look` under the XDG roots), and no unpacked copy (`arcade/runtime/look-*`, U1) may remain. It then
  installs again, runs a preview, and after `--uninstall --silent --remove-data` nothing new may remain at all,
  Look's settings folder included. Not counted: the script's work folder and Link's runtime state (`run`,
  `arcade/run`, `arcade/handoff`, which the script's SIGKILL leaves). WebKit storage outside Arcade's folders is
  printed, not failed.
- The "keeps user data" sentinel was written to `ARCADE_LOOK_HOME`, which Look never reads, so it proved nothing. It now
  sits in Look's real settings folder (`<XDG_CONFIG_HOME>/arcade/look`, `config.rs`).
- Version 1.0.0 (`src-tauri/Cargo.toml`, the lock file and the shortcut sheet) for the first release candidate,
  `v1.0.0-rc.1`. README screenshots stay open, as for Box.

## D21 - Link conformance before rc.2

Arcade Link's static conformance check (`arcade-link conformance .`) was run on `v1.0.0-rc.1`. Everything else in the
gates passed; five checks failed:
- **ID-1, ID-2, ID-3:** no `dev.arcade.Look.desktop`, app icon or symbolic icon in the repository. Added
  `packaging/linux/dev.arcade.Look.desktop` (`X-Arcade-Id=arcade.look`), `dev.arcade.Look.svg` (Link's
  `design/app-icons/look/icon.svg`, unchanged) and `dev.arcade.Look-symbolic.svg` (Link's `design/glyphs/arcade.look.svg`,
  unchanged), and listed them in `bundle.linux.appimage.files` in `src-tauri/tauri.conf.json` at the same `/usr/share/...`
  paths Box uses. The launcher Look writes at install time (`Icon=arcade-look`) is unchanged.
- **REPO-1:** `CHANGELOG.md` now follows Keep a Changelog 1.1.0, as Box's D18 did: `[Unreleased]`, `## [X.Y.Z] - date`,
  the 1.0.0 bullets grouped by what each says, older bullets unchanged and grouped by their first verb, and compare links.
- **REL-1:** every `apt-get`, `npm ci` and `pip install` step in `ci.yml` and `build.yml` has a `timeout-minutes` (10, or 20
  for the apt installs). No command changed.

## D22 - Restart hung in rc.2's gates: Link 1.0.13

rc.2's Gates failed "explicit Restart replaces the process" in `verify-resident.py`; the same check had passed in the
release build on the same commit. A diagnostic branch (`claude/sharp-cori-29w5eg-wip-restart`) ran the check 12 times
on the Gates setup and failed once: the old process had exited, and the successor's unpacked copy was stuck in its
`AppRun` on `gsettings get org.gnome.desktop.interface color-scheme`, which had crashed and sat in its core dump. Link's
`relaunch_unpacked` gave the copy the launcher's environment, where the AppImage's `AppRun` had pointed
`GSETTINGS_SCHEMA_DIR` and `GIO_EXTRA_MODULES` into the mount, and the launcher releases the mount as soon as it has
started the copy. linuxdeploy's GTK hook calls `gsettings` only when the desktop portal does not answer, as on CI.

Fixed in Link 1.0.13 (the copy gets no reference to the mount), which Look moves to before rc.3. `verify-resident.py`
now lists the test's processes when Restart times out.
