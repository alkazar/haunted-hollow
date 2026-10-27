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

You need Bluetooth, the remotes, and an IR sensor bar. Step 1 is needed once per remote and
step 2 once per computer.

**1. Pair each remote**

```sh
bluetoothctl
[bluetooth]# power on
[bluetooth]# pairable on
[bluetooth]# scan on
```

Press the red **SYNC** button under the battery cover (the LEDs start blinking). When it shows
up as `Nintendo RVL-CNT-01` (or `Nintendo RVL-CNT-01-TR` for newer remotes), pair it using its
address:

```
[bluetooth]# pair XX:XX:XX:XX:XX:XX
[bluetooth]# trust XX:XX:XX:XX:XX:XX
[bluetooth]# connect XX:XX:XX:XX:XX:XX
[bluetooth]# scan off
```

If pairing fails, try again and press SYNC just before `pair`. Once a remote is trusted,
pressing any button on it reconnects it. `bluetoothctl devices Connected` lists what's
connected.

**2. Let the game read the remotes (udev rule)**

The game reads remotes through the kernel's `hid-wiimote` driver (in every mainstream kernel),
which turns each remote into several input devices (`/dev/input/event*`) that normally only
root can open. Install a udev rule that gives the logged-in user access:

```sh
sudo tee /etc/udev/rules.d/70-wiimote.rules <<'EOF'
SUBSYSTEM=="input", ATTRS{name}=="Nintendo Wii Remote*", TAG+="uaccess", MODE="0660"
SUBSYSTEM=="leds", KERNEL=="*:blue:p*", RUN+="/bin/chmod a+w /sys%p/brightness"
EOF
sudo udevadm control --reload && sudo udevadm trigger
```

The first rule is the important one; the second is optional and lets the game light each
remote's player LED. Then turn the remote off (hold its power button) and reconnect it so the
rule applies.

(Alternatively, add yourself to the `input` group with `sudo usermod -aG input $USER` and log
out and back in. That works too, but gives access to every input device, keyboards included.)

To check it worked, with a remote connected:

```sh
grep -A5 "Wii Remote" /proc/bus/input/devices   # "Nintendo Wii Remote", "... IR", etc.
ls -l /dev/input/event*                          # the remote's devices: crw-rw----+
```

**3. Set up the sensor bar**

The remote aims by seeing two IR lights, so you need a **sensor bar**: a Wii console's bar (the
Wii must be switched on), or any USB- or battery-powered one. Put it centred above or below the
screen. Without it the buttons still work, but there's no crosshair.

**Troubleshooting**

* **Run with `--debug` and press F3.** The overlay shows how many remotes the game sees and the
  raw IR dots each one picks up. No remotes listed means a pairing or permissions problem;
  remotes but no dots means the remote can't see the sensor bar.
* **The remote's devices show `c---------` (mode 0000) even with the rule installed.** Another
  program is hiding the remote so that only it can read it. **InputPlumber** (a controller
  remapper on some gaming-focused distros) does this, and its rules are in
  `/run/udev/rules.d/50-inputplumber-hide-*.rules`. Stop it, clear its rules, then reconnect
  the remote:
  ```sh
  sudo systemctl stop inputplumber
  sudo rm /run/udev/rules.d/50-inputplumber-hide-*.rules
  sudo udevadm control --reload
  ```
  If you don't need InputPlumber, `sudo systemctl mask --now inputplumber` stops it from coming
  back (undo with `sudo systemctl unmask inputplumber`).
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

## Code tour

| File | What |
|---|---|
| `src/vk.rs` | Vulkan renderer: swapchain, world + HUD pipelines, atlas, quad batching, screenshots |
| `shaders/*.wgsl` | World shader (baked light, rider's lantern, fog) and HUD shader |
| `src/atlas.rs` | Procedural Minecraft-style block textures + 8×8 pixel font |
| `src/world.rs` | Voxel world generation, RGB flood-fill lighting + moonlight, meshing with AO |
| `src/track.rs` | Coaster spline, speed profile normalised to a 120 s ride, banking |
| `src/mesh.rs` | Voxel models: ghost, bat, pumpkin, skeleton, spider, books, candy, Pumpkin King, boss doors |
| `src/game.rs` | Spawning, behaviours, hit testing, scoring, boss, title/results screens |
| `src/synth.rs` | Tiny synthesizer: instruments, drums, the songs and every sound effect |
| `src/audio.rs` | Real-time mixer thread, crossfading music, ALSA output via dlopen |
| `src/wiimote.rs` | evdev Wii Remote reader: buttons, IR pointer, rumble, LEDs, hot-plug |
| `src/highscores.rs` | The top-ten table and its save file |
| `src/main.rs` | Window, input, game loop, command-line flags |

Tuning knobs: spawn pools and density in `Game::build_schedule`, points/hit sizes in `Kind`,
boss health in `update` (`40 + 22 × players`), track shape and section speeds in `track.rs`.

## License

MIT — see [LICENSE](LICENSE).
