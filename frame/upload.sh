#!/bin/bash
# Builds the game and installs it on a Steam Frame in Developer Mode, the way the SteamOS Devkit
# Client's Title Upload does: pair over HTTP, rsync the build over ssh, then ask the headset's
# Steam client to add it to the library. It shares the official client's key, so a Frame paired
# with either one works with both.
#
#   frame/upload.sh [HOST] [--run] [--no-build]
#
# HOST defaults to frame.local (the Frame's hostname is "frame" unless you changed it).
# --run starts the game on the headset after the upload.
set -euo pipefail

host=frame.local run=0 build=1
for a in "$@"; do
    case $a in
        --run) run=1 ;;
        --no-build) build=0 ;;
        -*) echo "unknown option $a" >&2; exit 2 ;;
        *) host=$a ;;
    esac
done

root=$(cd "$(dirname "$0")/.." && pwd)
# Steam refuses devkit game IDs with a hyphen ("missing/invalid arguments").
gameid=HauntedHollow
# The runtime the devkit client offers for native arm64 Linux games. The build targets Debian 11's
# glibc, so it runs in this and in the older 3.0 (sniper) arm64 runtime.
runtime=${FRAME_RUNTIME:-SteamLinuxRuntime_4-arm64}
port=32000
conf=${XDG_CONFIG_HOME:-$HOME/.config}/steamos-devkit
key=$conf/devkit_rsa
utils=${XDG_CACHE_HOME:-$HOME/.cache}/haunted-hollow/steamos-devkit
# Valve's MIT-licensed devkit client, for the helper scripts it installs on the headset.
utils_repo=https://gitlab.steamos.cloud/devkit/steamos-devkit.git
# Sent along with the key when pairing, as the official client does.
magic=900b919520e4cf601998a71eec318fec

for tool in ssh rsync curl git python3; do
    command -v $tool >/dev/null || { echo "$tool is needed" >&2; exit 1; }
done

[ $build = 1 ] && "$root/frame/build.sh"
[ -x "$root/target/frame/haunted-hollow/haunted_hollow" ] || { echo "no build: run frame/build.sh" >&2; exit 1; }

if [ ! -f "$key" ]; then
    mkdir -p "$conf"
    ssh-keygen -q -t rsa -b 2048 -m PEM -N '' -C "devkit-client:$USER@$(hostname)" -f "$key"
    chmod 400 "$key" "$key.pub"
fi

json() { python3 -c "import json,sys; print(json.load(sys.stdin)$1)"; }

login=$(curl -sf --max-time 5 "http://$host:$port/properties.json" | json "['login']" 2>/dev/null ||
    curl -sf --max-time 5 "http://$host:$port/login-name") || {
    echo "No devkit service at $host:$port. Is the Frame on, awake and in Developer Mode?" >&2
    exit 1
}
ssh_opts=(-i "$key" -o IdentitiesOnly=yes -o BatchMode=yes -o ConnectTimeout=5
    -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile="$conf/known_hosts")
remote() { ssh "${ssh_opts[@]}" "$login@$host" "$@"; }

if ! remote true 2>/dev/null; then
    echo "Pairing with $host: confirm the request on the headset..."
    curl -sf --max-time 120 -H 'Content-Type: text/plain' \
        --data-binary "$(cat "$key.pub") $magic" "http://$host:$port/register" >/dev/null || {
        echo "Pairing failed or was declined." >&2
        exit 1
    }
    remote true
fi

if [ -d "$utils/.git" ]; then
    git -C "$utils" pull -q --ff-only || echo "(could not update the devkit scripts, using the cached copy)"
else
    git clone -q --depth 1 "$utils_repo" "$utils"
fi
rsh="ssh ${ssh_opts[*]@Q}"
rsync -az --chmod=Du=rwx,Dgo=rx,Fu=rwx,Fog=rx -e "$rsh" "$utils/client/devkit-utils/" "$login@$host:devkit-utils/"

dest=$(remote python3 devkit-utils/steamos-prepare-upload --gameid $gameid | json "['directory']")
echo "Uploading to $host:$dest"
rsync -az --delete --chmod=Du=rwx,Dgo=rx,Fu=rwx,Fog=rx -e "$rsh" \
    "$root/target/frame/haunted-hollow/" "$login@$host:$dest/"

parms=$(python3 -c 'import json,sys; print(json.dumps({
    "gameid": sys.argv[1], "directory": sys.argv[2], "argv": ["haunted-hollow.sh"], "env": {},
    "settings": {"steam_play": "0", "compat_tool": sys.argv[3]}, "force_appid": "", "lepton_args": ""}))' \
    $gameid "$dest" "$runtime")
result=$(remote python3 devkit-utils/steam-client-create-shortcut --parms "${parms@Q}")
if echo "$result" | json ".get('error') or ''" | grep .; then
    exit 1
fi
echo "Installed: it is in the headset's library as \"Devkit Game: $gameid\"."

if [ $run = 1 ]; then
    remote python3 devkit-utils/steam-devkit-rpc run-game gameid=$gameid >/dev/null
    echo "Started on the headset."
fi
