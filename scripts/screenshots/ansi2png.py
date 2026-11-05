#!/usr/bin/env python3
"""Render `tmux capture-pane -e -p -N` output as a window screenshot.

usage: ansi2png.py IN.ansi OUT.png [--title TEXT]
"""
import re
import sys

from PIL import Image, ImageDraw, ImageFont

FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
FONT_BOLD = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf"
SIZE = 15
BASE, TEXT = (0x30, 0x34, 0x46), (0xC6, 0xD0, 0xF5)
# Catppuccin Frappe as the terminal palette: the TUI uses indexed colours.
ANSI16 = [
    "51576d", "e78284", "a6d189", "e5c890", "8caaee", "f4b8e4", "81c8be", "b5bfe2",
    "626880", "e78284", "a6d189", "e5c890", "8caaee", "f4b8e4", "81c8be", "a5adce",
]
ANSI16 = [tuple(int(c[i : i + 2], 16) for i in (0, 2, 4)) for c in ANSI16]


def indexed(n):
    if n < 16:
        return ANSI16[n]
    if n >= 232:
        v = 8 + (n - 232) * 10
        return (v, v, v)
    n -= 16
    lv = [0, 95, 135, 175, 215, 255]
    return (lv[n // 36], lv[n // 6 % 6], lv[n % 6])


def parse(text):
    """Lines of (char, fg, bg, bold, dim, underline, reverse) cells."""
    fg = bg = None
    bold = dim = ul = rev = False
    rows = []
    for line in text.split("\n"):
        row = []
        pos = 0
        for m in re.finditer(r"\x1b\[([0-9;:]*)m", line):
            for ch in line[pos : m.start()]:
                row.append((ch, fg, bg, bold, dim, ul, rev))
            pos = m.end()
            p = [int(x) if x else 0 for x in re.split(r"[;:]", m.group(1))] or [0]
            i = 0
            while i < len(p):
                c = p[i]
                if c == 0:
                    fg = bg = None
                    bold = dim = ul = rev = False
                elif c == 1:
                    bold = True
                elif c == 2:
                    dim = True
                elif c == 4:
                    ul = True
                elif c == 7:
                    rev = True
                elif c == 22:
                    bold = dim = False
                elif c == 24:
                    ul = False
                elif c == 27:
                    rev = False
                elif 30 <= c <= 37:
                    fg = ANSI16[c - 30]
                elif 90 <= c <= 97:
                    fg = ANSI16[c - 90 + 8]
                elif 40 <= c <= 47:
                    bg = ANSI16[c - 40]
                elif 100 <= c <= 107:
                    bg = ANSI16[c - 100 + 8]
                elif c == 39:
                    fg = None
                elif c == 49:
                    bg = None
                elif c in (38, 48):
                    if p[i + 1] == 5:
                        col, i = indexed(p[i + 2]), i + 2
                    else:
                        col, i = tuple(p[i + 2 : i + 5]), i + 4
                    if c == 38:
                        fg = col
                    else:
                        bg = col
                i += 1
        for ch in line[pos:]:
            row.append((ch, fg, bg, bold, dim, ul, rev))
        rows.append(row)
    return rows


def render(text, title):
    rows = parse(text.rstrip("\n"))
    font, fbold = ImageFont.truetype(FONT, SIZE), ImageFont.truetype(FONT_BOLD, SIZE)
    cw = round(font.getlength("M"))
    ch = SIZE + 5
    cols = max(len(r) for r in rows)
    pad, bar = 18, 36
    w, h = cols * cw + pad * 2, len(rows) * ch + pad * 2 + bar
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle((0, 0, w - 1, h - 1), 12, fill=BASE + (255,), outline=(0x51, 0x57, 0x6D, 255))
    for i, c in enumerate(("e78284", "e5c890", "a6d189")):
        x = pad + i * 20
        d.ellipse((x, 12, x + 12, 24), fill="#" + c)
    if title:
        d.text((w / 2, 18), title, fill=(0xA5, 0xAD, 0xCE), font=font, anchor="mm")
    for y, row in enumerate(rows):
        for x, (c, fg, bg, bold, dim, ul, rev) in enumerate(row):
            f, b = fg or TEXT, bg or BASE
            if rev:
                f, b = b, f
            if dim:
                f = tuple((a + z) // 2 for a, z in zip(f, b))
            px, py = pad + x * cw, bar + pad + y * ch
            if b != BASE:
                d.rectangle((px, py, px + cw - 1, py + ch - 1), fill=b)
            if c != " ":
                d.text((px, py + 2), c, fill=f, font=fbold if bold else font)
            if ul:
                d.line((px, py + ch - 2, px + cw, py + ch - 2), fill=f)
    return img


if __name__ == "__main__":
    args = sys.argv[1:]
    title = args[args.index("--title") + 1] if "--title" in args else ""
    render(open(args[0], encoding="utf-8").read(), title).save(args[1])
