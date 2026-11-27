#!/usr/bin/env python3
"""Rasterizza un font con antialiasing in un file binario compatto per il kernel.
uso: genfont.py font.otf size out.bin
formato: "BFNT" u16 n, u8 ascent, u8 line_h, poi per glifo:
  u32 codepoint, u8 w, u8 h, i8 x_off, i8 y_off (dal top della riga), u8 advance, w*h byte di alfa
"""
import sys, struct
from PIL import Image, ImageDraw, ImageFont

path, size, out = sys.argv[1], int(sys.argv[2]), sys.argv[3]
f = ImageFont.truetype(path, size)
asc, desc = f.getmetrics()
chars = [chr(c) for c in range(32, 127)] + list("àèéìòùÀÈÉÌÒÙ°·…€—–«»’“”•✓×←→↑↓")
blob = b""
n = 0
for ch in chars:
    try:
        bbox = f.getbbox(ch)
    except Exception:
        continue
    adv = round(f.getlength(ch))
    x0, y0, x1, y1 = bbox
    w, h = max(0, x1 - x0), max(0, y1 - y0)
    img = Image.new("L", (max(w, 1), max(h, 1)), 0)
    ImageDraw.Draw(img).text((-x0, -y0), ch, font=f, fill=255)
    data = img.tobytes() if w and h else b""
    blob += struct.pack("<IBBbbB", ord(ch), w, h, x0, y0, adv) + data
    n += 1
open(out, "wb").write(b"BFNT" + struct.pack("<HBB", n, asc, asc + desc) + blob)
print(out, n, "glifi", len(blob), "byte, ascent", asc, "riga", asc + desc)
