#!/usr/bin/env python3
"""Runs on the headset (upload.sh sends it): gives a devkit game its library artwork.

    python3 steam-art.py GAMEID ARTDIR

Steam keeps a devkit game as a shortcut in each user's config/shortcuts.vdf, and looks for a
shortcut's custom art in config/grid/ under the shortcut's app ID. ARTDIR holds frame/art/.
"""
import glob
import os
import shutil
import struct
import sys
import time

# frame/art file -> name in grid/, after the app ID.
NAMES = {'capsule.png': '', 'capsule_portrait.png': 'p', 'hero.png': '_hero', 'logo.png': '_logo',
         'icon.png': '_icon'}


def parse(data, i=0):
    """Binary VDF: 0 = nested map, 1 = string, 2 = int32, 8 = end of map."""
    out = {}
    while data[i] != 8:
        kind = data[i]
        end = data.index(b'\0', i + 1)
        key = data[i + 1:end].decode('utf-8', 'replace')
        i = end + 1
        if kind == 0:
            out[key], i = parse(data, i)
        elif kind == 1:
            end = data.index(b'\0', i)
            out[key] = data[i:end].decode('utf-8', 'replace')
            i = end + 1
        elif kind in (2, 4, 6):
            out[key] = struct.unpack_from('<I', data, i)[0]
            i += 4
        elif kind == 7:
            out[key] = struct.unpack_from('<Q', data, i)[0]
            i += 8
        else:
            raise ValueError(f'unknown VDF type {kind}')
    return out, i + 1


def find(gameid):
    """(grid folder, app ID) for every Steam user that has the devkit game."""
    found = []
    for vdf in glob.glob(os.path.expanduser('~/.steam/steam/userdata/*/config/shortcuts.vdf')):
        shortcuts = parse(open(vdf, 'rb').read())[0].get('shortcuts', {})
        for s in shortcuts.values():
            if {k.lower(): v for k, v in s.items()}.get('devkitgameid') == gameid:
                found.append((os.path.join(os.path.dirname(vdf), 'grid'), s['appid']))
    return found


def main():
    gameid, art = sys.argv[1:3]
    # Steam writes shortcuts.vdf shortly after it registers the game.
    for _ in range(10):
        found = find(gameid)
        if found:
            break
        time.sleep(1)
    else:
        print(f'{gameid} is not in any shortcuts.vdf: no artwork installed', file=sys.stderr)
        sys.exit(1)
    for grid, appid in found:
        os.makedirs(grid, exist_ok=True)
        for src, suffix in NAMES.items():
            if os.path.exists(os.path.join(art, src)):
                shutil.copyfile(os.path.join(art, src), os.path.join(grid, f'{appid}{suffix}.png'))
        print(f'Artwork installed for app ID {appid} in {grid}')


if __name__ == '__main__':
    main()
