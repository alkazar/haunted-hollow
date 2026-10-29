#!/bin/sh
# Runs wii/build/hollow.dol in Dolphin (Flathub) without showing a window, recording
# every frame, then saves one PNG every STEP frames into OUT_DIR and deletes the
# recording. Build with CMAKE_ARGS=-DHH_BOTS=2 to have computer players play.
#
#   wii/test/run.sh OUT_DIR SECONDS [STEP]
#
# WIDE=1 sets the emulated Wii to 16:9.
set -e
out=$1
secs=$2
step=${3:-600}
here=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$out"
out=$(cd "$out" && pwd)
user=$out/dolphin-user
rm -rf "$user" "$out"/f_*.png
mkdir -p "$user"
cp "$here/build/hollow.dol" "$out/hollow.dol"
QT_QPA_PLATFORM=offscreen timeout "$secs" flatpak run --filesystem="$out" org.DolphinEmu.dolphin-emu \
	-u "$user" -b -v Vulkan -C Dolphin.Movie.DumpFrames=True -C Dolphin.Core.EmulationSpeed=0 \
	-C SYSCONF.IPL.AR=${WIDE:+True}${WIDE:-False} \
	-e "$out/hollow.dol" > "$out/dolphin.log" 2>&1 || true
avi=$(find "$user" -name '*.avi' | head -1)
ffmpeg -v quiet -i "$avi" -vf "select='not(mod(n\,$step))'" -vsync vfr "$out/f_%03d.png" || true
echo "$(ffprobe -v error -count_packets -show_entries stream=nb_read_packets -of csv=p=0 "$avi") frames"
rm -rf "$user" "$out/hollow.dol"
