# Checks and release candidates

Fetch packages with `npm ci` and `python3 -m pip install -r requirements-dev.txt`
before formatting or checks. Use Rust 1.88.0 and Node 22; installer builds pin
Rust 1.99.1 because 1.88’s release LTO loses encoding_rs statics. `scripts/dev`,
`scripts/check`, `scripts/package` and `scripts/sync-design` are the entry points.
Cargo.toml is the version source; `scripts/check-version.py --set VERSION`
updates generated lock/shortcut metadata (regenerate the shortcut Markdown).

CI runs `scripts/check` on Linux, Windows and macOS with warnings denied. Linux
also uses the pinned Link tools/e2e.py runner’s temporary HOME, D-Bus and Xvfb.
The preserved nine `src-tauri/tests/link_consumer.rs` tests run explicitly with
`--ignored` inside that runner. `src-tauri/tests/real_shelf.rs` runs explicitly
against SHA-256-pinned Shelf v0.3.3 fetched by scripts/fetch-test-peer.py. Ordinary
unit-test discovery continues to ignore these runtime tests; CI must run both
explicit commands. No failures or unavailable artifacts may be counted as passes.

The real native check builds the embedded frontend with the e2e feature, checks
Settings create/destroy, two process-tree RSS returns within 5 MiB, both live
themes, every viewer with generated real files at minimum and full display size,
and 125% Settings text. `artifacts/ui/native` contains screenshots and memory
measurements. Playwright separately checks the compiled UI with deterministic
IPC fixtures, all Settings sections and error/retry/shortcut interactions.
Those browser fixtures do not verify the native backend.

Installer jobs smoke-test AppImage, DMG and Windows setup in temporary profiles.
The beta package rehearsal checks all three installers, arcade-release.json and
SHA256SUMS.txt and uploads a candidate artifact without publishing. tag.yml and
release.yml remain manual; official releases are outside this task’s authorization.
Do not create tags by hand. A real Hyprland desktop is still needed for overlay
shortcut/placement/focus, tray clicks and update-through-Tools; Windows/macOS
login items and macOS permission re-grant also need their native desktops.
