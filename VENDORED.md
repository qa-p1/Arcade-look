# Vendored Arcade Link assets

Shared installer, release, shortcut and protocol assets are copied unchanged
from [Arcade Link v0.3.0](https://github.com/qa-p1/Arcade-link/tree/v0.3.0),
commit `33650a87b6999fbd553ac678cc7bde7a308cb852`. License: MIT OR Apache-2.0.
Run `python3 scripts/check-vendored.py` to verify the SHA-256 inventory.
Refresh the source pin and hashes together.

| App path | Link source |
| --- | --- |
| `assets/strings/install.json` | `assets/strings/install.json` |
| `scripts/arcade-release.py` | `tools/arcade-release.py` |
| `scripts/test_arcade_release.py` | `tools/test_arcade_release.py` |
| `scripts/test_validate_shortcuts.py` | `tools/test_validate_shortcuts.py` |
| `scripts/validate_shortcuts.py` | `tools/validate_shortcuts.py` |
| `spec/arcade-release.schema.json` | `spec/arcade-release.schema.json` |
| `spec/receipt.schema.json` | `spec/receipt.schema.json` |
| `spec/shortcuts.schema.json` | `spec/shortcuts.schema.json` |
| `spec/vectors/accelerators.json` | `spec/vectors/accelerators.json` |
| `spec/vectors/methods.json` | `spec/vectors/methods.json` |
| `spec/vectors/receipts.json` | `spec/vectors/receipts.json` |
| `spec/vectors/shortcuts.json` | `spec/vectors/shortcuts.json` |
| `spec/vectors/trayhost.json` | `spec/vectors/trayhost.json` |
| `src-tauri/windows/README.md` | `packaging/windows/README.md` |
| `src-tauri/windows/arcade-integration.ps1` | `packaging/windows/arcade-integration.ps1` |
| `src-tauri/windows/arcade.iss` | `packaging/windows/arcade.iss` |
| `src-tauri/windows/arcade.nsh` | `packaging/windows/arcade.nsh` |

```text
3b5d2981ab87fdf07437daa294dce825d59b63580430e9ffa2b58e87d5d72cf0  tools/validate_shortcuts.py
d1f3ac5e6a854a5a46c03eb53600be4c25f698e90cdd818f2aaf3b600bb89398  spec/shortcut-sheet.schema.json
2943f9f8459a0c2299bf7650a4beeb812b88d5097624483fdc252dc9d4342829  assets/strings/install.json
8f360855419930730c486d0978acaecf7a8179cc063465c9f8324878dcf6c2fd  scripts/arcade-release.py
58d78ac6a7b5d129bac4509e59f50fedb9db8c1fd2247431726bc8768776439a  scripts/test_arcade_release.py
8e616c4585084906a4976b207b4cb657c26ab05388e78fc3cdd79df942689dae  scripts/test_validate_shortcuts.py
3b5d2981ab87fdf07437daa294dce825d59b63580430e9ffa2b58e87d5d72cf0  scripts/validate_shortcuts.py
b3f4fffd8929a6a9449947b303b9e07171de8a5820d576f2d47b002944a00624  spec/arcade-release.schema.json
bf510b32071cd2883cee1e258afbd3bcf51fd6eb92f07e8a58cc117686546e07  spec/receipt.schema.json
68e8ed4c5adae97895b215813f5a528b53f2b720bb428112e94ab35489269a1a  spec/shortcuts.schema.json
67d0d5a86e8d9f953c2cf779136c790bdd62d7983fa12d36a65c446e12b246ec  spec/vectors/accelerators.json
a3c5796e067e4e0d71dad453e1be141a873b0999cba6fe9616c866be0906e2e5  spec/vectors/methods.json
aebde82ca97adebe6c4fdd5d0dea338d1134db5ccddf0678ec4cb6d836f94198  spec/vectors/receipts.json
2691d5b60314b7060bac5c219ada25edbfdc30829ea8a22d7496714afe2d4cc5  spec/vectors/shortcuts.json
4e1ce6155a2fbff13cfb5878ed66ee691843ecc36b631693914d5d893b2e8204  spec/vectors/trayhost.json
c0b052a5165ab43390d4cd4f19c05e1a9f9e6ff65e5a64c978087c91331c6c68  src-tauri/windows/README.md
2b2a8b699dc1e6c81c02bb9ad9be1ddf2f8a5447541eb1dfe886056aa5dc4fd6  src-tauri/windows/arcade-integration.ps1
fe152028ff674fdacdef4aed8970bb0a7afeb2dd67d2a3af075d7d95683bf96b  src-tauri/windows/arcade.iss
1f548b420bc08b479d6c65801a208aa67dc80fef851b5e2a49696d307562831e  src-tauri/windows/arcade.nsh
```

The custom NSIS template on the packaging branch retains Tauri’s MIT and Apache-2.0 licenses; its Arcade adaptations are local code.
