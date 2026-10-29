# Vendored Arcade Link assets

Shared installer, release, shortcut and protocol assets are copied unchanged
from [Arcade Link](https://github.com/qa-p1/Arcade-link/tree/3bbbc5d0d7d383e18c7e7e20552e942c1a971abd) commit
`3bbbc5d0d7d383e18c7e7e20552e942c1a971abd` (v0.3.0 plus unreleased installer fixes). License: MIT OR Apache-2.0.
Run `python3 scripts/check-vendored.py` to verify the SHA-256 inventory.
Refresh the source pin and hashes together.

One file is adapted, not copied unchanged: in `src-tauri/windows/README.md` the two
links to Link's `SPEC.md` and `spec/vectors/` point at Link on GitHub, because the
relative links only resolve inside the Link repository.

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
0c829174d2dd5e15930cd2c4c5cb791ab6f744a3dab13ed557161aabcacf5fce  assets/strings/install.json
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
49eeca7ca6582dae31b9714b3e1c5e910c8040581bf7484b07c91741f3698874  src-tauri/windows/README.md
2b2a8b699dc1e6c81c02bb9ad9be1ddf2f8a5447541eb1dfe886056aa5dc4fd6  src-tauri/windows/arcade-integration.ps1
fe152028ff674fdacdef4aed8970bb0a7afeb2dd67d2a3af075d7d95683bf96b  src-tauri/windows/arcade.iss
1f548b420bc08b479d6c65801a208aa67dc80fef851b5e2a49696d307562831e  src-tauri/windows/arcade.nsh
5e20732b8216c2c814073d01ac2d3fe1a53289658ed397588df3ab63dad3dbc8  src-tauri/windows/installer.nsi
9dd42ea92cff2ede5cd477cbfcce051b2d0115c0ac7f368ee88cb545055dff1d  src-tauri/windows/LICENSE-TAURI-MIT
0d542e0c8804e39aa7f37eb00da5a762149dc682d7829451287e11b938e94594  src-tauri/windows/LICENSE-TAURI-APACHE-2.0
```

## Tauri NSIS template

`src-tauri/windows/installer.nsi` is adapted from Tauri `tauri-cli-v2.12.1`
(original template SHA-256
`dabed59013b1d78b879a1a85bc7f2eed2993b33a9a90cdabe5946de3d3950597`; the local
patched hash is above). License: MIT OR Apache-2.0, with the upstream texts in
`LICENSE-TAURI-MIT` and `LICENSE-TAURI-APACHE-2.0`. It keeps Tauri's installer,
hooks, payload and upgrade flow, and changes the install folder to
`%LOCALAPPDATA%\\Programs\\Arcade Look`, uses Arcade's stable Apps & Features
key, includes the shared task page, and guards Tauri's legacy cleanup with the
shared receipt ownership result. Review it against the pinned upstream source
when Tauri is upgraded.

## Arcade design kit

Copied unchanged from [Arcade Link](https://github.com/qa-p1/Arcade-Link/tree/v0.4.4) tag `v0.4.4` (commit
`1d150983114b36c6adb2dc80f8889cec43079bf8`) by `scripts/sync-design.py`; never edit these files here.
License: MIT OR Apache-2.0; Inter is under the SIL Open Font License 1.1
(`src/design/fonts/OFL.txt`).

```text
38bad47bf726e131e4d1cc9b75a0c83ef52b6f40a9140aaac6b9cf045b978842  src/design/arcade.css
69a7a24cd12ce0981eab0a10145cc1f18afc9829d6351fb8673ded33147c88ab  src/design/icons.ts
9f30255bf79330c12e962502f92059df256c6360f9df844c334f912aff85b9d8  src/design/format.ts
693b77d4f32ee9b8bfc995589b5fad5e99adf2832738661f5402f9978429a8e3  src/design/fonts/InterVariable.woff2
262481e844521b326f5ecd053e59b98c8b2da78c8ee1bdbb6e8174305e54935a  src/design/fonts/OFL.txt
11405845a97dcec4150fb3943d03dbd11eeb086ded31a5cab27aa92b620838c1  src/design/glyphs/arcade.box.svg
bbf1518dc224aa8f8df59f432cf5eb8b6e25b1f4a4e4676fe231d84770dac156  src/design/glyphs/arcade.clipboard.svg
54c0b3e869912313a7b543175ad6bda5853b0bc8118d753e2c67de6dfdd07df9  src/design/glyphs/arcade.find.svg
ba869f44636f28ea28ebd71c8e00b8206fd1eaa28ce23cfc628b8feac1bb556b  src/design/glyphs/arcade.lens.svg
ce15684dfd003b0beb7e21a6e8c5a080433a0e25b804e48f024f3ba17794279b  src/design/glyphs/arcade.look.svg
42ff3fbca18365d18875061617446aca6876bf5f740160412663d3460f81e0cc  src/design/glyphs/arcade.shelf.svg
dd2aa2c70bb8ebe8b597eb8440dfc032a548ff43f90f2ffd08ec00cd02b4fac2  src/design/glyphs/arcade.tools.svg
6718ccee834c345aac4b4a71ea83461412e5f4ae6f24179ab897192cf243ce63  src/design/glyphs/arcade.wheel.svg
63de4bfe801cbd796ce732d86b71a91ed85a8ab63af31586799835119aa5ada9  src-tauri/icons/128x128.png
67959269a0cb3704c965985ea3f541c2966cc13324448321f1ab344c60c79ed6  src-tauri/icons/128x128@2x.png
bb81e1dcdbfe60dcad5edabb2abe890c7451a0825541ac93305f3b58e60ddb8b  src-tauri/icons/32x32.png
fa3fe4902089c79c13c05527266bbd215544cae07db1fbae1dabf29648d952d0  src-tauri/icons/64x64.png
fe9da095682d7f84e3fadf2cabb5dcf2db31db31ed214bf3037786559f8bfdf8  src-tauri/icons/Square107x107Logo.png
cf5b37ebe25792bf71c614b22cb4c99f60ddbf882660aaa8ee0555c093545fb5  src-tauri/icons/Square142x142Logo.png
a61328106e2ec7b0071b51841fb60516aa8ef92d65dbf2455283985e461f7cb9  src-tauri/icons/Square150x150Logo.png
02c04c260b0816d606e6ebb92a64ac1c03164c5bede7a62c24ab0418ba1e429a  src-tauri/icons/Square284x284Logo.png
a84de0005dcc1138638a943b422e7f90e4cc3a49dccad6c15dcfc55bc001ce92  src-tauri/icons/Square30x30Logo.png
8884e535dfc191184b156fec2ca0daaba6810966353bb30f85cbb2578ac4f7dc  src-tauri/icons/Square310x310Logo.png
e4df1b0a8a26281c3c1ff78c44b86946d3c68511c006add621efa84fb25a8065  src-tauri/icons/Square44x44Logo.png
d355721c61076ddd63671a42fbd3431183252270edc74b29ef2d5504de76032d  src-tauri/icons/Square71x71Logo.png
e6631dd3ac4c007b30d71a1b6d00f754f77de2219da6ed97d06bd71e48d58024  src-tauri/icons/Square89x89Logo.png
76da7fe62ad9aed6709840377287ec5364d8dbaa1854f8d8f4c1e7bd660c3d1b  src-tauri/icons/StoreLogo.png
ca38842fdd654cd0cfa1427afd6cd1e3c79a7c8869d5cc189105f813eeda3f75  src-tauri/icons/icon.icns
92826207c62916c25737322db626f2631933604d20344985de45ab67a32decf8  src-tauri/icons/icon.ico
c792a7a325be394d3ead30ba7824b80b0833639f37ba558301ea9d26d1d822e7  src-tauri/icons/icon.png
57dfa7e1db9e82005ef7820a2ae1b9a4b2abd19e5f4b5a69e003cda5582032e2  assets/icon.svg
67959269a0cb3704c965985ea3f541c2966cc13324448321f1ab344c60c79ed6  assets/icon.png
```
