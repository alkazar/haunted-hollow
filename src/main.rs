//! HAUNTED HOLLOW - a Halloween on-rails shooter for 1-4 players with Wii Remotes.
//! Rust + Vulkan (ash) + winit, Linux.

mod atlas;
mod audio;
mod synth;
mod game;
mod highscores;
mod hud;
mod mesh;
mod ps1export;
mod rng;
mod track;
mod vk;
mod wiimote;
mod world;
mod xr;

use game::{Game, SourceId, SourceInput, State};
use glam::{Mat4, Vec2};
use std::collections::HashSet;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

struct Args {
    debug: bool,
    fullscreen: bool,
    bots: u32,
    timescale: f32,
    shots: Vec<f32>,
    shot_dir: String,
    exit_after: Option<f32>,
    size: (u32, u32),
    ir: wiimote::IrConfig,
    boss: bool,
    mute: bool,
    dump_audio: Option<String>,
    export_ps1: Option<String>,
    vr: bool,
}

fn parse_args() -> Args {
    let mut a = Args {
        debug: false,
        fullscreen: true,
        bots: 0,
        timescale: 1.0,
        shots: vec![],
        shot_dir: ".".into(),
        exit_after: None,
        size: (1280, 720),
        ir: wiimote::IrConfig { flip_x: true, flip_y: false, gain: 1.35, y_offset: 0.0 },
        boss: false,
        mute: false,
        dump_audio: None,
        export_ps1: None,
        vr: false,
    };
    let mut windowed = false;
    let mut full = false;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let val = |i: &mut usize| -> String {
        *i += 1;
        args.get(*i).cloned().unwrap_or_default()
    };
    while i < args.len() {
        match args[i].as_str() {
            "--debug" | "-d" => a.debug = true,
            "--windowed" => windowed = true,
            "--fullscreen" => full = true,
            "--bots" => a.bots = val(&mut i).parse().unwrap_or(1),
            "--timescale" => a.timescale = val(&mut i).parse().unwrap_or(1.0),
            "--shots" => a.shots = val(&mut i).split(',').filter_map(|s| s.parse().ok()).collect(),
            "--shot-dir" => a.shot_dir = val(&mut i),
            "--exit-after" => a.exit_after = val(&mut i).parse().ok(),
            "--boss" => a.boss = true,
            "--mute" => a.mute = true,
            "--vr" => a.vr = true,
            "--dump-audio" => a.dump_audio = Some(val(&mut i)),
            "--export-ps1" => a.export_ps1 = Some(val(&mut i)),
            "--size" => {
                let v = val(&mut i);
                if let Some((w, h)) = v.split_once('x') {
                    a.size = (w.parse().unwrap_or(1280), h.parse().unwrap_or(720));
                }
            }
            "--wii-flip-x" => a.ir.flip_x = !a.ir.flip_x,
            "--wii-flip-y" => a.ir.flip_y = !a.ir.flip_y,
            "--wii-gain" => a.ir.gain = val(&mut i).parse().unwrap_or(1.35),
            "--wii-y-offset" => a.ir.y_offset = val(&mut i).parse().unwrap_or(0.0),
            "--version" | "-V" => {
                println!("haunted-hollow {}", env!("HAUNTED_VERSION"));
                std::process::exit(0);
            }
            "--help" | "-h" => {
                println!("{}", include_str!("../HELP.txt"));
                std::process::exit(0);
            }
            other => eprintln!("unknown argument: {other} (try --help)"),
        }
        i += 1;
    }
    a.fullscreen = if full { true } else if windowed { false } else { !a.debug };
    a
}

struct App {
    args: Args,
    game: Game,
    static_mesh: Vec<vk::Vertex>,
    window: Option<Window>,
    renderer: Option<vk::Renderer>,
    wii: wiimote::WiimoteManager,
    audio: audio::Audio,
    last: Instant,
    mouse: Vec2,
    mouse_in: bool,
    mouse_down: bool,
    mouse_clicked: bool,
    keys_pressed: HashSet<KeyCode>,
    keys_down: HashSet<KeyCode>,
    mouse_slot: Option<usize>,
    sim_time: f32,
    shots_done: usize,
    fps_acc: (f32, u32),
    dyn_buf: Vec<vk::Vertex>,
    mist_buf: Vec<vk::Vertex>,
}

impl App {
    fn screen(&self) -> Vec2 {
        match &self.renderer {
            Some(r) => Vec2::new(r.extent.width.max(1) as f32, r.extent.height.max(1) as f32),
            None => Vec2::new(self.args.size.0 as f32, self.args.size.1 as f32),
        }
    }

