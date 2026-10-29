# Changelog

## 0.4.0 — 2026-10-29

- Adopt the Arcade kit’s Inter, blue accent, controls, viewer chrome, welcome, actions menu and Connected apps page. Keep all viewers, plugins, navigation, metadata and file-manager integrations.
- Open Settings in a separate native window with live themes, shortcut controls, previews, integration repair, plugins, diagnostics, notifications and About. Destroy it on close and reuse the guarded idle restart to reclaim WebKit memory.
- Migrate to `dev.arcade.Look` and Link’s `arcade/look/` folders with copy/verify/remove and conflict preservation. Keep saved shortcuts; standardize new-install defaults and the CLI/tray/desktop entry.
- Pin development toolchains, format the frontend with Prettier, use Cargo.toml as the version source, check vendored hashes across line endings, and run the same scripts/check in CI.
- Small text (metadata, hints, badges) meets 4.5:1 contrast in both themes (Arcade Link design kit `v0.4.4`), and file-type badge icons meet 3:1 on their tint.
- Settings opens reliably from the tray and `--settings`, and a preview no longer closes on the brief focus change while its window appears.
- Releases start from `tag.yml` and `release.yml`, like the other Arcade apps.

## 0.3.5 — 2026-10-19

- A long day of previews no longer makes Look grow until it fails. Each preview
  window Look releases leaves some of WebKit's shared memory behind, and the
  restart that clears it waited for five quiet minutes, which a steady user
  never gives it: in a soak test the resident Look gained about two open files
  for every three previews, on its way to the 1024 limit. Look now restarts
  itself after every tenth release, as soon as the preview is hidden.

## 0.3.4 — 2026-10-19

- Previews no longer crash Arcade Look when it runs from its AppImage (0.3.1 to
  0.3.3). Since 0.3.1 Look moved to the folder you started it from, so
  `arcade-look notes.md` found the file, and the web view behind every preview
  then could not find its helper programs, which are looked up relative to the
  AppImage. Look now reads the files you name against your folder and then goes
  back into the AppImage's folder.
- The AppImage check in CI now previews a file named relative to the user's
  folder from the installed AppImage and fails if Look quits.

## 0.3.3 — 2026-10-19

- A helper an app starts from its own image (such as a settings window) keeps
  working after the app quits: the previous release stopped passing the AppImage
  runtime's keep-alive pipe to every program, its own helpers included, so
  quitting could unmount the image under them. Now only programs from another
  image (the installed copy, other Arcade apps, a restart through the AppImage
  file) drop it, so installing from Downloads still leaves nothing behind.
- Restarting from the tray, and starting the service from `arcade-look <file>`,
  no longer keep the previous image mounted.

## 0.3.2 — 2026-10-19

- Installing or updating from your Downloads folder no longer leaves the
  downloaded AppImage mounted and its runtime running. The previous release
  stopped passing on the handle on the AppImage's mount, but the runtime also
  waits on a pipe the app inherits, and the installed copy still inherited that;
  now no inherited handle is passed on.

## 0.3.1 — 2026-10-18

- Updating or installing from your Downloads folder, the lean restart after five
  quiet minutes and starting other Arcade apps no longer leave the old AppImage
  mounted and its runtime (about 20 MB) running. A program the app started
  inherited an open handle on its AppImage mount and kept it busy.
- `arcade-look report.json` with a relative path opens the file instead of "This
  file no longer exists".
- Archive entries and plugin output are extracted into your own cache folder
  (`~/.cache/arcade-look` on Linux) instead of a shared `/tmp/arcade-look`, so
  other users on the same computer can preview archives too.

## 0.3.0 — 2026-10-18

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

## 0.2.1 — 2026-10-16

- Release the shared Preview/Settings webview 30 seconds after hiding or closing it, keeping repeated Space presses warm while removing idle webview cost (SPEC §17.5). The owner measured about 60 MiB RSS for `arcade-look --service` before opening a window; previously, closing a window left a 404 MiB WebKitWebProcess and a 36 MiB WebKitNetworkProcess alive. These are pre-fix measurements; post-release desktop memory verification remains to be done.
- Keep Link, the tray and global shortcuts available with no webview, including non-interactive `look.inspect`. `idleMinutes` now controls only optional process exit, independently of webview release.
- Queue preview requests until the page has installed its listeners, including cold reopens after release; cancel stale release timers as soon as another preview is requested.

## 0.2.0 — 2026-10-14

- Add Arcade Link v0.3.0 hub methods, tray hosting and portable settings.
- Add receipt-owned installation, repair and removal, with Linux first-run setup and macOS Applications support.
- Use the shared per-user Windows installer integration and release tools from Link v0.3.0.
- Add user guides, generated shortcut tables and CI checks with isolated profiles on Rust 1.88.
