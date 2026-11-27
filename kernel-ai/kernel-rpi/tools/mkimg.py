#!/usr/bin/env python3
"""ELF aarch64 -> immagine binaria grezza per il firmware del Raspberry (kernel8.img / kernel_2712.img).
Solo libreria standard: funziona anche su Windows senza objcopy.
uso: mkimg.py kernel.elf kernel8.img"""
import struct, sys
elf = open(sys.argv[1], "rb").read()
assert elf[:4] == b"\x7fELF" and elf[4] == 2, "serve un ELF a 64 bit"
phoff, = struct.unpack_from("<Q", elf, 0x20)
phentsize, phnum = struct.unpack_from("<HH", elf, 0x36)
segs = []
for i in range(phnum):
    p_type, _, off, vaddr, paddr, filesz, memsz, _ = struct.unpack_from("<IIQQQQQQ", elf, phoff + i * phentsize)
    if p_type == 1 and filesz > 0:
        segs.append((paddr, elf[off:off + filesz]))
segs.sort()
base = segs[0][0]
out = bytearray()
for addr, data in segs:
    o = addr - base
    if len(out) < o:
        out += b"\0" * (o - len(out))
    out[o:o + len(data)] = data
open(sys.argv[2], "wb").write(out)
print(f"{sys.argv[2]}: {len(out)} byte, base {base:#x}")
