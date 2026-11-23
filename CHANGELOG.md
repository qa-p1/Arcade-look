# Changelog

All notable changes to Arcade Look. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- The Actions menu (A) is Arcade Link's shared answer for this file instead of a list built into Look: the same actions, order and names as Arcade's entries in your file manager's menu, from every installed app (best matches first, at most four per app). Actions the catalog renames or that you hid elsewhere follow along. Clipboard's 16 MB limit still shows as a reason, and the current PDF page still goes to Lens.
- Arcade Box 1.0 lists each saved pipeline as its own action, so Look no longer asks Box for its pipelines.
- Files of the kinds Arcade Link 1.1 names exactly reach the apps made for them: SVG drawings, CSV, JSON, HTML, Markdown, e-books and subtitles.
- Quick Look appears in Arcade's file manager menus and the browser extension (links and downloads), ranked after the tools made for a file type; "Inspect file" is only for other apps and the selection preview is a palette command.
- Arcade Link 1.1.1 (from 1.0.13); url 2.5.9 (RUSTSEC-2026-0592) and cookie 0.18.3 (RUSTSEC-2026-0594); quick-xml 0.42 and lzma-rust2 0.25; GitHub Actions checkout 7, setup-node 7, upload-artifact 7 and download-artifact 8.

## [1.0.0] - 2026-11-16

### Added

- The AppImage carries the `dev.arcade.Look` desktop entry and icons (app icon and symbolic tray icon).
- Settings → About has "Third-party notices", which opens the licence notices that ship with Look. Look's package now carries `THIRD_PARTY_NOTICES.md` and `LICENSE`.
- Preview failures are written to the support log, without file paths (LK-8).

### Changed

- "Remove my data" when uninstalling leaves nothing of Look's behind, and a plain uninstall keeps only Look's settings and data; CI checks both on the AppImage.
- A settings, window-state or window-position file that Look can't read is kept as `<name>.unreadable-<time>` next to it and Look starts with defaults, instead of the file being replaced by the next save. A file made by a newer Arcade Look is read and never overwritten by an older one, and settings you didn't set from a newer version are kept when you change a setting.
- Accessibility: links in rendered documents are underlined, the image viewer's "1:1" button and the shortcut recorder are announced by screen readers with the text they show, the PDF page area can be focused and scrolled with the keyboard, and Look's dialogs keep keyboard focus inside them and give it back when they close.
- Escape steps back one level: the actions menu, then Info, then fullscreen, then the preview (RD-LOOK-1, LK-10). The keyboard shortcuts dialog scrolls and lists every key once (LK-2). Error messages go away on their own and never carry over to the next file (LK-1). Actions (A) with no connected apps says so.
- Unreadable files say "Permission denied." and broken links name their target (LK-3).
- `arcade-look --unknown-option` prints usage and exits 2; a path that doesn't exist prints "not found" and exits 1 (LK-7).
- Linux: the AppImage starts faster and no helper process stays running. Look unpacks itself once per version (about 440 MB under `~/.local/share/arcade/runtime/`, removed by `--uninstall`) and runs from there.
- Less memory in the background: once no window is left, WebKit's network process ends and Look's idle memory is returned (or moved to swap when memory is short), and a preview that stays open trims itself. A preview or Settings opens as fast as before.

### Fixed

- Linux: Look started from another Arcade app, or restarted, runs from its own unpacked copy too, asks the first-run install question when it should, and its memory trimmer loads in every case.
- Linux: Restart, and any start from the installed AppImage, could hang without opening Look when the desktop did not report its colour scheme. The unpacked copy inherited settings that pointed into the AppImage's mount, which closes as the copy starts (Arcade Link 1.0.13).
- Valid AVIF images preview on Linux. An image format this system can't decode (such as HEIC on Linux) says so instead of showing a hex dump (LK-11).
- Settings shows its section names, and About points to Arcade Tools for updates (LK-9).
- "1 row", not "1 rows"; an empty file says so; zip dates match tar's (LK-4, LK-5).

## [0.4.3] - 2026-11-06

### Added

- Add keyboard regressions for Settings and Copy path activation, preview shortcuts and conflict → Choose another → save a new chord, without manually restoring focus in the test.

### Changed

