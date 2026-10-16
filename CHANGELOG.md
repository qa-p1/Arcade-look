# Changelog

## 0.2.1 — 2026-10-16

- Release the shared Preview/Settings webview 30 seconds after hiding or closing it, keeping repeated Space presses warm while removing idle webview cost (SPEC §17.5). The owner measured about 60 MiB RSS for `arcade-look --service` before opening a window; previously, closing a window left a 404 MiB WebKitWebProcess and a 36 MiB WebKitNetworkProcess alive. These are pre-fix measurements; post-release desktop memory verification remains to be done.
- Keep Link, the tray and global shortcuts available with no webview, including non-interactive `look.inspect`. `idleMinutes` now controls only optional process exit, independently of webview release.
- Queue preview requests until the page has installed its listeners, including cold reopens after release; cancel stale release timers as soon as another preview is requested.

## 0.2.0 — 2026-10-14

- Add Arcade Link v0.3.0 hub methods, tray hosting and portable settings.
- Add receipt-owned installation, repair and removal, with Linux first-run setup and macOS Applications support.
- Use the shared per-user Windows installer integration and release tools from Link v0.3.0.
- Add user guides, generated shortcut tables and CI checks with isolated profiles on Rust 1.88.
