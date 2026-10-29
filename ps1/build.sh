#!/bin/sh
# Builds the PlayStation version: exports the game's data with the PC build, then
# compiles it with PSn00bSDK (fetched into ps1/tools/ on first use) into a CD image,
# ps1/build/haunted-hollow.bin + .cue.
#
#   ps1/build.sh            build
#   ps1/build.sh --run      build, then run it in PCSX-Redux (also fetched on first use)
#
# Extra CMake options can be passed in CMAKE_ARGS, e.g. CMAKE_ARGS=-DHH_DEBUG=ON.
set -e
here=$(cd "$(dirname "$0")" && pwd)
root=$(dirname "$here")
tools=$here/tools
sdk=$tools/PSn00bSDK-0.24-Linux
data=$here/build/data

if [ ! -x "$sdk/bin/mipsel-none-elf-gcc" ]; then
	echo "Fetching PSn00bSDK 0.24 (with its GCC for the PS1) into $tools"
	mkdir -p "$tools"
	curl -fL -o "$tools/psn00bsdk.zip" https://github.com/Lameguy64/PSn00bSDK/releases/download/v0.24/PSn00bSDK-0.24-Linux.zip
	(cd "$tools" && unzip -q -o psn00bsdk.zip && rm psn00bsdk.zip)
fi

echo "Exporting the game's data"
(cd "$root" && cargo build --release --quiet && ./target/release/haunted_hollow --export-ps1 "$data")

export PATH="$sdk/bin:$PATH"
export PSN00BSDK_LIBS="$sdk/lib/libpsn00b"
cmake -S "$here" -B "$here/build" -G Ninja -Wno-dev \
	-DCMAKE_TOOLCHAIN_FILE="$PSN00BSDK_LIBS/cmake/sdk.cmake" \
	-DCMAKE_BUILD_TYPE=Release -DHH_DATA="$data" -DHH_DEBUG=OFF -DHH_TEST_BOSS=OFF -DHH_BOTS=0 $CMAKE_ARGS > /dev/null
cmake --build "$here/build"
echo "Built $here/build/haunted-hollow.cue"

if [ "$1" = "--run" ]; then
	redux=$tools/pcsx-redux/PCSX-Redux-HEAD-x86_64.AppImage
	if [ ! -x "$redux" ]; then
		echo "Fetching PCSX-Redux into $tools/pcsx-redux"
		mkdir -p "$tools/pcsx-redux"
		manifest=https://distrib.app/storage/manifests/pcsx-redux/dev-linux-x64
		id=$(curl -fsS "$manifest/manifest.json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["builds"][0]["id"])')
		path=$(curl -fsS "$manifest/manifest-$id.json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["path"])')
		curl -fL -o "$tools/pcsx-redux.zip" "https://distrib.app$path"
		(cd "$tools" && unzip -q -o pcsx-redux.zip -d pcsx-redux && rm pcsx-redux.zip)
		chmod +x "$redux"
	fi
	exec "$redux" -run -iso "$here/build/haunted-hollow.cue"
fi
