#!/bin/sh
# Builds the Wii version: exports the game's data with the PC build, then compiles
# it in devkitPro's devkitPPC container (podman, or CONTAINER=docker) into
# wii/build/hollow.dol, plus a Homebrew Channel folder in wii/build/apps/.
#
#   wii/build.sh            build
#   wii/build.sh --run      build, then run it in Dolphin (the Flathub package)
#
# Extra CMake options can be passed in CMAKE_ARGS, e.g. CMAKE_ARGS=-DHH_DEBUG=ON.
set -e
here=$(cd "$(dirname "$0")" && pwd)
root=$(dirname "$here")
container=${CONTAINER:-podman}
image=docker.io/devkitpro/devkitppc:latest

echo "Exporting the game's data"
(cd "$root" && cargo build --release --quiet && ./target/release/haunted_hollow --export-wii "$here/build/data")

# The repository is mounted at /src so the shared code (../shared) is reachable.
$container run --rm -v "$root:/src:z" -w /src/wii "$image" sh -c "
	set -e
	/opt/devkitpro/portlibs/wii/bin/powerpc-eabi-cmake -S . -B build/cmake -DCMAKE_BUILD_TYPE=Release \
		-DHH_DATA=/src/wii/build/data -DHH_DEBUG=OFF -DHH_BOTS=0 $CMAKE_ARGS > /dev/null
	cmake --build build/cmake -j
	cp build/cmake/hollow.dol build/hollow.dol
"

# A folder for the Homebrew Channel (copy apps/ to the root of an SD card).
app=$here/build/apps/hauntedhollow
mkdir -p "$app"
cp "$here/build/hollow.dol" "$app/boot.dol"
cp "$here/meta.xml" "$app/meta.xml"
echo "Built $here/build/hollow.dol"

if [ "$1" = "--run" ]; then
	exec flatpak run org.DolphinEmu.dolphin-emu -e "$here/build/hollow.dol"
fi
