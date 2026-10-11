# Vendored packaging assets

`scripts/VENDORED` is a `sha256sum -c` manifest for every copied asset and
the local Tauri template. Update it only with a reviewed upstream change.

| Local files | Source pin | License |
|---|---|---|
| `src-tauri/windows/arcade.nsh`, `src-tauri/windows/arcade-integration.ps1`, `scripts/arcade-release.py`, `scripts/validate_shortcuts.py`, `spec/shortcuts.schema.json`, `spec/arcade-release.schema.json`, `spec/vectors/accelerators.json` | Arcade Link `arcade/tools-hub` at `e90a860d3337171ffc1a7d2197700a3de63edbd0` | MIT OR Apache-2.0 per Link Cargo metadata; this Link revision contains no license text files |
| `src-tauri/windows/installer.nsi` | Tauri `tauri-cli-v2.12.1`; original template SHA-256 `dabed59013b1d78b879a1a85bc7f2eed2993b33a9a90cdabe5946de3d3950597`; local patched SHA-256 is in `scripts/VENDORED` | MIT OR Apache-2.0; corresponding upstream texts are included as `LICENSE-TAURI-MIT` and `LICENSE-TAURI-APACHE-2.0` |

The Tauri template keeps Tauri's NSIS installer, hooks, application payload,
and upgrade flow. Its changes select `%LOCALAPPDATA%\\Programs\\Arcade Look`,
use Arcade's stable Apps & Features key, include the shared task page, and
guard Tauri's legacy cleanup with the shared receipt ownership result. The
template is configured through `bundle.windows.nsis.template` and must be
reviewed against the pinned upstream source when Tauri is upgraded.

Run `sha256sum -c scripts/VENDORED` from the repository root to verify the
vendored content.