    fn gather_inputs(&mut self) -> Vec<SourceInput> {
        let screen = self.screen();
        let mut out = vec![];
        let remotes = self.wii.poll();
        let mut dbg = vec![format!("WII REMOTES {}", remotes.len())];
        for r in &remotes {
            use wiimote::*;
            let fire_bits = B_B | B_A;
            out.push(SourceInput {
                id: SourceId::Wii(r.id),
                aim: r.aim.map(|(x, y)| Vec2::new(x * screen.x, y * screen.y)),
                fire: r.buttons & fire_bits != 0,
                fire_pressed: r.pressed & fire_bits != 0,
                join: r.pressed & (B_A | B_B) != 0,
                start: r.pressed & B_PLUS != 0,
                pause: r.pressed & B_HOME != 0,
                quit_to_title: r.pressed & B_MINUS != 0,
            });
            dbg.push(format!("  #{} {}HZ IR {:?}", r.id, r.report_rate, r.raw_dots));
        }
        self.game.debug_lines = dbg;

        let kp = |k: KeyCode, s: &Self| s.keys_pressed.contains(&k);
        let start = kp(KeyCode::Enter, self) || kp(KeyCode::NumpadEnter, self) || kp(KeyCode::Space, self);
        let pause = kp(KeyCode::KeyP, self);
        let quit = kp(KeyCode::Backspace, self);
        if self.args.debug {
            for (k, slot) in [(KeyCode::Digit1, 0), (KeyCode::Digit2, 1), (KeyCode::Digit3, 2), (KeyCode::Digit4, 3)] {
                if kp(k, self) {
                    self.mouse_slot = Some(slot);
                    self.game.players[slot].joined = true;
                }
            }
            out.push(SourceInput {
                id: SourceId::Mouse,
                aim: if self.mouse_in { Some(self.mouse) } else { None },
                fire: self.mouse_down,
                fire_pressed: self.mouse_clicked,
                join: self.mouse_clicked,
                start,
                pause,
                quit_to_title: quit,
            });
        } else {
            // Keyboard can still start / pause the game without a mouse player.
            out.push(SourceInput {
                id: SourceId::Bot(99),
                aim: None,
                fire: false,
                fire_pressed: false,
                join: false,
                start,
                pause,
                quit_to_title: quit,
            });
        }
        self.mouse_clicked = false;
        out
    }

