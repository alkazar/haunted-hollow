#!/bin/sh
# Runs the CD image in PCSX-Redux (headless) and saves screenshots as PNG.
# Usage: test/run.sh OUT_DIR VBLANK,VBLANK,... [FRAME:BUTTON:FRAMES,...]  (from the ps1 directory)
set -e
out=$1
shots=$2
mkdir -p "$out"
rm -f "$out"/shot_*.raw
HH_SHOT_DIR=$out HH_SHOTS=$shots HH_PRESS=$3 timeout 400 tools/pcsx-redux/PCSX-Redux-HEAD-x86_64.AppImage \
	-no-ui -run -stdout -iso build/haunted-hollow.cue -dofile test/shots.lua > "$out/redux.log" 2>&1 || true
for f in "$out"/shot_*.raw; do
	size=$(stat -c %s "$f")
	if [ "$size" = 153600 ]; then
		python3 -I test/raw2png.py 320 240 "$f" "${f%.raw}.png"
	fi
done
grep -v 'BIOS Trace' "$out/redux.log" | grep -iE 'error|exception|missing|crash' || true