- Sanitize the restart successor's environment as well as its descriptors, so a fresh AppImage launcher cannot load incompatible libraries from its predecessor. Gate the actual Linux package on Settings/preview release, stable resident identity and explicit Restart/Quit.
- Let Enter and Space activate the focused preview toolbar button, link or control instead of triggering the preview's open/close shortcuts. Keep those shortcuts available when focus is on the preview itself.
- Focus the shortcut recorder when choosing another shortcut after a conflict, so the next key chord is captured immediately.

## [0.4.2] - 2026-11-03

### Changed

- Keep previews open when the pointer leaves or another window takes focus. Close only on an explicit close/toggle action; verify focus loss, Escape, Space and native window close in the real Linux UI.
- Deliver Linux connected-action notifications from a blocking worker so synchronous D-Bus delivery cannot panic inside the async runtime. Keep notification opt-in, visible-window and cancelled-action suppression.
- Keep the same background process when closing Settings, hiding previews or reclaiming idle WebViews. Remove the automatic memory-cleanup restarts, including the five-minute timer and every-tenth-preview replacement. Explicit Restart and installation/update handoffs remain available.
- Verify native process identity across Settings close/reopen and twelve preview release cycles, while checking that the WebViews are destroyed and explicit Restart still works.
- Release retired Linux WebKit renderers by disconnecting the closing view's IPC signal cycle. Keep other windows and their shared context available; no process killing, cache-policy changes or background restarts.

## [0.4.1] - 2026-11-03

### Changed

- Start normally after an update leaves both the old and new launcher or start-at-login entries. Preserve conflicting entries and record the optional migration warning in the app log instead of exiting before Settings or previews can open.
- Verify native windows with a legacy-entry conflict, and check the actual AppImage with both launcher and login-entry upgrade conflicts before publishing.

## [0.4.0] - 2026-10-29

### Changed

- Adopt the Arcade kit’s Inter, blue accent, controls, viewer chrome, welcome, actions menu and Connected apps page. Keep all viewers, plugins, navigation, metadata and file-manager integrations.
- Open Settings in a separate native window with live themes, shortcut controls, previews, integration repair, plugins, diagnostics, notifications and About. Destroy it on close and reuse the guarded idle restart to reclaim WebKit memory.
- Migrate to `dev.arcade.Look` and Link’s `arcade/look/` folders with copy/verify/remove and conflict preservation. Keep saved shortcuts; standardize new-install defaults and the CLI/tray/desktop entry.
- Pin development toolchains, format the frontend with Prettier, use Cargo.toml as the version source, check vendored hashes across line endings, and run the same scripts/check in CI.
- Small text (metadata, hints, badges) meets 4.5:1 contrast in both themes (Arcade Link design kit `v0.4.4`), and file-type badge icons meet 3:1 on their tint.
- Settings opens reliably from the tray and `--settings`, and a preview no longer closes on the brief focus change while its window appears.
- Restart from the tray no longer waits five seconds, or fails, when the process that started Look hasn't yet reaped the old one (Linux).
- Releases start from `tag.yml` and `release.yml`, like the other Arcade apps.

## [0.3.5] - 2026-10-19

### Changed

- A long day of previews no longer makes Look grow until it fails. Each preview
  window Look releases leaves some of WebKit's shared memory behind, and the
  restart that clears it waited for five quiet minutes, which a steady user
  never gives it: in a soak test the resident Look gained about two open files
  for every three previews, on its way to the 1024 limit. Look now restarts
  itself after every tenth release, as soon as the preview is hidden.

## [0.3.4] - 2026-10-19

### Changed

- Previews no longer crash Arcade Look when it runs from its AppImage (0.3.1 to
  0.3.3). Since 0.3.1 Look moved to the folder you started it from, so
  `arcade-look notes.md` found the file, and the web view behind every preview
  then could not find its helper programs, which are looked up relative to the
  AppImage. Look now reads the files you name against your folder and then goes
  back into the AppImage's folder.
- The AppImage check in CI now previews a file named relative to the user's
  folder from the installed AppImage and fails if Look quits.

## [0.3.3] - 2026-10-19

### Changed

- A helper an app starts from its own image (such as a settings window) keeps
  working after the app quits: the previous release stopped passing the AppImage
  runtime's keep-alive pipe to every program, its own helpers included, so
  quitting could unmount the image under them. Now only programs from another
  image (the installed copy, other Arcade apps, a restart through the AppImage
  file) drop it, so installing from Downloads still leaves nothing behind.