    fn frame(&mut self, el: &ActiveEventLoop) {
        let now = Instant::now();
        let real_dt = (now - self.last).as_secs_f32().min(0.1);
        self.last = now;
        self.fps_acc.0 += real_dt;
        self.fps_acc.1 += 1;
        if self.fps_acc.0 > 0.5 {
            self.game.fps = self.fps_acc.1 as f32 / self.fps_acc.0;
            self.fps_acc = (0.0, 0);
        }

        // Debug keys.
        if self.keys_pressed.contains(&KeyCode::F3) {
            self.game.show_debug = !self.game.show_debug;
        }
        if self.args.debug && self.keys_pressed.contains(&KeyCode::KeyB) {
            self.game.skip_to_boss();
        }
        if self.keys_pressed.contains(&KeyCode::F11) {
            if let Some(w) = &self.window {
                w.set_fullscreen(if w.fullscreen().is_some() { None } else { Some(Fullscreen::Borderless(None)) });
            }
        }
        let mut scale = self.args.timescale;
        if self.args.debug && self.keys_down.contains(&KeyCode::Tab) {
            scale *= 4.0;
        }

        let inputs = self.gather_inputs();
        self.keys_pressed.clear();
        let screen = self.screen();
        let mouse_slot = if self.args.debug { Some(self.mouse_slot.unwrap_or(0)) } else { None };
        // Fixed-size sub steps keep the simulation stable at high time scales.
        let mut remaining = real_dt * scale;
        if self.args.shots.len() > self.shots_done || self.args.exit_after.is_some() {
            // Deterministic stepping for capture runs.
            remaining = scale / 60.0;
        }
        self.sim_time += step_game(&mut self.game, &self.audio, &inputs, remaining, screen, mouse_slot);
        for (src, ms) in self.game.rumble_requests.drain(..) {
            if let SourceId::Wii(id) = src {
                self.wii.rumble(id, ms);
            }
        }
        for (src, slot) in self.game.led_requests.drain(..) {
            if let SourceId::Wii(id) = src {
                self.wii.set_led(id, slot);
            }
        }

        // Build dynamic geometry and HUD.
        self.dyn_buf.clear();
        self.game.draw_world(&mut self.dyn_buf);
        self.mist_buf.clear();
        self.game.draw_mist(&mut self.mist_buf);
        let mut ui = hud::Ui::new(screen.x, screen.y);
        self.game.draw_ui(&mut ui);

        let f = self.game.frame();
        let world_pc = vk::PushConstants {
            viewproj: f.viewproj.to_cols_array_2d(),
            cam: [f.cam.x, f.cam.y, f.cam.z, self.sim_time],
            fog: [f.fog[0], f.fog[1], f.fog[2], 1.0],
            params: [f.fog_range.0, f.fog_range.1, f.lamp, 20.0],
        };
        let ortho = Mat4::orthographic_rh(0.0, screen.x, screen.y, 0.0, -1.0, 1.0);
        let ui_pc = vk::PushConstants { viewproj: ortho.to_cols_array_2d(), ..world_pc };

        let capture = self.shots_done < self.args.shots.len() && self.sim_time >= self.args.shots[self.shots_done];
        let win_size = self.window.as_ref().map(|w| w.inner_size()).map(|s| (s.width, s.height)).unwrap_or(self.args.size);
        if let Some(r) = self.renderer.as_mut() {
            if let Some(cap) = r.draw(&world_pc, &self.dyn_buf, &self.mist_buf, &ui_pc, &ui.v, f.fog, capture, win_size) {
                let path = format!("{}/shot_{:06.1}.ppm", self.args.shot_dir, self.args.shots[self.shots_done]);
                save_ppm(&path, &cap);
                eprintln!(
                    "saved {path} (state {:?}, ride {:.1}s, scores {:?})",
                    self.game.state,
                    self.game.ride_t,
                    self.game.players.iter().filter(|p| p.joined).map(|p| p.score).collect::<Vec<_>>()
                );
                self.shots_done += 1;
            }
        }
        if let Some(t) = self.args.exit_after {
            if self.sim_time >= t {
                eprintln!(
                    "exit after {:.1}s sim time: state {:?}, ride {:.1}s, spawned/skipped {:?}, scores {:?}",
                    self.sim_time,
                    self.game.state,
                    self.game.ride_t,
                    (self.game.spawn_stats, "pumpkin/ghost/bat/book/skel/spider/candy/portrait", self.game.spawn_kinds),
                    self.game.players.iter().filter(|p| p.joined).map(|p| p.score).collect::<Vec<_>>()
                );
                el.exit();
            }
        }
        let _ = State::Title;
    }
}

/// Advances the game by `remaining` seconds in fixed-size sub steps (which keep the simulation
/// stable at high time scales), plays the sounds it asks for and returns the time stepped.
/// Button presses only count in the first sub step.
fn step_game(
    game: &mut Game,
    audio: &audio::Audio,
    inputs: &[SourceInput],
    mut remaining: f32,
    screen: Vec2,
    mouse_slot: Option<usize>,
) -> f32 {
    let mut stepped = 0.0;
    let mut first = true;
    while remaining > 1e-5 {
        let step = remaining.min(1.0 / 60.0);
        remaining -= step;
        let inp: Vec<SourceInput> = if first {
            inputs.to_vec()
        } else {
            inputs
                .iter()
                .map(|i| SourceInput { fire_pressed: false, join: false, start: false, pause: false, quit_to_title: false, ..i.clone() })
                .collect()
        };
        first = false;
        game.update(step, &inp, screen, mouse_slot);
        for e in game.sounds.drain(..) {
            audio.play(e);
        }
        stepped += step;
    }
    audio.music(game.music());
    audio.cart(game.cart_noise());
    stepped
}

/// `$XDG_DATA_HOME/haunted-hollow/highscores.txt` (default `~/.local/share/...`).
fn high_score_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .filter(|d| !d.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share")))?;
    Some(base.join("haunted-hollow").join("highscores.txt"))
}

