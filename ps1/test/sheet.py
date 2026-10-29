#!/usr/bin/env python3
"""Tiles the shot_*.png files in a directory into sheet.png (two columns): sheet.py DIR"""
import glob, os, struct, sys, zlib

def load(path):
    d = open(path, 'rb').read()
    i, idat = 8, b''
    while i < len(d):
        n = struct.unpack('>I', d[i:i + 4])[0]
        t = d[i + 4:i + 8]
        if t == b'IHDR':
            w, h = struct.unpack('>II', d[i + 8:i + 16])
        if t == b'IDAT':
            idat += d[i + 8:i + 8 + n]
        i += 12 + n
    raw = zlib.decompress(idat)
    return w, h, [raw[y * (w * 3 + 1) + 1:(y + 1) * (w * 3 + 1)] for y in range(h)]

def main():
    d = sys.argv[1]
    files = sorted(glob.glob(os.path.join(d, 'shot_*.png')), key=lambda f: int(os.path.basename(f)[5:-4]))
    imgs = [load(f) for f in files]
    w, h = imgs[0][0], imgs[0][1]
    cols, rows = 2, (len(imgs) + 1) // 2
    out = []
    for r in range(rows):
        for y in range(h):
            line = b''
            for c in range(cols):
                k = r * cols + c
                line += imgs[k][2][y] if k < len(imgs) else b'\0' * (w * 3)
            out.append(b'\0' + line)
    def chunk(t, data):
        c = struct.pack('>I', len(data)) + t + data
        return c + struct.pack('>I', zlib.crc32(t + data) & 0xffffffff)
    png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w * cols, h * rows, 8, 2, 0, 0, 0))
    png += chunk(b'IDAT', zlib.compress(b''.join(out), 6)) + chunk(b'IEND', b'')
    open(os.path.join(d, 'sheet.png'), 'wb').write(png)

main()
