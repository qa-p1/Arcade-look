# Arcade Look v1 progress

Detail for `qa-p1/Arcade-catalog` > `dev/v1/PROGRESS.md`. Newest first.


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