fn save_ppm(path: &str, cap: &vk::Capture) {
    let mut data = format!("P6\n{} {}\n255\n", cap.width, cap.height).into_bytes();
    for px in cap.rgba.chunks_exact(4) {
        data.extend_from_slice(&px[..3]);
    }
    if let Err(e) = std::fs::write(path, data) {
        eprintln!("could not save {path}: {e}");
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let mut attrs = Window::default_attributes()
            .with_title("Haunted Hollow")
            .with_inner_size(PhysicalSize::new(self.args.size.0, self.args.size.1));
        if self.args.fullscreen {
            attrs = attrs.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        let window = match el.create_window(attrs) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("could not create window: {e}");
                el.exit();
                return;
            }
        };
        window.set_cursor_visible(false);
        let size = window.inner_size();
        let atlas = atlas::build();
        match vk::Renderer::new(&window, size.width.max(1), size.height.max(1), &atlas, atlas::SIZE) {
            Ok(mut r) => {
                eprintln!("Vulkan device: {}", r.device_name);
                if let Err(e) = r.set_static_mesh(&self.static_mesh) {
                    eprintln!("static mesh upload failed: {e}");
                }
                self.renderer = Some(r);
            }
            Err(e) => {
                eprintln!("Vulkan init failed: {e}");
                el.exit();
                return;
            }
        }
        self.window = Some(window);
        self.last = Instant::now();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(_) => {
                if let Some(r) = self.renderer.as_mut() {
                    r.needs_resize = true;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let ws = self.window.as_ref().map(|w| w.inner_size()).unwrap_or(PhysicalSize::new(1, 1));
                let s = self.screen();
                self.mouse = Vec2::new(
                    position.x as f32 * s.x / ws.width.max(1) as f32,
                    position.y as f32 * s.y / ws.height.max(1) as f32,
                );
                self.mouse_in = true;
            }
            WindowEvent::CursorLeft { .. } => self.mouse_in = false,
            WindowEvent::MouseInput { state, button, .. } => match button {
                MouseButton::Left => {
                    self.mouse_down = state == ElementState::Pressed;
                    if self.mouse_down {
                        self.mouse_clicked = true;
                    }
                }
                MouseButton::Right if state == ElementState::Pressed => {
                    self.keys_pressed.insert(KeyCode::Enter);
                }
                _ => {}
            },
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if event.state == ElementState::Pressed {
                        if !event.repeat {
                            self.keys_pressed.insert(code);
                        }
                        self.keys_down.insert(code);
                        if code == KeyCode::Escape {
                            el.exit();
                        }
                    } else {
                        self.keys_down.remove(&code);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.frame(el);
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

fn main() {
    let args = parse_args();
    if let Some(dir) = &args.dump_audio {
        let bank = audio::Bank::build();
        match bank.dump(dir) {
            Ok(()) => eprintln!("wrote music and sound effects to {dir}"),
            Err(e) => eprintln!("could not write audio: {e}"),
        }
        return;
    }
    if let Some(dir) = &args.export_ps1 {
        if let Err(e) = ps1export::export(dir) {
            eprintln!("could not export the PS1 data: {e}");
            std::process::exit(1);
        }
        return;
    }
    let t0 = Instant::now();
    let track = track::Track::new();
    let world = world::generate(&track);
    eprintln!("world generated in {:.2}s (track {:.0} m)", t0.elapsed().as_secs_f32(), track.total);
    let mut game = Game::new(track, world, args.debug, args.bots);
    if let Some(path) = high_score_path() {
        game.set_high_score_file(path);
    }
    let t1 = Instant::now();
    let static_mesh = game.static_mesh();
    eprintln!("world mesh: {} quads in {:.2}s", static_mesh.len() / 4, t1.elapsed().as_secs_f32());
    if args.boss {
        game.skip_to_boss();
    }
    if args.debug {
        eprintln!("DEBUG MODE: mouse = player 1 (keys 1-4 pick player), left click shoots, Enter/right click starts,");
        eprintln!("            B = skip to boss, hold Tab = fast forward, P = pause, F3 = debug info, Esc = quit");
    }
    let audio = if args.mute { audio::Audio::silent() } else { audio::Audio::start() };
    if args.vr {
        // Single player with the headset's controllers: no window and no Wii Remotes.
        if let Err(e) = xr::run(game, &static_mesh, &audio, args.timescale) {
            eprintln!("VR mode failed: {e}");
            std::process::exit(1);
        }
        return;
    }
    let wii = wiimote::WiimoteManager::start(args.ir.clone());

    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        args,
        game,
        static_mesh,
        window: None,
        renderer: None,
        wii,
        audio,
        last: Instant::now(),
        mouse: Vec2::ZERO,
        mouse_in: false,
        mouse_down: false,
        mouse_clicked: false,
        keys_pressed: HashSet::new(),
        keys_down: HashSet::new(),
        mouse_slot: None,
        sim_time: 0.0,
        shots_done: 0,
        fps_acc: (0.0, 0),
        dyn_buf: Vec::with_capacity(200_000),
        mist_buf: Vec::with_capacity(4096),
    };
    event_loop.run_app(&mut app).expect("run");
    // Drop the renderer before the window.
    app.renderer = None;
}
