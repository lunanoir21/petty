#!/usr/bin/env python3
"""Render a pet JSON to a PNG contact sheet (authoring aid, not needed at runtime).

usage: python3 tools/sheet.py pets/cat_black.json out.png [scale]
"""
import json
import struct
import sys
import zlib


def png(path, w, h, rgb):
    raw = b"".join(b"\x00" + bytes(rgb[y * w * 3:(y + 1) * w * 3]) for y in range(h))

    def chunk(tag, data):
        c = tag + data
        return struct.pack(">I", len(data)) + c + struct.pack(">I", zlib.crc32(c))

    hdr = struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)
    with open(path, "wb") as fh:
        fh.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", hdr)
                 + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def main():
    pet = json.load(open(sys.argv[1]))
    out = sys.argv[2]
    scale = int(sys.argv[3]) if len(sys.argv) > 3 else 6
    fw, fh_ = pet["frame"]["width"], pet["frame"]["height"]
    pal = {k: tuple(int(v[i:i + 2], 16) for i in (1, 3, 5)) for k, v in pet["palette"].items()}
    bg = (18, 18, 26)
    anims = list(pet["animations"].items())
    cols = max(len(a["frames"]) for _, a in anims)
    W = cols * (fw + 1) * scale
    H = len(anims) * (fh_ + 1) * scale
    buf = bytearray()
    for _ in range(W * H):
        buf += bytes(bg)

    def px(x, y, c):
        if 0 <= x < W and 0 <= y < H:
            i = (y * W + x) * 3
            buf[i:i + 3] = bytes(c)

    for r, (name, anim) in enumerate(anims):
        for c, frame in enumerate(anim["frames"]):
            ox, oy = c * (fw + 1) * scale, r * (fh_ + 1) * scale
            for y, row in enumerate(frame):
                for x, ch in enumerate(row):
                    if ch == pet.get("transparent", "."):
                        continue
                    col = pal[ch]
                    for dy in range(scale):
                        for dx in range(scale):
                            px(ox + x * scale + dx, oy + y * scale + dy, col)
    png(out, W, H, buf)
    print(f"{out} {W}x{H}  " + ", ".join(f"{n}:{len(a['frames'])}" for n, a in anims))


if __name__ == "__main__":
    main()
