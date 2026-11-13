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
