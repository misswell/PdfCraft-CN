#!/usr/bin/env bash
# Regenerate every app icon from assets/app-icon/pdfcraft.png (the master tile: 1024 px, full bleed,
# transparent outside the rounded square).
#
# Needs python3 with Pillow (pip install pillow). On macOS, iconutil also writes the .icns. The
# outputs are committed, so building and packaging never need these tools. After running it, update
# the sha256 values in ATTRIBUTION.toml, then `cargo xtask assets --write`.
#
#   packaging/icons.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/assets/app-icon"
PNG="$DIR/pdfcraft.png"
ID="ai.storyteller.pdfcraft"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

[ -f "$PNG" ] || { echo "error: $PNG not found" >&2; exit 1; }

# The master is a full-bleed rounded tile: right for Windows and Linux. macOS icons follow Apple's
# grid instead: an 824 px body centred on a transparent 1024 px canvas.
python3 - "$PNG" "$DIR" "$ID" "$TMP" <<'PY'
import struct, sys
from PIL import Image

master_path, out_dir, app_id, tmp = sys.argv[1:5]
master = Image.open(master_path).convert("RGBA")


def tile(size):
    """The master at `size` px, full bleed."""
    return master.resize((size, size), Image.LANCZOS)


def apple_grid(size):
    """The tile on Apple's grid: the body at 824/1024 of the canvas, centred, padding transparent."""
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    body = tile(round(size * 824 / 1024))
    canvas.paste(body, ((size - body.width) // 2,) * 2, body)
    return canvas


# 1024 px on Apple's grid (also the runtime Dock icon on macOS, see apps/pdfcraft/src/main.rs).
apple_grid(1024).save(f"{out_dir}/pdfcraft-1024.png")

# Linux hicolor theme (full bleed; hicolor/256x256 is also the runtime icon on Windows and Linux).
for s in (16, 24, 32, 48, 64, 128, 256, 512):
    tile(s).save(f"{out_dir}/hicolor/{s}x{s}/apps/{app_id}.png")

# Windows .ico: PNG-compressed entries, 16-256 px.
blobs = []
for s in (16, 20, 24, 32, 40, 48, 64, 128, 256):
    path = f"{tmp}/ico-{s}.png"
    tile(s).save(path)
    blobs.append(open(path, "rb").read())
head = struct.pack("<HHH", 0, 1, len(blobs))
entries, data, offset = b"", b"", 6 + 16 * len(blobs)
for b in blobs:
    w, h = struct.unpack(">II", b[16:24])  # IHDR
    entries += struct.pack("<BBBBHHII", w % 256, h % 256, 0, 0, 1, 32, len(b), offset)
    data += b
    offset += len(b)
open(f"{out_dir}/pdfcraft.ico", "wb").write(head + entries + data)

# The iconset the .icns is built from.
import os

set_dir = f"{tmp}/{app_id}.iconset"
os.makedirs(set_dir, exist_ok=True)
for s in (16, 32, 128, 256, 512):
    apple_grid(s).save(f"{set_dir}/icon_{s}x{s}.png")
    apple_grid(s * 2).save(f"{set_dir}/icon_{s}x{s}@2x.png")
open(f"{tmp}/iconset", "w").write(set_dir)
PY

if command -v iconutil >/dev/null; then
  iconutil -c icns -o "$DIR/pdfcraft.icns" "$(cat "$TMP/iconset")"
else
  echo "warning: iconutil not found (macOS only); pdfcraft.icns not regenerated" >&2
fi
echo "icons written to $DIR"
