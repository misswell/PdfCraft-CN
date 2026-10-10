# PdfCraft CN app icon

<img src="pdfcraft.png" alt="PdfCraft CN app icon: a white PDF page with a red P and a red cn ribbon on a dark navy rounded tile" width="128">

**Picture:** a white document page with its top-right corner folded, a red `P` on it, and a red ribbon
banner reading `cn` across the bottom right — all on a dark navy rounded tile.

**Tile:** `pdfcraft.png` is a 1024 px full-bleed rounded square with transparent corners. Windows and
Linux icons use the full-bleed tile. macOS icons put it on Apple's grid (an 824 px body centred on a
transparent 1024 px canvas).

**Provenance:** made for PdfCraft CN by @misswell, who generated the artwork with ChatGPT on 2026-10-10
and owns it. The black corners of the generated canvas were keyed out to transparency and the art
resized to the 1024 px master; nothing else was changed and no third-party material is in it. Licensed
like the rest of the project: [LICENSE.txt](LICENSE.txt) (`MIT OR Apache-2.0`).

**Branch:** this icon lives on the `cn` branch of `misswell/PdfCraft-CN` only. Upstream PdfCraft ships
its own icon; nothing here is proposed to it.

## Files

| File | What it is |
|---|---|
| `pdfcraft.png` | the master tile, 1024 px full bleed; every other PNG, `.ico` and `.icns` is rendered from it |
| `pdfcraft-1024.png` | 1024 px on Apple's grid; also the runtime Dock icon on macOS |
| `pdfcraft.icns` | macOS icon (16–1024 px) |
| `pdfcraft.ico` | Windows icon (16–256 px), embedded in `pdfcraft.exe` by `apps/pdfcraft/build.rs` |
| `hicolor/<n>x<n>/apps/ai.storyteller.pdfcraft.png` | Linux hicolor theme, 16–512 px; the 256 px one is the runtime icon on Windows and Linux |

Where it shows: `apps/pdfcraft/src/main.rs` sets the window icon (Dock, taskbar, Alt-Tab, launcher) and the
Wayland app id `ai.storyteller.pdfcraft`; `packaging/linux/ai.storyteller.pdfcraft.desktop` names the
hicolor icon; `packaging/macos/package.sh` copies the `.icns` into the app bundle.

## Regenerate

```sh
packaging/icons.sh                          # needs python3 with Pillow; iconutil (macOS) for the .icns
python3 packaging/macos/dmg/generate.py     # the DMG background; needs resvg and pillow/ds_store/mac_alias
cargo xtask assets --write                  # regenerate ATTRIBUTION.md
cargo xtask assets                          # after updating the sha256 values in ATTRIBUTION.toml
```

`packaging/macos/dmg/background.svg` draws the master into the DMG window background, so an icon
change means re-running both of the first two commands.
