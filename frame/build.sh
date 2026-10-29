#!/bin/bash
# Builds an arm64 release for the Steam Frame into target/frame/haunted-hollow/, ready for the
# SteamOS Devkit Client's Title Upload. Needs podman (or docker: CONTAINER=docker).
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
tool=${CONTAINER:-podman}
# The toolchain runs natively (x86-64) and cross-compiles: pin the platform, or a local arm64
# debian image would quietly make it all run under emulation.
"$tool" build --platform linux/amd64 -t haunted-hollow-frame "$root/frame"
# The cargo registry is shared with the host's; the build itself goes to its own target directory.
"$tool" run --rm -v "$root:/src:z" -v "${CARGO_HOME:-$HOME/.cargo}/registry:/opt/cargo/registry:z" \
    -w /src haunted-hollow-frame \
    cargo build --release --target aarch64-unknown-linux-gnu --target-dir target/frame-build
out="$root/target/frame/haunted-hollow"
rm -rf "$out" && mkdir -p "$out"
cp "$root/target/frame-build/aarch64-unknown-linux-gnu/release/haunted_hollow" "$out/"
cp "$root/frame/haunted-hollow.sh" "$out/"
echo "Built $out"
