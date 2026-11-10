# Arcade Look 0.4.3 audit

Method: installed AppImage, isolated HOME/XDG/XDG_RUNTIME_DIR + private dbus, Xvfb :92 (no window manager), previews via CLI path, keys via xdotool, screenshots via import. Evidence in AUDIT/look/ (m_a..m_e = file-type contact sheets).

## Coverage
| Feature | Tested | Method / inputs | Result |
|---|---|---|---|
| Text, md, code(py,rs), json tree, bad json, csv, tsv, html, svg, ipynb, stl | yes | CLI path | pass (see LK-4 polish) |
| png/jpg/gif/webp/6000x4000 png | yes | CLI | pass |
| HEIC (400 B stub) | yes | CLI | falls back to hex + error (expected for stub) |
| pdf, mp4, webm, mp3, wav, flac | yes | CLI | render/autoplay OK |
| zip/tar/tgz/7z, docx, ttf, folders, symlink | yes | CLI | pass (LK-5) |
| empty / unreadable (mode 000) / broken symlink | yes | CLI | LK-3 |
| Keys: Left/Right (ends clamp), Esc, ?, i, +/-/0/1/r, Esc closes help only | yes | xdotool | pass except LK-2 |
| Pointer leaves / focus to other window / focus blip | yes | xdotool + GTK second window, no WM | window stays open: PASS |
| Nonexistent path | yes | CLI | LK-1 |
| Settings window (7 sections, Esc closes, destroyed on close, webview released after 30 s) | yes | Ctrl+, / --settings | pass; LK-9 |
| Dark theme (settings.json theme=dark) | yes | md, json, docx, pdf, mp3, ttf, zip | pass (dark_m = dk_m.png) |
| Ctrl+C copy path | yes | gtk clipboard read | pass |
| CLI --show/--hide/--toggle/--status/--settings/--version/--arcade-manifest | yes | CLI | pass; LK-7 |
| 1.3 MB / 81 MB text | yes | CLI | pass (truncated banner correct) |
| Idle: 0.1 CPU tick/s, no growth over 90 s; hidden-app RSS about 230 MB until webview released after 30 s | yes | /proc | pass |
| Startup (--background to ready) | yes | status poll | about 0.25 s, pass |
| Enter/Ctrl+R/F/W/media keys/drag-drop/plugins/Actions strip with peers | no | no WM / no peers in Xvfb | see Unconfirmed |

## Findings
### LK-1 (UX) Error toast never auto-dismisses and survives to unrelated previews
Steps: `arcade-look /nonexistent/path` (error toast "Couldn't finish the action"), Esc, then preview any other file: the same toast is still shown on the new file (and after close/reopen, same webview).
Expected: error toast dismissed on new preview / after timeout. Actual: stays until Close pressed. Evidence: t_m.png, a_m.png. Source: src/app.ts:591-598 (`if (!text.startsWith("Couldn't"))` skips the timer; no clear on open). Method: GUI + code.

### LK-2 (broken) Keyboard-shortcuts dialog overflows its card; rows and Close button unreachable
Steps: preview any file at default 1040x720, press `?`. Card is clipped at "Video · Audio": K and J/L rows spill outside the card on the backdrop, M/Mute, Slower/faster, the whole "3D models" group and the Close button are cut off; wheel scroll does nothing. Esc still closes. Evidence: h2c.png, h3c.png. Source: src/lib/welcome.ts:119-150 (.dialog.help has no max-height/overflow:auto). Also help says Backspace=back while the spec lists Alt+Left/Right (both work).

### LK-3 (UX) Unreadable / broken-link files show misleading generic error
`noperm.txt` (chmod 000): meta says "Plain text · 7 B · 1 rows of 16 bytes", banner "Couldn't render a rich preview. Retry or open in another app", hex view of `-- --`. Expected: "Permission denied". `broken.lnk` (dangling symlink): "Plain text · 8 B", same banner; expected "Broken link to nonexist". Evidence: m_e.png.

### LK-4 (polish) Pluralisation / counts
"1 rows" (t.tsv, HEIC hex "1 rows of 16 bytes"), empty.txt says "0 B · 1 line" (should be empty/0 lines). Evidence: m_a/m_b/m_e.

### LK-5 (polish) Archive dates inconsistent
z.zip row shows absolute "6 Nov", z.tar/tgz/7z show "39 min ago". Evidence: m_d.png.

### LK-7 (polish) Bad CLI input is silent
`arcade-look --bogus` prints nothing, exits 0. `arcade-look /nonexistent/x.txt` exits 0 and opens an error preview window titled "x.txt" ("This file no longer exists" although it never existed). Expected: usage error / non-zero exit. Method: CLI.

