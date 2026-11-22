"""Draws TermF's menu bar icon: the app icon's three chevrons, black on transparent (a macOS template image).

Usage, from the repository: python3 -I app/src-tauri/icons/tray-icon.py app/src-tauri/icons/tray.png 44
"""
import math
import struct
import sys
import zlib

# The chevrons of app/src-tauri/icons/icon.svg (1024 canvas): (points, stroke width).
PATHS = [
    ([(541, 388), (691, 512), (541, 636)], 88),
    ([(329, 244), (435, 332), (329, 420)], 80),
    ([(329, 604), (435, 692), (329, 780)], 80),
]


def dist(p, a, b):
    (px, py), (ax, ay), (bx, by) = p, a, b
    dx, dy = bx - ax, by - ay
    t = max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)))
    return math.hypot(px - ax - t * dx, py - ay - t * dy)


def main():
    out = sys.argv[1]
    size = int(sys.argv[2]) if len(sys.argv) > 2 else 44
    x0, x1, y0, y1 = 289, 735, 204, 820
    pad = size * 0.06
    scale = (size - 2 * pad) / max(x1 - x0, y1 - y0)
    ox = (size - (x1 - x0) * scale) / 2 - x0 * scale
    oy = (size - (y1 - y0) * scale) / 2 - y0 * scale
    segs = []
    for pts, width in PATHS:
        pts = [(x * scale + ox, y * scale + oy) for x, y in pts]
        for a, b in zip(pts, pts[1:]):
            segs.append((a, b, width * scale / 2))
    n = 6
    rows = []
    for y in range(size):
        row = bytearray([0])
        for x in range(size):
            hit = 0
            for sy in range(n):
                for sx in range(n):
                    p = (x + (sx + 0.5) / n, y + (sy + 0.5) / n)
                    if any(dist(p, a, b) <= r for a, b, r in segs):
                        hit += 1
            row += bytes([0, 0, 0, round(255 * hit / (n * n))])
        rows.append(bytes(row))
    raw = b"".join(rows)

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    with open(out, "wb") as f:
        f.write(png)


main()
