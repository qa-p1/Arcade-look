# Spec 5: Arcade Look 1.0.0

**Outcome:** Look 1.0.0. It delivers:
- Escape always steps back one level;
- every advertised format previews;
- errors are accurate and transient;
- one desktop identity;
- the memory budget is met.

Starting point: `qa-p1/Arcade-look`, Look 0.4.3. Read `AUDIT/look.md`, `AUDIT/real-desktop.md` §Look and
`AUDIT/FINDINGS.md` L1/L2.

## Findings to fix

| ID | P | What | Where |
|---|---|---|---|
| RD-LOOK-1 | P0 | After the actions menu (A) closes, the menu root still handles Escape and steals focus back, so fullscreen can't be left and Escape is swallowed. | `src/lib/actions-strip.ts:66-70` (handle Escape only while open) |
| LK-10 | P1 | Escape with the Info panel open closes the whole preview. The order must be: help/menu → Info → fullscreen → close. | `src/app.ts:776-790` |
| LK-2 | P1 | The keyboard-shortcuts dialog overflows its card, so the rows and the Close button are unreachable. Add a max height and scrolling. Generate the list from the real key map (it omits M, `,`/`.` and the 3D group; A is listed twice). | `src/lib/welcome.ts:68-150` |
| LK-11 | P1 | A valid AVIF falls back to a hex dump. Enable the AVIF decoder, or bundle the codec WebKit needs; test HEIC the same way. | `src-tauri/Cargo.toml:43` (`image` without `avif`), `src/viewers/image.ts:68-104` |
| L1 | P1 | Duplicate `arcade-look.desktop` and `dev.arcade.Look.desktop` in autostart and applications, logged on every start. Retire the legacy entry once. | integration (Link install code) |
| LK-1 | P1 | "Couldn't…" error toasts never auto-dismiss and carry over to the next file. | `src/app.ts:588-598` |
| LK-3 | P1 | Unreadable or broken-symlink files show a generic render error and a hex dump. Show "Permission denied" or "Broken link to …". | open path |
| LK-7 | P2 | `--bogus` exits 0 silently; a nonexistent path opens "This file no longer exists". Print usage and exit 2; report "not found". | CLI |
| LK-8 | P2 | Preview failures never reach the support log. | `src-tauri/src/commands.rs:323-329`, `src/viewers/registry.ts:134` |
| LK-9 | P2 | Icon-only settings sidebar with mixed tooltip styles; About has "Check for updates" although Tools manages updates. | settings |
| LK-4 / LK-5 | P2 | "1 rows", "0 B · 1 line"; archive dates inconsistent (absolute vs relative). | formatting |
| — | P2 | Actions (A) with no peers does nothing. Show "No connected apps can take this file". | `src/app.ts:733` |

## Memory and size

Look is the largest AppImage (157 MB) and costs about 110 PSS / 194 RSS in the background. Apply the Look items from
`AUDIT/memory.md`:
- release the webview and network process on hide (the 30 s release exists; the network process and pages stay);
- the startup trim;
- running unpacked;
- shared WebKit chunks with Tools and Box.

Items U1, W1, OPEN1, W3b and LK1. LK1: virtualize or truncate large text and markdown, since a 5 MB file peaks at 880/654 (RSS/PSS). Keep the 30 s keep-warm (0.15 s re-show). Budget: background 50 PSS / 70 RSS.
Re-show latency must stay the same, measured in CI.

## CI

Measured: main CI takes 10–75 minutes, and a release rebuilds everything (74 runner-minutes). Build once and promote (`specs/LINK.md` §4).

## Acceptance

All `PLAN.md` exit gates, plus:
- The Escape ladder harness test: menu → Info → fullscreen → close.
- Format matrix test: every fixture in the audit, plus valid AVIF and HEIC.
- One desktop entry after upgrading from 0.4.3.
