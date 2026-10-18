# Changelog

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
