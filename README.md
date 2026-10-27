# Haunted Hollow 🎃

A Halloween on-rails shooter for **1–4 players with Wii Remotes**, made for young kids and up.
Everyone rides a roller-coaster cart through a blocky haunted-house attraction — graveyard,
grand foyer, haunted library, dining hall, a big drop into the crypt — and pops ghosts, bats,
pumpkins, flying books, waving skeletons and dangling spiders into candy confetti. At the end
the **Pumpkin King** rises from his pumpkin patch in front of a pair of grand red-and-gold doors
and blocks the track until the riders beat him. Then the doors swing open and the ride rolls out
through them, down a lantern-lit hall that fades away into darkness, to the score screen.

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

The game reads remotes through the kernel's `hid-wiimote` driver (in every mainstream kernel).

1. **Pair** each remote (once):
   ```sh
   bluetoothctl
   [bluetooth]# scan on          # now press the red SYNC button (or 1+2) on the remote
   [bluetooth]# pair XX:XX:XX:XX:XX:XX
   [bluetooth]# trust XX:XX:XX:XX:XX:XX
   [bluetooth]# connect XX:XX:XX:XX:XX:XX
   ```
2. **Permissions**: the game opens `/dev/input/event*`. Either add yourself to the `input`
   group (`sudo usermod -aG input $USER`, then log out/in) or install a udev rule:
   ```
   # /etc/udev/rules.d/70-wiimote.rules
   SUBSYSTEM=="input", ATTRS{name}=="Nintendo Wii Remote*", TAG+="uaccess", MODE="0660"
   SUBSYSTEM=="leds", KERNEL=="*:blue:p*", RUN+="/bin/chmod a+w /sys%p/brightness"
   ```
   (The second line is optional; it lets the game light each remote's player LED.)
3. You need an **IR sensor bar** (a Wii console's bar while the Wii is on, or any USB /
   battery sensor bar) above or below the TV.

Remotes are hot-plugged at any time. On the title screen each child presses **A** to join and
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
