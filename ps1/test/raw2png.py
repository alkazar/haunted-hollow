#!/usr/bin/env python3
"""Converts PCSX-Redux 16-bit screenshots (raw 15-bit BGR) to PNG: raw2png.py W H in.raw out.png"""
import struct, sys, zlib

def main():
    w, h, src, dst = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3], sys.argv[4]
    data = open(src, 'rb').read()
    rows = []
    for y in range(h):
        row = bytearray([0])
        for x in range(w):
            (v,) = struct.unpack_from('<H', data, (y * w + x) * 2)
            r, g, b = v & 31, (v >> 5) & 31, (v >> 10) & 31
            row += bytes([r << 3 | r >> 2, g << 3 | g >> 2, b << 3 | b >> 2])
        rows.append(bytes(row))
    def chunk(t, d):
        c = struct.pack('>I', len(d)) + t + d
        return c + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
    png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0))
    png += chunk(b'IDAT', zlib.compress(b''.join(rows), 9)) + chunk(b'IEND', b'')
    open(dst, 'wb').write(png)

main()
