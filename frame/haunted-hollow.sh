#!/bin/sh
# Start command for the Steam Frame: the headset has no desktop to play on, so always start in VR.
cd "$(dirname "$0")" && exec ./haunted_hollow --vr "$@"