### LK-8 (polish) Preview failures are absent from support logs
The earlier session called the logs folder empty. Continuation found an old startup-only log (303 B, pid 119000), so the literal "empty folder" claim is withdrawn. After current missing-file, unsupported AVIF and plugin failures, no corresponding error is added there; current redirected stdout contains only graphics/audio/a11y environment warnings. Expected: actionable render/open errors in the advertised logs. Evidence: `logs-validation.txt` and `continued-app.log`. Method: GUI + filesystem + code. Source: src-tauri/src/commands.rs:323-329 documents frontend log as debug-only; src/viewers/registry.ts:134 logs failed viewers to web console, not a persistent app log.

### LK-9 (UX) Settings deviations from LOOK.md spec
Sidebar is icon-only (tooltips with mixed black/light styles); About has "Check for updates" although spec says "Updates are managed by Arcade Tools"; Shortcuts list in help (`?`) is hard-coded in src/lib/welcome.ts:68-115 and omits M, `,`/`.` and the 3D group visually (LK-2). Evidence: set_m.png (copy at look/).

### LK-10 (UX) Escape closes the preview instead of stepping out of the Info panel
Method: GUI in Xvfb + code. Steps: preview a.txt, press I to open Info, then Escape. Expected under owner rule: close Info first; next Escape closes the preview. Actual: first Escape hides the entire preview. Evidence: info-before-Escape.png, info-after-Escape.png. Source: src/app.ts:779-789 checks fullscreen then closes, ignoring infoOpen. Focus-loss/pointer-leave retest passed in this session (focus-retained.png).

### LK-11 (broken) A valid AVIF falls back to a hex dump
Method: GUI in Xvfb, independent Pillow decode, code reading. Steps: encode existing p.png with Pillow's AVIF encoder as audit.avif; preview the resulting 2822-B valid AVIF. Expected: display image for recognized AVIF format. Actual: generic rich-preview failure + hex dump, metadata calls it AVIF image. Other image formats render. Evidence: continued-audit.avif.png; fixture generation/independent decode recorded in continued validation notes. Suspected source: src/viewers/image.ts:68-104 tries WebKit native decoder then image backend, but src-tauri/Cargo.toml:43 disables image crate defaults and omits AVIF decoder. This build's WebKitGTK also lacks native decode. Source supports fallback failure; packaging/codec choice is the suspected cause.

## Unconfirmed / needs real desktop
- Enter/Ctrl+R (xdg-open / file manager), fullscreen F, window controls: no window manager in Xvfb.
- Actions strip `A`: with no peer apps registered it does nothing (src/app.ts:733 only toggles if strip exists); spec wants an empty state "No connected apps can take this file". Needs a peer to judge.
- Focus-loss/pointer-leave closing: verified on X11 without WM (stays open); Wayland/Hyprland focus semantics need the real desktop.
- 3D `A` key conflict: help lists A both for connected actions and 3D auto-rotate.
- Valid HEIC/AVIF, large PDFs, RTF, office xlsx/pptx not exercised (no fixtures).
## Look 0.4.3

### RD-LOOK-1 — broken — Escape becomes trapped after closing the connected-actions menu
Steps: Space-preview a text file from Nautilus → A → Escape (menu closes, focus returns to its A button) → F → Escape. Expected: Escape exits fullscreen, then another Escape closes the preview. Actual: the fullscreen 2560×1440 window stays fullscreen; more Escape presses are swallowed by the now-closed menu’s button. Click the preview body first and Escape works again. Evidence: `real/look/actions.png`, `fullscreen-before-escape.json`, `fullscreen-after-escape.json` (both fullscreen=2). Reproduced twice; F still toggles. Source: `Rust/arcade-look/src/lib/actions-strip.ts:66-70`: the root handles Escape even when `closed`, stops propagation and refocuses the button. `src/app.ts:784-787` has the correct fullscreen-back behaviour but never receives it.

Verified working:
- Nautilus selects the audit text file → Space → floating 1040×720 preview at (785,360), proper filename title and readable text. No tiling. `real/look/space-preview.png` (crop was offset 25 px; actual geometry from clients).
- Pointer moves out via virtual-pointer motion and focus follows into the harmless file manager; Look remains mapped (`real/look/after-pointer-leave.json`).
- A shows live actions from Box, Clipboard, Shelf and Wheel, with app glyphs and number-key hints. First Escape closes only this menu.
- F enters/exits compositor fullscreen. With focus in the preview body, Escape exits fullscreen one level before closing.

## Round 4 (continuation) — method notes

## Look

- **L1 — Duplicate login and launcher entries.** Both `arcade-look.desktop` and `dev.arcade.Look.desktop` exist in
  `~/.config/autostart` and `~/.local/share/applications`. Look logs on every start: "old integration entries were kept:
  dev.arcade.Look.desktop already exists; both entries were kept". One identity, one entry; the legacy one gets retired.
- **L2 — AppImage size.** 157 MB (WebKitGTK bundled). Size reduction is a polish candidate.

