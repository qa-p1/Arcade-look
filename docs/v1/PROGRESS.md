# Arcade Look v1 progress

Detail for `qa-p1/Arcade-catalog` > `dev/v1/PROGRESS.md`. Newest first.


## 2026-11-15 (cloud session) - step 5

Memory (D12), on `local/look-step5`, from `AUDIT/memory.md` and Box's D14: U1 (the resident runs from
`<data>/arcade/runtime/look-*`), W1 (3 s after the last window, WebKit's network process ends), W3b (page-out of the
idle heap, hidden only), W0 (release 15 s after start), OPEN1 (self-trim while a window is open, and
`libarcade-trim.so` for WebKit's helpers, vendored from Link v1.0.7). The arena cap and LK1 were already in (D8, D7).
Also from U1: uninstall removes Look's unpacked copies and the resident removes older versions' copies. Not taken, with
reasons, in D12: trim after each hide, OPEN2, OPEN3. `verify-appimage.py` and `verify-resident.py` now follow the
resident (the launcher exits 0), check it runs from the copy with the trimmer preloaded, and that the network
process is gone after each close.

Tested locally:
- `npm ci`, `npm run format:check`, `npm run typecheck`, `npm run build`: pass.
- `python3 scripts/check-version.py`, `check-docs.py`, `check-vendored.py` (56 files, the two trimmer sources
  included), `check-design.py`: pass. `python3 -m unittest discover -s scripts -p 'test_*.py'`: 12 tests OK.
- `cargo +1.88.0 fmt --all --check`: pass.
- `cargo +1.88.0 clippy --all-targets --locked -- -D warnings`: pass; with `--target x86_64-pc-windows-gnu`: pass.
- `python3 scripts/isolate.py cargo +1.88.0 test --manifest-path src-tauri/Cargo.toml --locked`: 91 passed (88 +
  `memory::tests` x3; the installer test now also covers the unpacked copies).
- `node scripts/ui-checks/check.mjs` and `behaviour.mjs` (PLAYWRIGHT_CHROMIUM set, run directly): PASS.
- `scripts/prepare-trimmer.py` builds `libarcade-trim.so`; preloaded into a copy of `sleep` named `WebKitWebProcess`
  it starts its trim thread.
- `py_compile` of the three scripts changed or added.
- Not run here (needs an AppImage, a bundle build or more disk than this machine has): `verify-appimage.py`,
  `verify-resident.py`, `check-native.py`, the `--ignored` link_consumer and real_shelf tests, the memory budget.
  CI's `build.yml` runs the first two on the packaged AppImage.

## 2026-11-15 (cloud session) - step 4

Dead code (D11), on `local/look-step4`: `util::file_url` and `protocol_base` removed (`url_segments_from_path` is now
test-only), the `reload_plugins` command and `api.reloadPlugins`, `rafThrottle` and `nextFrame`, three unused CSS
rules. No unused npm or Cargo dependencies, files or commands were found beyond that.

Tested locally:
- `npm run format:check`, `npm run typecheck`, `npm run build`: pass.
- `python3 scripts/check-version.py`, `check-docs.py`, `check-vendored.py`, `check-design.py`: pass.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 12 tests OK.
- `cargo +1.88.0 fmt --all --check`: pass.
- `cargo +1.88.0 clippy --all-targets --locked -- -D warnings`: pass.
- `cargo +1.88.0 clippy --all-targets --locked --target x86_64-pc-windows-gnu -- -D warnings`: pass (needed
  `binutils-mingw-w64-x86-64` for `windres`).
- `python3 scripts/isolate.py cargo +1.88.0 test --manifest-path src-tauri/Cargo.toml --locked`: 88 passed.
- `node scripts/ui-checks/check.mjs` and `behaviour.mjs` (PLAYWRIGHT_CHROMIUM set, run directly, not through Link's
  `e2e.py` runner): PASS.
- Not run here: the `--ignored` link_consumer and real_shelf tests, `tauri build --features e2e`, `check-native.py`.

macOS code compiles only in CI.

## 2026-11-13 (step 3: fixes)

On `claude/tender-feynman-3vt7dg`, each fix with its regression test (Rust unit tests, and
`scripts/ui-checks/behaviour.mjs` against the built UI):

| Finding | Commit | Test |
|---|---|---|
| Step 1 lock (Escape ladder, help dialog, error toast, empty Actions) | 9431109 | `behaviour.mjs` |
| RD-LOOK-1, LK-10, LK-2, LK-1, empty Actions | 4fe14c6 | `behaviour.mjs` (markers removed) |
| LK-7 (CLI) | e12dd17 | `cli::tests::unknown_options_are_reported_and_missing_paths_found` |
| LK-3, LK-11, LK-8 | e06a6d6 | `commands::tests::unreadable_and_broken_links_are_named`, `imaging::tests::avif_decodes`, `commands::tests::warnings_and_errors_are_logged_on_one_line`; behaviour: LK-3 (two), LK-11 (HEIC without a decoder, path-free log line) |
| LK-9 | 8c50d67 | behaviour: `LK-9: Settings shows labelled sections and leaves updates to Tools`; check.mjs settings pages at 800x520 |
| LK-4, LK-5 | d90cbf3 | `text::tests::line_counts`, `archive::tests::zip_times_are_utc`; behaviour: `LK-4` |

Left: L1 (move to Link v1.0.7 once tagged, then leave `LEGACY_PENDING`), untangle (step 2), dead code, the memory
items (U1, W1, OPEN1, W3b, LK1) and budgets, CI targets, Round 5 (notices now include dav1d), release.

## 2026-11-14 (Link v1.0.7, L1)

- Look pins Arcade Link `v1.0.7` (40ec027): Cargo, the design kit (`scripts/sync-design.py`; arcade.css and icons.ts
  changed, the tray icons render identically so their PNGs are kept), and the Link runner clones in ci.yml and
  build.yml. Look already lists `arcade-look` and `app.arcadelook` in `legacy_desktop_ids`, so Link 1.0.7's installer
  retires those entries (L1). Removing `arcade.look` from Link's `LEGACY_PENDING` is a Link change, left to Link's
  owner once a Look build with this pin is on main.
- First CI run on the fixes: clippy `unnecessary_cast` on `tm_gmtoff` (fixed), dav1d 0.9 on the 22.04 AppImage runner
  (now built from source and linked statically, D1), and the native Settings size check (placement now uses the same
  880x600 constant as the window builder).
