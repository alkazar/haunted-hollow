# Haunted Hollow 🎃

> **Note:** This game was made with the help of [Claude Opus 5.5](https://www.anthropic.com/claude),
> an AI model by Anthropic, working in Claude Code. The code, world, models, textures, music and
> sound effects were all written with Claude, under human direction.

A Halloween on-rails shooter for **1–4 players with Wii Remotes**, made for young kids and up.
Everyone rides a roller-coaster cart through a blocky haunted-house attraction — graveyard,
grand foyer, haunted library, dining hall, a big drop into the crypt — and pops ghosts, bats,
pumpkins, flying books, waving skeletons and dangling spiders into candy confetti. At the end
the **Pumpkin King** rises from his pumpkin patch in front of a pair of grand red-and-gold doors
and blocks the track until the riders beat him. Then the doors swing open and the ride rolls out
through them, down a lantern-lit hall that fades away into darkness, to the score screen.

![Title screen with four players joined](docs/screenshots/title.png)

| | |
|:---:|:---:|
| ![The "how to shoot" tutorial with a practice target per player](docs/screenshots/tutorial.png) | ![Popping ghosts and bats in the graveyard](docs/screenshots/graveyard.png) |
| Everyone pops a practice target before the countdown | The graveyard |
| ![The grand foyer](docs/screenshots/foyer.png) | ![The haunted library](docs/screenshots/library.png) |
| The grand foyer | The haunted library |
| ![The dining hall](docs/screenshots/dining-hall.png) | ![Plunging down the big drop](docs/screenshots/drop.png) |
| The dining hall | The big drop |
| ![The misty green crypt](docs/screenshots/crypt.png) | ![Four players battling the Pumpkin King](docs/screenshots/boss.png) |
| The creepy crypt | The Pumpkin King |
| ![The score screen with awards](docs/screenshots/results.png) | |
| Scores and awards | |

Nobody can lose: there is no health and no game over, just a score per player and a fun
award on the results screen ("GHOST CATCHER", "BAT BOPPER", "BOSS BUSTER"...).

* Ride: exactly **120 seconds** from the station to the boss (the track's speed profile is
  normalised to that), then the boss fight lasts as long as it takes (~15–40 s depending on skill
  and player count; his health scales with the number of players).
* Each player has their own crosshair colour: **P1 red, P2 blue, P3 green, P4 yellow**.
* Kid-friendly aiming: generous hit areas with a minimum on-screen size, targets gently pulse.
* Rust + Vulkan (ash) + winit, Minecraft-style voxel world with procedural 16×16 textures,
  coloured flood-fill block lighting, smooth lighting/ambient occlusion and a rider's lantern.
  No asset files: all textures, models and the world are generated in code.
* Music and sound: original tunes (spooky theremin title, creepy music-box ride theme,
  brassy boss battle, victory fanfare, happy results theme) and ~20 effects, all synthesized in
  code at start-up. Sound goes out through ALSA, loaded at runtime (works with PipeWire /
  PulseAudio via their ALSA bridge). `--mute` turns it off, `HAUNTED_ALSA_DEVICE=hw:1,0`
  picks a different device, and `--dump-audio DIR` writes every track and effect as WAV files.
* Targets are only spawned where every rider can see and reach them: each spot is checked
  against the ride's predicted camera over the target's whole life. The whole body has to be in
  open air (never inside a wall, the floor or a prop), every edge has to be visible (nothing in
  the way), it can't overlap another target, and it must stay clear of the score panels and the
  boss bar. New targets are steered to different parts of the screen so they don't bunch up.

## Download

Prebuilt Linux (x86_64) builds are on the [Releases](https://github.com/alkazar/haunted-hollow/releases)
page. Grab `haunted-hollow-<version>-linux-x86_64.tar.gz`, unpack it and run `./haunted-hollow`.
You need a Vulkan driver (Mesa or NVIDIA).

To publish a new release, push a version tag: `git tag v1.0.0 && git push origin v1.0.0`.
The [release workflow](.github/workflows/release.yml) tests, builds and uploads it. The tag is
the version: the game shows it on the title screen and with `--version`. Local builds use
`git describe --tags` instead (e.g. `v1.0.0-3-gabc1234`, three commits after `v1.0.0`), so the
version in `Cargo.toml` doesn't need updating.

## Build & run

Requirements: Linux, a Rust toolchain (1.77+), a Vulkan driver (Mesa or NVIDIA).
Shaders are WGSL compiled to SPIR-V at build time by `naga`, so no `glslc` is needed.

```sh
cargo build --release
./target/release/haunted_hollow            # fullscreen, Wii Remotes
./target/release/haunted_hollow --debug    # windowed, play with the mouse
./target/release/haunted_hollow --help
```

## Playing with Wii Remotes

### Connecting Wii Remotes

You need Bluetooth, the remotes, and an IR sensor bar. There's nothing to pair or install: the
game finds and connects the remotes itself.

**1. Connect each remote**

With the game running, press **1** and **2** on the remote together. Its four LEDs blink while
it's looking for the game; when it connects all four light up steadily and it gives a short
buzz. Then press **A** to join, and only your player's LED stays lit. Connect up to four remotes this way, at any time (also mid-ride).
Remotes don't stay paired: after the game quits (or a remote turns off), press 1+2 again.

The game talks to the remotes directly over Bluetooth, so the `hid-wiimote` kernel driver,
udev rules and controller remappers like InputPlumber don't get involved. It only needs
BlueZ running and the adapter powered on (`bluetoothctl power on`).

**2. Set up the sensor bar**

The remote aims by seeing two IR lights, so you need a **sensor bar**: a Wii console's bar (the
Wii must be switched on), or any USB- or battery-powered one. Put it centred above or below the
screen. Without it the buttons still work, but there's no crosshair.

**Troubleshooting**

* **Run with `--debug` and press F3.** The overlay shows how many remotes the game sees, how
  many reports per second each one sends and the raw IR dots it picks up. The terminal logs
  each remote connecting and disconnecting (`[wiimote] ...`). Remotes but no dots means the
  remote can't see the sensor bar.
* **A remote never connects.** If it was paired before (e.g. with `bluetoothctl`), BlueZ
  hands it to the kernel driver instead of the game, and the game logs
  `... is paired with BlueZ`. Unpair it once with `bluetoothctl remove XX:XX:XX:XX:XX:XX`.
  Also check that no other program (Dolphin, say) is connecting remotes at the same time.
* **The crosshair moves the wrong way or doesn't reach the edges.** See the pointer calibration
  flags below.

### Playing

Remotes are hot-plugged at any time. On the title screen each player presses **A** to join and
gets the next colour; anyone presses **+** to start (it also auto-starts 15 s after the first
join). Late arrivals can press A mid-ride to jump in. After the score screen everyone goes
back to the title and presses **A** again to play another round.

| Button | Action |
|---|---|
| B (trigger) or A | Shoot (hold for auto-fire) / join |
| + | Start the ride |
| HOME | Pause / resume |
| − (while paused) | Back to the title screen |

Remotes rumble when you pop something.

**High scores**: after the score screen, every rider whose score makes the top ten shoots the
letters of their initials (up to three) on a big letter grid, then **END** (**DEL** removes a
letter). Anyone who hasn't finished after 45 s keeps what they shot, or goes in as P1–P4. The
table is then shown before the title, and the best score is shown on the title screen. It's
saved in `~/.local/share/haunted-hollow/highscores.txt` (or under `$XDG_DATA_HOME`); delete
that file to reset it. Computer players (`--bots`) never enter the table.

**Pointer calibration** (if the crosshair moves the wrong way or doesn't reach the edges):
`--wii-flip-x`, `--wii-flip-y`, `--wii-gain 1.6`, `--wii-y-offset -0.1` (for a bar below the TV).
In `--debug` mode the overlay (F3) shows each remote's raw IR dots.

## Debug / mouse mode

`--debug` runs windowed with an info overlay and lets the mouse play:

| Input | Action |
|---|---|
| Mouse | Aim (player 1 by default) |
| Left click | Shoot / join |
| 1 – 4 | Make the mouse control that player (joins them) — test multiplayer scoring |
| Enter / Space / right click | Start |
| B | Skip straight to the boss |
| Tab (hold) | Fast-forward ×4 |
| P | Pause, Backspace while paused = title |
| F3 | Toggle debug overlay · F11 fullscreen · Esc quit |

Other handy flags: `--bots N` (computer players, an attract/demo mode), `--boss`,
`--timescale X`, `--size 1920x1080`, and `--shots 10,60 --exit-after 70` to save screenshots.

## VR mode

`--vr` plays the ride in a VR headset through OpenXR, for one rider with the headset's
controllers. Start an OpenXR runtime first (SteamVR, or Monado with `monado-service`), then
run `./target/release/haunted_hollow --vr`. It needs the OpenXR loader
(`libopenxr_loader.so.1`, Fedora package `openxr-libs`, Debian/Ubuntu `libopenxr-loader1`).

* The game runs exactly as on a TV. The cart's camera always faces forward and targets
  appear in front of it as usual. You can look around and lean to admire the scenery.
* The HUD (crosshair, scores, banners) floats on a virtual screen 9 m ahead that fills the
  camera's view, so it lines up with the world behind it.
* The controller is a laser pointer. Where its beam meets the virtual screen is your aim,
  and whichever hand last pulled its trigger does the aiming.

| Touch-style controllers (also Steam Frame) | Index | Action |
|---|---|---|
| Trigger | Trigger | Shoot / join |
| A / X | A | Start |
| Left menu | B | Pause |
| B / Y (while paused) | Thumbstick click (while paused) | Back to the title screen |

The game holds still while the runtime's own menu has your attention. Ctrl+C quits.

### Standalone on the Steam Frame

The Frame runs native arm64 Linux games in an arm64 Steam Linux Runtime (3.0 sniper is
Debian 11, 4.0 is newer).
`frame/build.sh` cross-compiles one in a Debian 11 container (podman, or `CONTAINER=docker`)
and puts it in `target/frame/haunted-hollow/`. The arm64 build has the OpenXR loader linked
in, so nothing else needs to be installed on the headset.

To install it, put the Frame in Developer Mode (Settings → System), open Settings →
Developer → Pair new host, and run:

```
frame/upload.sh --run        # or: frame/upload.sh <hostname or IP> --run
```

It builds, pairs on first use (confirm on the headset), uploads with rsync over ssh, adds
the game to the headset's library as "Devkit Game: HauntedHollow" and, with `--run`,
starts it. It does what the SteamOS Devkit Client's Title Upload does, with the same key
(`~/.config/steamos-devkit/devkit_rsa`) and Valve's helper scripts. `--no-build` uploads
the last build, and `FRAME_RUNTIME=SteamLinuxRuntime_sniper-arm64` (for example) picks a
different runtime than the default `SteamLinuxRuntime_4-arm64`.

The official client works too: Title Upload with **Local Folder**
`target/frame/haunted-hollow`, **Start Command** `haunted-hollow.sh` (which adds `--vr`)
and an ARM64 Steam Linux Runtime.

## Code tour

| File | What |
|---|---|
| `src/vk.rs` | Vulkan renderer: swapchain, world + HUD pipelines, atlas, quad batching, screenshots, VR eye targets |
| `src/xr.rs` | VR mode: OpenXR session, head pose inside the cart, controller aiming, the HUD's virtual screen |
| `shaders/*.wgsl` | World shader (baked light, rider's lantern, fog) and HUD shader |
| `src/atlas.rs` | Procedural Minecraft-style block textures + 8×8 pixel font |
| `src/world.rs` | Voxel world generation, RGB flood-fill lighting + moonlight, meshing with AO |
| `src/track.rs` | Coaster spline, speed profile normalised to a 120 s ride, banking |
| `src/mesh.rs` | Voxel models: ghost, bat, pumpkin, skeleton, spider, books, candy, Pumpkin King, boss doors |
| `src/game.rs` | Spawning, behaviours, hit testing, scoring, boss, title/results screens |
| `src/synth.rs` | Tiny synthesizer: instruments, drums, the songs and every sound effect |
| `src/audio.rs` | Real-time mixer thread, crossfading music, ALSA output via dlopen |
| `src/wiimote.rs` | Wii Remotes over Bluetooth L2CAP: BlueZ discovery, IR camera setup, buttons, IR pointer, rumble, LEDs |
| `src/highscores.rs` | The top-ten table and its save file |
| `src/main.rs` | Window, input, game loop, command-line flags |
| `frame/` | Steam Frame: cross-compiling container, start script, headless devkit upload |

Tuning knobs: spawn pools and density in `Game::build_schedule`, points/hit sizes in `Kind`,
boss health in `update` (`40 + 22 × players`), track shape and section speeds in `track.rs`.

## License

MIT — see [LICENSE](LICENSE).
