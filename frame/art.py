#!/usr/bin/env python3
"""Makes the Steam library artwork in frame/art/ from scenes rendered by the game itself.

    cargo build --release && frame/art.py

Runs target/release/haunted_hollow with --no-hud to capture the scenes (it opens a window for a
few minutes), then lays the title over them in the game's 8x8 font. Needs Pillow. upload.sh copies
the results into the headset's Steam grid folder.
"""
import glob
import os
import re
import subprocess
import sys
import tempfile

from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, 'frame', 'art')
GAME = os.path.join(ROOT, 'target', 'release', 'haunted_hollow')
# Colours from the title screen (Game::draw_title).
ORANGE = (255, 140, 20)
LILAC = (191, 140, 255)


def font():
    """The font8x8 crate's BASIC_LEGACY table, the font the game paints into its atlas."""
    home = os.environ.get('CARGO_HOME', os.path.expanduser('~/.cargo'))
    src = sorted(glob.glob(f'{home}/registry/src/*/font8x8-*/src/legacy.rs'))
    if not src:
        sys.exit('font8x8 source not found: build the game first')
    text = open(src[-1]).read()
    body = text[text.index('pub const BASIC_LEGACY'):]
    body = body[body.index('= [') + 3:]
    glyphs = []
    for m in re.finditer(r'NOTHING_TO_DISPLAY|\[([^\]]*)\]', body):
        glyphs.append([0] * 8 if m.group(1) is None else [int(v, 16) for v in m.group(1).split(',') if v.strip()])
        if len(glyphs) == 128:
            break
    return glyphs


GLYPHS = font()


def text(img, s, cx, y, px, colour, shadow=0.8):
    """Draws `s` centred on cx, `px` image pixels per font pixel, with the game's drop shadow."""
    d = ImageDraw.Draw(img, 'RGBA')
    x0 = cx - len(s) * 8 * px / 2
    for ox, oy, col in ((px, px, (0, 0, 0, int(255 * shadow))), (0, 0, colour + (255,))):
        for i, ch in enumerate(s):
            for gy, row in enumerate(GLYPHS[min(ord(ch), 127)]):
                for gx in range(8):
                    if row & (1 << gx):
                        x = x0 + (i * 8 + gx) * px + ox
                        d.rectangle([x, y + gy * px + oy, x + px - 1, y + (gy + 1) * px + oy - 1], fill=col)


def fit(s, width, frac):
    """Font pixel size that makes `s` take up `frac` of `width`."""
    return int(width * frac / (len(s) * 8))


def band(img, y0, y1, alpha=0.35):
    d = ImageDraw.Draw(img, 'RGBA')
    d.rectangle([0, y0, img.width, y1], fill=(0, 0, 0, int(255 * alpha)))


def capture(size, t, extra=()):
    """Renders the scene `t` seconds into an attract-mode run at `size`, without the HUD."""
    with tempfile.TemporaryDirectory() as tmp:
        subprocess.run([GAME, '--windowed', '--mute', '--no-hud', '--size', f'{size[0]}x{size[1]}', *extra,
                        '--shots', str(t), '--shot-dir', tmp, '--exit-after', str(t + 0.1)], check=True)
        return Image.open(glob.glob(f'{tmp}/*.ppm')[0]).convert('RGB').resize(size, Image.LANCZOS)


def cover(img, size):
    """Scales and centre-crops to fill `size`."""
    s = max(size[0] / img.width, size[1] / img.height)
    img = img.resize((round(img.width * s), round(img.height * s)), Image.LANCZOS)
    x, y = (img.width - size[0]) // 2, (img.height - size[1]) // 2
    return img.crop((x, y, x + size[0], y + size[1]))


def main():
    if not os.access(GAME, os.X_OK):
        sys.exit(f'{GAME} not found: run cargo build --release')
    os.makedirs(OUT, exist_ok=True)
    # The title screen's gate, and the boss at the end of the ride (with a bot riding along).
    gate = capture((2560, 1440), 1)
    gate_tall = capture((1200, 1800), 1)
    boss = capture((3840, 1240), 130, ['--bots', '1'])

    # Header / wide capsule (Steam shows it in recent games and the library list).
    img = cover(gate, (920, 430))
    px = fit('HAUNTED HOLLOW', 920, 0.88)
    band(img, 30, 60 + 8 * px + 36)
    text(img, 'HAUNTED HOLLOW', 460, 44, px, ORANGE)
    text(img, 'THE SPOOKY COASTER RIDE', 460, 52 + 8 * px + 8, fit('THE SPOOKY COASTER RIDE', 920, 0.62), LILAC)
    img.save(f'{OUT}/capsule.png')

    # Portrait capsule (the library grid).
    img = cover(gate_tall, (600, 900))
    px = fit('HOLLOW', 600, 0.8)
    band(img, 40, 70 + 16 * px + 50)
    text(img, 'HAUNTED', 300, 54, fit('HAUNTED', 600, 0.86), ORANGE)
    text(img, 'HOLLOW', 300, 62 + 8 * px + 4, px, ORANGE)
    text(img, 'THE SPOOKY COASTER RIDE', 300, 72 + 16 * px + 10, fit('THE SPOOKY COASTER RIDE', 600, 0.84), LILAC)
    img.save(f'{OUT}/capsule_portrait.png')

    # Hero (the banner at the top of the game's page); Steam puts the logo over it.
    cover(boss, (3840, 1240)).save(f'{OUT}/hero.png')

    # Logo, transparent.
    px = 10
    img = Image.new('RGBA', (len('HAUNTED HOLLOW') * 8 * px + px, 8 * px + 70), (0, 0, 0, 0))
    text(img, 'HAUNTED HOLLOW', img.width // 2 - px // 2, 0, px, ORANGE)
    text(img, 'THE SPOOKY COASTER RIDE', img.width // 2, 8 * px + 22, 4, LILAC)
    img.save(f'{OUT}/logo.png')

    # Icon: the initials on the gate.
    img = cover(gate, (256, 256))
    text(img, 'HH', 128, 128 - 8 * 14 // 2, 14, ORANGE)
    img.save(f'{OUT}/icon.png')
    print(f'Wrote {OUT}')


if __name__ == '__main__':
    main()
