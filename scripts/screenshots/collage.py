#!/usr/bin/env python3
"""usage: collage.py DIR OUT.png: lays lazygit, k9s, lazydocker and lfk out in a grid."""
import sys

from PIL import Image

src, dest = sys.argv[1], sys.argv[2]
tiles = [Image.open(f"{src}/{n}.png") for n in ("lazygit", "k9s", "lazydocker", "lfk")]
w = max(t.width for t in tiles)
h = max(t.height for t in tiles)
gap = 24
sheet = Image.new("RGBA", (w * 2 + gap, h * 2 + gap), (0, 0, 0, 0))
for i, t in enumerate(tiles):
    sheet.paste(t, ((i % 2) * (w + gap), (i // 2) * (h + gap)), t)
sheet.save(dest)