- Restarting from the tray, and starting the service from `arcade-look <file>`,
  no longer keep the previous image mounted.

## [0.3.2] - 2026-10-19

### Changed

- Installing or updating from your Downloads folder no longer leaves the
  downloaded AppImage mounted and its runtime running. The previous release
  stopped passing on the handle on the AppImage's mount, but the runtime also
  waits on a pipe the app inherits, and the installed copy still inherited that;
  now no inherited handle is passed on.

## [0.3.1] - 2026-10-18

### Changed

- Updating or installing from your Downloads folder, the lean restart after five
  quiet minutes and starting other Arcade apps no longer leave the old AppImage
  mounted and its runtime (about 20 MB) running. A program the app started
  inherited an open handle on its AppImage mount and kept it busy.
- `arcade-look report.json` with a relative path opens the file instead of "This
  file no longer exists".
- Archive entries and plugin output are extracted into your own cache folder
  (`~/.cache/arcade-look` on Linux) instead of a shared `/tmp/arcade-look`, so
  other users on the same computer can preview archives too.

## [0.3.0] - 2026-10-18

### Changed

- Lower idle memory on Linux: WebKit's processes are shed after five quiet
  minutes (about 53 MiB instead of 159 MiB).
- Faster second launch: a new `arcade-look <file>` hands its arguments to the
  running Look before starting up (warm invoke about 55 ms instead of 75 ms).
- Previews are faithful and bounded: spreadsheets and text tables are read
  correctly and only as far as shown; DOCX lists, DOCX/ODT image placement, RTF
  characters and code pages render correctly; JSON keeps exact numbers and
  "Expand all" works; nbformat 3 notebooks open; plugins and archives are bounded
  in time; the preview cache tells files apart by device, inode and ctime.
- Moving between files in a folder is fast, folder sizes stop on time, and a slow
  or stale preview no longer replaces a newer one.
- Removing Look's data also removes its WebView storage; logs are kept when Look
  starts without a terminal.
- Built against Arcade Link commit `2fc7c91`.

## [0.2.1] - 2026-10-16

### Changed

- Release the shared Preview/Settings webview 30 seconds after hiding or closing it, keeping repeated Space presses warm while removing idle webview cost (SPEC §17.5). The owner measured about 60 MiB RSS for `arcade-look --service` before opening a window; previously, closing a window left a 404 MiB WebKitWebProcess and a 36 MiB WebKitNetworkProcess alive. These are pre-fix measurements; post-release desktop memory verification remains to be done.
- Keep Link, the tray and global shortcuts available with no webview, including non-interactive `look.inspect`. `idleMinutes` now controls only optional process exit, independently of webview release.
- Queue preview requests until the page has installed its listeners, including cold reopens after release; cancel stale release timers as soon as another preview is requested.

## [0.2.0] - 2026-10-14

### Added

- Add Arcade Link v0.3.0 hub methods, tray hosting and portable settings.
- Add receipt-owned installation, repair and removal, with Linux first-run setup and macOS Applications support.
- Add user guides, generated shortcut tables and CI checks with isolated profiles on Rust 1.88.

### Changed

- Use the shared per-user Windows installer integration and release tools from Link v0.3.0.

[Unreleased]: https://github.com/qa-p1/Arcade-look/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/qa-p1/Arcade-look/compare/v0.4.3...v1.0.0
[0.4.3]: https://github.com/qa-p1/Arcade-look/compare/v0.4.2...v0.4.3
[0.4.2]: https://github.com/qa-p1/Arcade-look/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/qa-p1/Arcade-look/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/qa-p1/Arcade-look/compare/v0.3.5...v0.4.0
[0.3.5]: https://github.com/qa-p1/Arcade-look/compare/v0.3.4...v0.3.5
[0.3.4]: https://github.com/qa-p1/Arcade-look/compare/v0.3.3...v0.3.4
[0.3.3]: https://github.com/qa-p1/Arcade-look/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/qa-p1/Arcade-look/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/qa-p1/Arcade-look/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/qa-p1/Arcade-look/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/qa-p1/Arcade-look/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/qa-p1/Arcade-look/compare/v0.1.2...v0.2.0
