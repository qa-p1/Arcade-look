#!/usr/bin/env bash
# CI only: mount a DMG and verify its real silent install with an isolated home.
set -euo pipefail
image=$1
name=$2
executable=$3
id=$4
[[ "${GITHUB_ACTIONS:-}" == true ]]
image=$(cd "$(dirname "$image")" && pwd)/$(basename "$image")
root=$(mktemp -d)
mount="$root/mount"
mounted=0
cleanup() {
  if [[ "$mounted" == 1 ]]; then hdiutil detach "$mount" >/dev/null; fi
  rm -rf "$root"
}
trap cleanup EXIT
mkdir -p "$mount" "$root/home" "$root/config" "$root/data" "$root/cache" "$root/run"
chmod 700 "$root/run"
# All environment changes are confined to this process and its children.
export HOME="$root/home" ARCADE_HOME="$root/arcade" XDG_CONFIG_HOME="$root/config" XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_RUNTIME_DIR="$root/run" ARCADE_NO_INSTALL_PROMPT=1
export PATH="$ARCADE_HOME/.local/bin:$PATH"
hdiutil attach "$image" -readonly -nobrowse -mountpoint "$mount" >/dev/null
mounted=1
source="$mount/$name.app"
[[ -L "$mount/Applications" && "$(readlink "$mount/Applications")" == /Applications ]]
[[ -x "$source/Contents/MacOS/$executable" ]]
plutil -lint "$source/Contents/Info.plist"
"$source/Contents/MacOS/$executable" --install --silent --no-launch
installed="$ARCADE_HOME/Applications/$name.app"
[[ -x "$installed/Contents/MacOS/$executable" ]]
cmp "$source/Contents/MacOS/$executable" "$installed/Contents/MacOS/$executable"
python3 - "$ARCADE_HOME/installs/$id.json" "$installed" <<'PY'
import json, sys
from pathlib import Path
r = json.loads(Path(sys.argv[1]).read_text())
assert r['schema'] == 1 and r['method'] == 'macos-bundle'
assert Path(r['path']) == Path(sys.argv[2])
assert Path(r['integration']['cli']) == Path(sys.argv[1]).parents[1] / '.local/bin/arcade-look'
PY
cli="$ARCADE_HOME/.local/bin/arcade-look"
[[ -L "$cli" ]]
[[ "$(readlink "$cli")" == "$installed/Contents/MacOS/$executable" ]]
python3 - "$ARCADE_HOME" <<'PY'
from pathlib import Path
import sys
assert not list((Path(sys.argv[1]) / "Applications").glob(".arcade-*"))
PY
"$installed/Contents/MacOS/$executable" --uninstall --silent
[[ ! -e "$installed" ]]
[[ ! -e "$ARCADE_HOME/installs/$id.json" ]]
# -e alone misses a dangling symlink.
[[ ! -e "$cli" && ! -L "$cli" ]]
[[ ! -e "$HOME/.zprofile" && ! -e "$HOME/.bash_profile" && ! -e "$HOME/.profile" ]]
echo 'macOS CI passed: DMG mount, install, receipt, CLI and uninstall; no interactive desktop run.'
