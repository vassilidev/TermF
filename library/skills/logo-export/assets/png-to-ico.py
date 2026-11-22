#!/usr/bin/env python3
"""Packs square PNG files into one .ico, standard library only.

Usage: python3 png-to-ico.py favicon.ico icon-16.png icon-32.png icon-48.png
Each PNG is stored as is (PNG inside ICO, read by every browser and by Windows Vista and later).
"""
import struct
import sys


def size(png: bytes) -> tuple[int, int]:
    if png[:8] != b"\x89PNG\r\n\x1a\n" or png[12:16] != b"IHDR":
        raise SystemExit("not a PNG file")
    return struct.unpack(">II", png[16:24])


def main() -> None:
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    images = []
    for path in sys.argv[2:]:
        with open(path, "rb") as f:
            png = f.read()
        w, h = size(png)
        if w != h or w > 256:
            raise SystemExit(f"{path}: {w}x{h}, an icon is square and 256 px at most")
        images.append((w, png))
    images.sort()
    out = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    for w, png in images:
        # 0 in the one-byte size fields means 256.
        out += struct.pack("<BBBBHHII", w % 256, w % 256, 0, 0, 1, 32, len(png), offset)
        offset += len(png)
    out += b"".join(png for _, png in images)
    with open(sys.argv[1], "wb") as f:
        f.write(out)
    print(f"{sys.argv[1]}: {', '.join(f'{w}x{w}' for w, _ in images)}")


if __name__ == "__main__":
    main()
