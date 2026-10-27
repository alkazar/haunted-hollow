//! Gameplay: the ride, spooky targets, the Pumpkin King, scoring and all screens.
//! No one can lose: targets just pop into candy and every player earns a score.

use crate::atlas::*;
use crate::hud::{Align, Ui};
use crate::mesh::{push_box, Models};
use crate::rng::Rng;
use crate::track::{Track, RIDE_SECONDS};
use crate::vk::Vertex;
use crate::world::{zone_at, World, Zone};
use glam::{Mat4, Vec2, Vec3, Vec4Swizzles};

pub const PLAYER_COLORS: [[f32; 3]; 4] = [[1.0, 0.28, 0.28], [0.3, 0.6, 1.0], [0.35, 0.95, 0.4], [1.0, 0.86, 0.2]];
const PLAYER_NAMES: [&str; 4] = ["RED", "BLUE", "GREEN", "YELLOW"];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum SourceId {
    Wii(u32),
    Mouse,
    Bot(u32),
}

/// One input source's state this frame (aim in pixels).
#[derive(Clone, Debug)]
pub struct SourceInput {
    pub id: SourceId,
    pub aim: Option<Vec2>,
    pub fire: bool,
    pub fire_pressed: bool,
    pub join: bool,
    pub start: bool,
    pub pause: bool,
    pub quit_to_title: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Pumpkin,
    Ghost,
    Bat,
    Book,
    Skeleton,
    Spider,
    Candy,
}

impl Kind {
    fn points(self) -> u32 {
        match self {
            Kind::Pumpkin | Kind::Ghost => 100,
            Kind::Bat | Kind::Book | Kind::Spider => 150,
            Kind::Skeleton => 200,
            Kind::Candy => 500,
        }
    }
    fn radius(self) -> f32 {
        match self {
            Kind::Ghost => 0.85,
            Kind::Skeleton => 0.8,
            Kind::Pumpkin | Kind::Spider => 0.7,
            Kind::Bat | Kind::Book | Kind::Candy => 0.6,
        }
    }
    fn index(self) -> usize {
        self as usize
    }
}

const AWARDS: [&str; 9] = [
    "PUMPKIN POPPER",
    "GHOST CATCHER",
    "BAT BOPPER",
    "BOOKWORM",
    "BONE BONKER",
    "SPIDER SPOTTER",
    "CANDY CHAMP",
    "BOSS BUSTER",
    "SPOOKY STAR",
];

struct Target {
    kind: Kind,
    home: Vec3,
    a: Vec3,
    b: Vec3,
    age: f32,
    life: f32,
    leave: f32,
    phase: f32,
    variant: usize,
    pos: Vec3,
    scale: f32,
    dead: bool,
}

struct Particle {
    pos: Vec3,
    vel: Vec3,
    color: [f32; 3],
    size: f32,
    age: f32,
    life: f32,
    glow: f32,
    gravity: f32,
}

struct Popup {
    pos: Vec3,
    text: String,
    color: [f32; 3],
    age: f32,
    big: bool,
}

#[derive(Clone)]
pub struct Player {
    pub joined: bool,
    pub source: Option<SourceId>,
    pub score: u32,
    shown: f32,
    pub hits: u32,
    pub shots: u32,
    aim: Vec2,
    aim_target: Vec2,
    aim_visible: bool,
    cooldown: f32,
    held: f32,
    kick: f32,
    counts: [u32; 8],
    bot_target: Option<Vec2>,
    bot_think: f32,
}

impl Player {
    fn new() -> Player {
        Player {
            joined: false,
            source: None,
            score: 0,
            shown: 0.0,
            hits: 0,
            shots: 0,
            aim: Vec2::ZERO,
            aim_target: Vec2::ZERO,
            aim_visible: false,
            cooldown: 0.0,
            held: 0.0,
            kick: 0.0,
            counts: [0; 8],
            bot_target: None,
            bot_think: 0.0,
        }
    }
    fn reset_score(&mut self) {
        self.score = 0;
        self.shown = 0.0;
        self.hits = 0;
        self.shots = 0;
        self.counts = [0; 8];
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum State {
    Title,
    Ride,
    Boss,
    Victory,
    Outro,
    Results,
}

struct Boss {
    hp: f32,
    max_hp: f32,
    pos: Vec3,
    t: f32,
    flash: f32,
    wobble: f32,
    summons_done: u32,
    taunt: f32,
    taunt_text: &'static str,
    defeated: bool,
}

struct Spawn {
    t: f32,
    kind: Kind,
    seed: u64,
    side: f32,
}

/// What the renderer needs for one frame.
pub struct FrameOut {
    pub viewproj: Mat4,
    pub cam: Vec3,
    pub fog: [f32; 3],
    pub fog_range: (f32, f32),
    pub lamp: f32,
}

pub struct Game {
    pub state: State,
    pub players: [Player; 4],
    state_t: f32,
    pub ride_t: f32,
    total_t: f32,
    track: Track,
    world: World,
    models: Models,
    targets: Vec<Target>,
    particles: Vec<Particle>,
    popups: Vec<Popup>,
    spawns: Vec<Spawn>,
    next_spawn: usize,
    boss: Option<Boss>,
    rng: Rng,
    zone: Zone,
    banner: (String, f32),
    fog: [f32; 3],
    fog_range: (f32, f32),
    lamp: f32,
    viewproj: Mat4,
    cam: Vec3,
    pub paused: bool,
    pub debug: bool,
    pub show_debug: bool,
    title_join_t: Option<f32>,
    pub rumble_requests: Vec<(SourceId, u32)>,
    pub led_requests: Vec<(SourceId, usize)>,
    pub bots: u32,
    screen: Vec2,
    pub fps: f32,
    pub debug_lines: Vec<String>,
    shake: f32,
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn zone_name(z: Zone) -> &'static str {
    match z {
        Zone::Graveyard => "THE GRAVEYARD",
        Zone::Foyer => "THE GRAND FOYER",
        Zone::Library => "THE HAUNTED LIBRARY",
        Zone::Dining => "THE DINING HALL",
        Zone::Drop => "HOLD ON TIGHT!",
        Zone::Crypt => "THE CREEPY CRYPT",
        Zone::Lair => "THE PUMPKIN KING'S LAIR",
    }
}

fn zone_fog(z: Zone) -> ([f32; 3], (f32, f32), f32) {
    match z {
        Zone::Graveyard => ([0.07, 0.06, 0.15], (30.0, 130.0), 0.5),
        Zone::Foyer | Zone::Library | Zone::Dining => ([0.04, 0.03, 0.06], (18.0, 64.0), 0.95),
        Zone::Drop => ([0.02, 0.04, 0.03], (10.0, 44.0), 0.9),
        Zone::Crypt => ([0.03, 0.06, 0.05], (14.0, 50.0), 0.9),
        Zone::Lair => ([0.08, 0.03, 0.12], (24.0, 80.0), 0.45),
    }
}

impl Game {
    pub fn new(track: Track, world: World, debug: bool, bots: u32) -> Game {
        let mut g = Game {
            state: State::Title,
            players: std::array::from_fn(|_| Player::new()),
            state_t: 0.0,
            ride_t: 0.0,
            total_t: 0.0,
            track,
            world,
            models: Models::new(),
            targets: vec![],
            particles: vec![],
            popups: vec![],
            spawns: vec![],
            next_spawn: 0,
            boss: None,
            rng: Rng::new(7),
            zone: Zone::Graveyard,
            banner: (String::new(), 99.0),
            fog: zone_fog(Zone::Graveyard).0,
            fog_range: zone_fog(Zone::Graveyard).1,
            lamp: 0.35,
            viewproj: Mat4::IDENTITY,
            cam: Vec3::ZERO,
            paused: false,
            debug,
            show_debug: debug,
            title_join_t: None,
            rumble_requests: vec![],
            led_requests: vec![],
            bots,
            screen: Vec2::new(1280.0, 720.0),
            fps: 0.0,
            debug_lines: vec![],
            shake: 0.0,
        };
        for k in 0..bots.min(4) {
            g.players[k as usize].joined = true;
            g.players[k as usize].source = Some(SourceId::Bot(k));
        }
        g
    }

    fn joined_count(&self) -> usize {
        self.players.iter().filter(|p| p.joined).count()
    }

    pub fn start_ride(&mut self) {
        self.state = State::Ride;
        self.state_t = 0.0;
        self.ride_t = 0.0;
        self.targets.clear();
        self.particles.clear();
        self.popups.clear();
        self.boss = None;
        self.zone = Zone::Graveyard;
        self.banner = (zone_name(Zone::Graveyard).into(), -2.5);
        for p in self.players.iter_mut() {
            p.reset_score();
        }
        self.build_schedule();
    }

    /// Debug helper: jump straight to the boss.
    pub fn skip_to_boss(&mut self) {
        if self.state == State::Title {
            self.start_ride();
        }
        if self.state == State::Ride {
            self.ride_t = RIDE_SECONDS - 0.5;
            self.next_spawn = self.spawns.len();
            self.targets.clear();
        }
    }

    fn build_schedule(&mut self) {
        let n = self.joined_count().max(1) as f32;
        let tr = &self.track;
        let t_foyer = tr.time_at_z(39.5);
        let t_lib = tr.time_at_z(70.5);
        let t_din = tr.time_at_z(100.5);
        let t_drop = tr.time_at_z(127.5);
        let t_crypt = tr.time_at_z(140.0);
        let t_lair = tr.time_at_z(176.5);
        use Kind::*;
        let sections: Vec<(f32, f32, Vec<(Kind, u32)>, f32)> = vec![
            (3.4, t_foyer - 1.2, vec![(Pumpkin, 5), (Bat, 3), (Ghost, 1)], 1.0),
            (t_foyer + 0.3, t_lib - 0.6, vec![(Ghost, 6), (Bat, 2)], 1.0),
            (t_lib + 0.3, t_din - 0.6, vec![(Book, 6), (Ghost, 2), (Bat, 1)], 1.0),
            (t_din + 0.3, t_drop - 0.3, vec![(Skeleton, 5), (Ghost, 2), (Bat, 1)], 1.0),
            (t_drop + 0.2, t_crypt - 0.3, vec![(Bat, 1)], 0.6),
            (t_crypt + 0.2, t_lair - 0.4, vec![(Spider, 5), (Ghost, 3), (Bat, 2)], 1.0),
            (t_lair, RIDE_SECONDS - 3.0, vec![(Pumpkin, 3), (Bat, 2)], 1.0),
        ];
        let mut rng = Rng::new(4242);
        let mut spawns = vec![];
        let interval = 1.25 / (1.0 + 0.32 * (n - 1.0));
        for (t0, t1, pool, density) in sections {
            if t1 <= t0 {
                continue;
            }
            let total_w: u32 = pool.iter().map(|p| p.1).sum();
            let mut t = t0;
            while t < t1 {
                let mut pick = rng.int(0, total_w as i32 - 1) as u32;
                let mut kind = pool[0].0;
                for (k, w) in &pool {
                    if pick < *w {
                        kind = *k;
                        break;
                    }
                    pick -= w;
                }
                let side = rng.sign();
                spawns.push(Spawn { t, kind, seed: rng.next_u32() as u64, side });
                if rng.chance(0.22 + 0.08 * (n - 1.0)) {
                    spawns.push(Spawn { t: t + 0.15, kind, seed: rng.next_u32() as u64, side: -side });
                }
                t += interval / density * rng.range(0.7, 1.3);
            }
            // One golden candy bonus per section.
            if t1 - t0 > 4.0 {
                spawns.push(Spawn { t: rng.range(t0 + 1.0, t1 - 1.0), kind: Candy, seed: rng.next_u32() as u64, side: rng.sign() });
            }
        }
        spawns.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
        self.spawns = spawns;
        self.next_spawn = 0;
    }

    fn floor_below(&self, p: Vec3) -> f32 {
        let mut y = (p.y + 2.0).floor();
        for _ in 0..20 {
            if self.world.solid_at(Vec3::new(p.x, y - 0.5, p.z)) {
                return y;
            }
            y -= 1.0;
        }
        p.y - 1.0
    }

    fn ceiling_above(&self, p: Vec3) -> Option<f32> {
        let mut y = p.y.floor() + 1.0;
        for _ in 0..14 {
            if self.world.solid_at(Vec3::new(p.x, y + 0.5, p.z)) {
                return Some(y);
            }
            y += 1.0;
        }
        None
    }

    fn spawn_target(&mut self, kind: Kind, seed: u64, side_sign: f32, near: Option<Vec3>) {
        let mut rng = Rng::new(seed);
        let mut lead = rng.range(2.3, 3.3);
        // Flyers cross the track further ahead so they never zoom through the cart.
        match kind {
            Kind::Bat => lead += 2.2,
            Kind::Candy => lead += 1.0,
            _ => {}
        }
        let d = if let Some(_) = near {
            self.track.stop_dist
        } else if self.ride_t + lead > RIDE_SECONDS {
            self.track.stop_dist + rng.range(6.0, 12.0)
        } else {
            self.track.dist_at_time(self.ride_t + lead)
        };
        let f = self.track.frame_at(d);
        let right = Vec3::new(f.right.x, 0.0, f.right.z).normalize_or_zero();
        let fwd = Vec3::new(f.fwd.x, 0.0, f.fwd.z).normalize_or_zero();
        let mut side = side_sign * rng.range(2.1, 4.8);
        let fo = rng.range(-1.0, 2.5);
        let mut base = f.pos + right * side + fwd * fo;
        if let Some(c) = near {
            base = c + Vec3::new(rng.range(-7.0, 7.0), rng.range(-3.0, 2.0), rng.range(-6.0, -2.0));
        }
        // Keep out of walls: slide toward the track until we find air.
        for _ in 0..10 {
            let test = base + Vec3::Y * 1.5;
            if !self.world.solid_at(test) && !self.world.solid_at(base + Vec3::Y * 0.6) {
                break;
            }
            side *= 0.75;
            base = f.pos + right * side + fwd * fo;
        }
        let floor = self.floor_below(base + Vec3::Y * 1.0);
        let phase = rng.range(0.0, 6.28);
        let variant = rng.int(0, 2) as usize;
        let mut t = Target {
            kind,
            home: base,
            a: base,
            b: base,
            age: 0.0,
            life: rng.range(4.5, 6.0),
            leave: 0.0,
            phase,
            variant,
            pos: base,
            scale: 0.0,
            dead: false,
        };
        match kind {
            Kind::Ghost => {
                t.home.y = floor + rng.range(1.6, 3.6);
                if near.is_some() {
                    t.home.y = base.y;
                }
            }
            Kind::Pumpkin => {
                t.home.y = floor + 0.56;
                if near.is_some() {
                    t.home.y = base.y;
                }
            }
            Kind::Skeleton => {
                t.home.y = floor + 0.95;
                t.life = rng.range(3.5, 4.5);
            }
            Kind::Book => {
                t.home.y = floor + rng.range(2.0, 4.0);
            }
            Kind::Spider => {
                let ceil = self.ceiling_above(base + Vec3::Y).unwrap_or(base.y + 7.0);
                t.a = Vec3::new(base.x, ceil, base.z);
                t.home.y = (floor + rng.range(1.4, 3.0)).min(ceil - 1.0);
            }
            Kind::Bat | Kind::Candy => {
                let h = if kind == Kind::Bat { rng.range(2.0, 4.2) } else { rng.range(1.6, 3.0) };
                let y = (floor + h).max(f.pos.y + 1.2);
                let span = if kind == Kind::Bat { 7.0 } else { 9.0 };
                let s = side_sign;
                t.a = f.pos + right * (s * span) + fwd * fo + Vec3::Y * (y - f.pos.y);
                t.b = f.pos - right * (s * span) + fwd * (fo + rng.range(-2.0, 2.0)) + Vec3::Y * (y - f.pos.y + rng.range(-0.5, 1.0));
                if let Some(c) = near {
                    t.a = c + Vec3::new(s * 9.0, rng.range(-2.0, 3.0), rng.range(-6.0, -3.0));
                    t.b = c + Vec3::new(-s * 9.0, rng.range(-2.0, 3.0), rng.range(-6.0, -3.0));
                }
                // Clamp the flight path to the inside of the room.
                for _ in 0..8 {
                    if !self.world.solid_at(t.a) {
                        break;
                    }
                    t.a = t.a.lerp(f.pos + Vec3::Y * (y - f.pos.y), 0.25);
                }
                for _ in 0..8 {
                    if !self.world.solid_at(t.b) {
                        break;
                    }
                    t.b = t.b.lerp(f.pos + Vec3::Y * (y - f.pos.y), 0.25);
                }
                t.life = if kind == Kind::Bat { rng.range(3.6, 4.6) } else { 2.8 };
                t.pos = t.a;
            }
        }
        self.targets.push(t);
    }

    // ================================================================= update

    pub fn update(&mut self, dt: f32, inputs: &[SourceInput], screen: Vec2, mouse_slot: Option<usize>) {
        self.screen = screen;
        self.total_t += dt;
        self.rumble_requests.clear();
        self.led_requests.clear();
        self.handle_sources(inputs, mouse_slot);
        let any_pause = inputs.iter().any(|i| i.pause);
        if any_pause && matches!(self.state, State::Ride | State::Boss) {
            self.paused = !self.paused;
        }
        if self.paused {
            if inputs.iter().any(|i| i.quit_to_title) {
                self.paused = false;
                self.state = State::Title;
                self.state_t = 0.0;
                self.title_join_t = None;
            }
            return;
        }
        self.state_t += dt;
        self.update_bots(dt);

        match self.state {
            State::Title => {
                let start = inputs.iter().any(|i| i.start);
                if self.joined_count() > 0 && self.title_join_t.is_none() {
                    self.title_join_t = Some(0.0);
                }
                if let Some(t) = self.title_join_t.as_mut() {
                    *t += dt;
                }
                let auto = self.title_join_t.map_or(false, |t| t > 15.0) || (self.bots > 0 && self.state_t > 1.5);
                if (start && self.joined_count() > 0) || auto {
                    self.title_join_t = None;
                    self.start_ride();
                }
            }
            State::Ride => {
                self.ride_t += dt;
                while self.next_spawn < self.spawns.len() && self.spawns[self.next_spawn].t <= self.ride_t {
                    let s = &self.spawns[self.next_spawn];
                    let (k, seed, side) = (s.kind, s.seed, s.side);
                    self.spawn_target(k, seed, side, None);
                    self.next_spawn += 1;
                }
                if self.ride_t >= RIDE_SECONDS {
                    self.ride_t = RIDE_SECONDS;
                    self.state = State::Boss;
                    self.state_t = 0.0;
                    let n = self.joined_count().max(1) as f32;
                    let hp = 40.0 + 22.0 * n;
                    self.boss = Some(Boss {
                        hp,
                        max_hp: hp,
                        pos: Vec3::new(0.0, -26.0, 200.0),
                        t: 0.0,
                        flash: 0.0,
                        wobble: 0.0,
                        summons_done: 0,
                        taunt: 2.5,
                        taunt_text: "MWAHAHA!",
                        defeated: false,
                    });
                    self.banner = ("THE PUMPKIN KING!".into(), 0.0);
                    self.shake = 0.6;
                }
            }
            State::Boss => self.update_boss(dt),
            State::Victory => {
                if self.state_t > 3.8 {
                    self.state = State::Outro;
                    self.state_t = 0.0;
                }
            }
            State::Outro => {
                if self.state_t > 4.5 {
                    self.state = State::Results;
                    self.state_t = 0.0;
                }
            }
            State::Results => {
                let go = inputs.iter().any(|i| i.join || i.start || i.fire_pressed);
                if (go && self.state_t > 4.0) || self.state_t > 45.0 || (self.bots > 0 && self.state_t > 6.0) {
                    self.state = State::Title;
                    self.state_t = 0.0;
                    self.title_join_t = None;
                }
            }
        }

        // Targets.
        for t in self.targets.iter_mut() {
            t.age += dt;
            if t.age > t.life {
                t.leave += dt;
            }
            let appear = smooth(t.age / 0.35) * (1.0 - smooth(t.leave / 0.4));
            t.scale = appear;
            let a = t.age;
            let ph = t.phase;
            t.pos = match t.kind {
                Kind::Ghost => t.home + Vec3::new((a * 1.3 + ph).sin() * 0.6, (a * 2.1 + ph).sin() * 0.35, 0.0),
                Kind::Pumpkin => t.home - Vec3::Y * (1.0 - appear) * 0.9 + Vec3::Y * ((a * 3.0 + ph).sin() * 0.06),
                Kind::Skeleton => t.home - Vec3::Y * (1.0 - appear) * 1.7,
                Kind::Spider => {
                    let top = t.a.y - 0.4;
                    Vec3::new(t.home.x, top + (t.home.y - top) * appear + (a * 2.0).sin() * 0.12, t.home.z)
                }
                Kind::Book => t.home + Vec3::new((a * 1.6 + ph).cos() * 1.1, (a * 3.0).sin() * 0.3, (a * 1.6 + ph).sin() * 1.1),
                Kind::Bat => {
                    let u = (a / t.life).min(1.0);
                    t.a.lerp(t.b, u) + Vec3::Y * (a * 5.0 + ph).sin() * 0.45
                }
                Kind::Candy => {
                    let u = (a / t.life).min(1.0);
                    t.a.lerp(t.b, u) + Vec3::Y * (a * 6.0).sin() * 0.3
                }
            };
            if matches!(t.kind, Kind::Bat | Kind::Candy) {
                t.scale = smooth(t.age / 0.25);
                if t.age >= t.life {
                    t.dead = true;
                }
            } else if t.leave > 0.4 {
                t.dead = true;
            }
            // Gone behind the cart.
            let to = t.pos - self.cam;
            if self.state == State::Ride && to.length() > 3.0 {
                let fwd = self.track.frame_at(self.track.dist_at_time(self.ride_t)).fwd;
                if to.dot(fwd) < -3.0 {
                    t.dead = true;
                }
            }
        }
        self.targets.retain(|t| !t.dead);

        // Particles and popups.
        for p in self.particles.iter_mut() {
            p.age += dt;
            p.vel.y -= p.gravity * dt;
            p.vel *= 1.0 - 0.6 * dt;
            p.pos += p.vel * dt;
        }
        self.particles.retain(|p| p.age < p.life);
        for p in self.popups.iter_mut() {
            p.age += dt;
            p.pos.y += dt * 0.9;
        }
        self.popups.retain(|p| p.age < 1.3);
        self.banner.1 += dt;
        self.shake = (self.shake - dt).max(0.0);
        for p in self.players.iter_mut() {
            p.shown += (p.score as f32 - p.shown) * (1.0 - (-dt * 6.0).exp());
            if (p.score as f32 - p.shown).abs() < 1.0 {
                p.shown = p.score as f32;
            }
            p.kick = (p.kick - dt * 5.0).max(0.0);
        }

        self.update_camera(dt);
        self.handle_shooting(inputs, dt);
    }

    fn handle_sources(&mut self, inputs: &[SourceInput], mouse_slot: Option<usize>) {
        // Mouse is bound to a chosen slot in debug mode.
        if let Some(slot) = mouse_slot {
            for (i, p) in self.players.iter_mut().enumerate() {
                if p.source == Some(SourceId::Mouse) && i != slot {
                    p.source = None;
                    p.aim_visible = false;
                }
            }
            let p = &mut self.players[slot];
            if p.source != Some(SourceId::Mouse) {
                p.source = Some(SourceId::Mouse);
            }
            if !p.joined && inputs.iter().any(|i| i.id == SourceId::Mouse && (i.join || i.fire_pressed)) {
                p.joined = true;
            }
        }
        for inp in inputs {
            let slot = self.players.iter().position(|p| p.source == Some(inp.id));
            match slot {
                Some(s) => {
                    let p = &mut self.players[s];
                    if !p.joined && (inp.join || inp.fire_pressed) {
                        p.joined = true;
                    }
                    if let Some(a) = inp.aim {
                        p.aim_target = a;
                        if !p.aim_visible {
                            p.aim = a;
                        }
                        p.aim_visible = true;
                    } else {
                        p.aim_visible = false;
                    }
                }
                None => {
                    if matches!(inp.id, SourceId::Wii(_)) && (inp.join || inp.fire_pressed) {
                        if let Some(free) = self.players.iter().position(|p| !p.joined && p.source.is_none())
                            .or_else(|| self.players.iter().position(|p| !p.joined))
                        {
                            let p = &mut self.players[free];
                            p.joined = true;
                            p.source = Some(inp.id);
                            p.reset_score();
                            self.led_requests.push((inp.id, free));
                            self.rumble_requests.push((inp.id, 200));
                        }
                    }
                }
            }
        }
        // Players whose remote vanished keep their score but lose their crosshair.
        for p in self.players.iter_mut() {
            if let Some(SourceId::Wii(id)) = p.source {
                if !inputs.iter().any(|i| i.id == SourceId::Wii(id)) {
                    p.aim_visible = false;
                }
            }
        }
    }

    fn update_bots(&mut self, dt: f32) {
        let screen = self.screen;
        let targets: Vec<Vec2> = self
            .targets
            .iter()
            .filter(|t| t.scale > 0.5)
            .filter_map(|t| self.project(t.pos))
            .map(|(p, _)| p)
            .collect();
        let boss_pt = self.boss.as_ref().filter(|b| !b.defeated && b.t > 2.0).and_then(|b| self.project(b.pos));
        let mut rng = self.rng.clone();
        for (i, p) in self.players.iter_mut().enumerate() {
            let Some(SourceId::Bot(_)) = p.source else { continue };
            p.aim_visible = true;
            p.bot_think -= dt;
            if p.bot_think <= 0.0 {
                p.bot_think = rng.range(0.25, 0.6);
                let mut cands = targets.clone();
                if let Some((b, _)) = boss_pt {
                    cands.push(b);
                }
                p.bot_target = if cands.is_empty() {
                    None
                } else {
                    let k = (i + rng.int(0, cands.len() as i32 - 1) as usize) % cands.len();
                    Some(cands[k] + Vec2::new(rng.range(-25.0, 25.0), rng.range(-25.0, 25.0)))
                };
            }
            p.aim_target = p.bot_target.unwrap_or(screen * Vec2::new(0.3 + 0.13 * i as f32, 0.5));
        }
        self.rng = rng;
    }

    fn update_camera(&mut self, dt: f32) {
        let _ = dt;
        let (d, speed) = match self.state {
            State::Title => (0.0, 0.0),
            State::Ride => {
                let d = self.track.dist_at_time(self.ride_t);
                (d, (self.track.dist_at_time(self.ride_t + 0.1) - d) * 10.0)
            }
            State::Boss | State::Victory => (self.track.stop_dist, 0.0),
            State::Outro => {
                let t = self.state_t;
                (self.track.stop_dist + 0.9 * t * t, 1.8 * t)
            }
            State::Results => (self.track.stop_dist + 20.0, 0.0),
        };
        let f = self.track.frame_at(d);
        let ahead = self.track.pos_at(d + 4.0);
        let mut dir = (ahead - f.pos).normalize_or_zero().lerp(f.fwd, 0.4).normalize_or_zero();
        if dir == Vec3::ZERO {
            dir = Vec3::Z;
        }
        let mut eye = f.pos + Vec3::Y * 1.65;
        if self.state == State::Title {
            let s = self.state_t * 0.25;
            dir = Vec3::new(s.sin() * 0.25, 0.08 + (s * 0.7).sin() * 0.03, 1.0).normalize();
            eye = f.pos + Vec3::Y * 1.9;
        }
        if let (Some(b), true) = (&self.boss, matches!(self.state, State::Boss | State::Victory)) {
            let to = (b.pos - eye).normalize_or_zero();
            dir = dir.lerp(to, 0.3).normalize();
        }
        let roll = (self.track.turn_rate(d) * speed * 0.22).clamp(-0.3, 0.3);
        let right = dir.cross(Vec3::Y).normalize_or_zero();
        let up0 = right.cross(dir).normalize_or_zero();
        let up = (up0 * roll.cos() + right * roll.sin()).normalize();
        if self.shake > 0.0 {
            let k = self.shake * 0.12;
            eye += Vec3::new((self.total_t * 53.0).sin() * k, (self.total_t * 61.0).cos() * k, 0.0);
        }
        // Rattle of the cart on the rails.
        if speed > 0.5 {
            eye.y += (self.total_t * 23.0).sin() * 0.012 * speed.min(8.0) / 8.0;
        }
        self.cam = eye;
        let view = Mat4::look_to_rh(eye, dir, up);
        let aspect = self.screen.x / self.screen.y.max(1.0);
        let proj = Mat4::perspective_rh(70f32.to_radians(), aspect, 0.05, 800.0);
        self.viewproj = proj * view;

        // Zone banners and fog transitions.
        let z = zone_at(f.pos);
        if z != self.zone && self.state == State::Ride {
            self.zone = z;
            self.banner = (zone_name(z).into(), 0.0);
        }
        if self.state == State::Title {
            self.zone = Zone::Graveyard;
        }
        let (fc, fr, lamp) = zone_fog(zone_at(f.pos));
        let k = 1.0 - (-dt * 1.5).exp();
        for i in 0..3 {
            self.fog[i] += (fc[i] - self.fog[i]) * k;
        }
        self.fog_range.0 += (fr.0 - self.fog_range.0) * k;
        self.fog_range.1 += (fr.1 - self.fog_range.1) * k;
        self.lamp += (lamp - self.lamp) * k;
    }

    /// World point -> (screen px, clip w).
    fn project(&self, p: Vec3) -> Option<(Vec2, f32)> {
        let c = self.viewproj * p.extend(1.0);
        if c.w <= 0.1 {
            return None;
        }
        let ndc = c.xyz() / c.w;
        if ndc.x.abs() > 1.2 || ndc.y.abs() > 1.2 {
            return None;
        }
        Some((Vec2::new((ndc.x * 0.5 + 0.5) * self.screen.x, (0.5 - ndc.y * 0.5) * self.screen.y), c.w))
    }

    fn pixel_radius(&self, r: f32, w: f32) -> f32 {
        let focal = self.screen.y * 0.5 / (35f32.to_radians()).tan();
        (r * 1.45 * focal / w).max(30.0 * self.screen.y / 720.0)
    }

    fn handle_shooting(&mut self, inputs: &[SourceInput], dt: f32) {
        let can_shoot = matches!(self.state, State::Ride | State::Boss);
        for i in 0..4 {
            let src = self.players[i].source;
            let (fire_pressed, fire_held) = match src {
                Some(SourceId::Bot(_)) => {
                    let p = &self.players[i];
                    let close = p.bot_target.map_or(false, |t| t.distance(p.aim) < 30.0);
                    (close && p.cooldown <= 0.0 && (self.total_t * 3.7 + i as f32).fract() < 0.5, false)
                }
                Some(id) => inputs
                    .iter()
                    .find(|x| x.id == id)
                    .map(|x| (x.fire_pressed, x.fire))
                    .unwrap_or((false, false)),
                None => (false, false),
            };
            let p = &mut self.players[i];
            let smooth_k = if matches!(src, Some(SourceId::Bot(_))) { 6.0 } else { 28.0 };
            p.aim += (p.aim_target - p.aim) * (1.0 - (-dt * smooth_k).exp());
            p.cooldown -= dt;
            if fire_held {
                p.held += dt;
            } else {
                p.held = 0.0;
            }
            let auto = fire_held && p.held > 0.35 && p.cooldown <= 0.0;
            if !(p.joined && p.aim_visible && can_shoot && (fire_pressed || auto) && p.cooldown <= 0.0) {
                continue;
            }
            p.cooldown = if auto { 0.28 } else { 0.11 };
            p.shots += 1;
            p.kick = 1.0;
            let aim = p.aim;
            self.shoot(i, aim);
        }
    }

    fn shoot(&mut self, pi: usize, aim: Vec2) {
        let color = PLAYER_COLORS[pi];
        // Find the nearest target under the crosshair.
        let mut best: Option<(usize, f32)> = None;
        for (ti, t) in self.targets.iter().enumerate() {
            if t.scale < 0.3 {
                continue;
            }
            if let Some((sp, w)) = self.project(t.pos) {
                let r = self.pixel_radius(t.kind.radius(), w);
                if sp.distance(aim) <= r && best.map_or(true, |b| w < b.1) {
                    best = Some((ti, w));
                }
            }
        }
        let mut boss_hit = false;
        if let Some(b) = &self.boss {
            if !b.defeated && b.t > 1.5 {
                for (c, r) in self.boss_hit_spheres(b) {
                    if let Some((sp, w)) = self.project(c) {
                        if sp.distance(aim) <= self.pixel_radius(r, w) * 0.9 && best.map_or(true, |bb| w < bb.1) {
                            boss_hit = true;
                            best = None;
                        }
                    }
                }
            }
        }
        if let Some((ti, _)) = best {
            let t = self.targets.remove(ti);
            let pts = t.kind.points();
            let p = &mut self.players[pi];
            p.score += pts;
            p.hits += 1;
            p.counts[t.kind.index()] += 1;
            self.popups.push(Popup { pos: t.pos + Vec3::Y * 0.5, text: format!("+{pts}"), color, age: 0.0, big: t.kind == Kind::Candy });
            self.burst(t.pos, if t.kind == Kind::Candy { 40 } else { 22 }, color, t.kind == Kind::Ghost);
            if let Some(src) = self.players[pi].source {
                self.rumble_requests.push((src, 80));
            }
        } else if boss_hit {
            self.hit_boss(pi);
        } else if let Some(dir) = self.ray_dir(aim) {
            // A harmless sparkle where the shot lands nearby.
            let p = self.cam + dir * 6.0;
            for _ in 0..5 {
                let v = Vec3::new(self.rng.range(-1.0, 1.0), self.rng.range(-1.0, 1.0), self.rng.range(-1.0, 1.0));
                self.particles.push(Particle { pos: p, vel: v * 1.5, color, size: 0.06, age: 0.0, life: 0.35, glow: 1.0, gravity: 0.0 });
            }
        }
    }

    fn ray_dir(&self, px: Vec2) -> Option<Vec3> {
        let inv = self.viewproj.inverse();
        let ndc = Vec2::new(px.x / self.screen.x * 2.0 - 1.0, 1.0 - px.y / self.screen.y * 2.0);
        let a = inv * glam::Vec4::new(ndc.x, ndc.y, 0.0, 1.0);
        let b = inv * glam::Vec4::new(ndc.x, ndc.y, 1.0, 1.0);
        let a = a.xyz() / a.w;
        let b = b.xyz() / b.w;
        Some((b - a).normalize_or_zero())
    }

    fn burst(&mut self, at: Vec3, n: usize, player_color: [f32; 3], puff: bool) {
        let candy = [[1.0, 0.55, 0.1], [0.7, 0.3, 1.0], [0.4, 1.0, 0.3], [1.0, 0.9, 0.2], [1.0, 0.45, 0.75], [1.0, 1.0, 1.0]];
        for k in 0..n {
            let v = Vec3::new(self.rng.range(-1.0, 1.0), self.rng.range(-0.2, 1.4), self.rng.range(-1.0, 1.0)).normalize_or_zero()
                * self.rng.range(2.0, 5.5);
            let color = if k % 4 == 0 { player_color } else { candy[self.rng.int(0, 5) as usize] };
            self.particles.push(Particle {
                pos: at,
                vel: v,
                color,
                size: self.rng.range(0.09, 0.17),
                age: 0.0,
                life: self.rng.range(0.7, 1.3),
                glow: 0.9,
                gravity: 7.0,
            });
        }
        if puff {
            for _ in 0..10 {
                let v = Vec3::new(self.rng.range(-1.0, 1.0), self.rng.range(-0.5, 1.0), self.rng.range(-1.0, 1.0)) * 1.2;
                self.particles.push(Particle {
                    pos: at,
                    vel: v,
                    color: [0.9, 0.9, 1.0],
                    size: self.rng.range(0.25, 0.4),
                    age: 0.0,
                    life: 0.5,
                    glow: 0.8,
                    gravity: -0.5,
                });
            }
        }
    }

    // ================================================================= boss

    fn boss_parts(b: &Boss) -> (Vec3, [Vec3; 2]) {
        let t = b.t;
        let head = b.pos;
        let l = head + Vec3::new(-6.2 + (t * 2.0).sin() * 0.6, -2.0 + (t * 3.1).sin() * 1.2, -1.5);
        let r = head + Vec3::new(6.2 + (t * 2.3).cos() * 0.6, -2.0 + (t * 2.7).cos() * 1.2, -1.5);
        (head, [l, r])
    }

    fn boss_hit_spheres(&self, b: &Boss) -> Vec<(Vec3, f32)> {
        let (head, hands) = Self::boss_parts(b);
        vec![(head + Vec3::Y * 0.5, 3.8), (hands[0], 1.2), (hands[1], 1.2)]
    }

    fn update_boss(&mut self, dt: f32) {
        let n_targets = self.targets.len();
        let Some(b) = self.boss.as_mut() else { return };
        b.t += dt;
        b.flash = (b.flash - dt * 5.0).max(0.0);
        b.wobble = (b.wobble - dt * 2.0).max(0.0);
        let rage = 1.0 - b.hp / b.max_hp;
        let rise = smooth(b.t / 2.5);
        let x = (b.t * (0.45 + rage * 0.5)).sin() * (7.5 + rage * 2.0) * smooth((b.t - 2.0) / 2.0);
        let y = -26.0 + (16.5 + (b.t * 1.1).sin() * 1.3) * rise;
        let z = 200.0 + (b.t * 0.37).sin() * 2.5 - rage * 2.0;
        b.pos = Vec3::new(x, y, z);
        b.taunt -= dt;
        if b.taunt <= 0.0 {
            b.taunt = 5.5;
            let lines = ["MWAHAHA!", "TRICK OR TREAT!", "YOU CAN'T CATCH ME!", "OOH, THAT TICKLES!", "BOO!"];
            b.taunt_text = lines[(b.t as usize / 5) % lines.len()];
        }
        // Summon helpers at 75%, 50% and 25% health (extra points for everyone).
        let thresholds = [0.75, 0.5, 0.25];
        let mut summon = false;
        if (b.summons_done as usize) < thresholds.len() && b.hp / b.max_hp < thresholds[b.summons_done as usize] {
            b.summons_done += 1;
            b.taunt = 3.0;
            b.taunt_text = "GET THEM, MY BATS!";
            summon = true;
        }
        let pos = b.pos;
        let defeated = b.defeated;
        if summon && n_targets < 12 {
            let n = 3 + self.joined_count();
            for k in 0..n {
                let kind = if k % 2 == 0 { Kind::Bat } else { Kind::Pumpkin };
                let seed = self.rng.next_u32() as u64;
                let side = if k % 2 == 0 { 1.0 } else { -1.0 };
                self.spawn_target(kind, seed, side, Some(pos));
            }
        }
        if defeated {
            self.state = State::Victory;
            self.state_t = 0.0;
        }
    }

    fn hit_boss(&mut self, pi: usize) {
        let color = PLAYER_COLORS[pi];
        let Some(b) = self.boss.as_mut() else { return };
        b.hp -= 1.0;
        b.flash = 1.0;
        b.wobble = 1.0;
        let final_hit = b.hp <= 0.0;
        let pos = b.pos;
        let p = &mut self.players[pi];
        p.hits += 1;
        p.counts[7] += 1;
        let pts = if final_hit { 1000 } else { 25 };
        p.score += pts;
        self.popups.push(Popup {
            pos: pos + Vec3::new(self.rng.range(-2.0, 2.0), self.rng.range(0.0, 3.0), -3.0),
            text: if final_hit { "+1000 FINAL HIT!".into() } else { format!("+{pts}") },
            color,
            age: 0.0,
            big: final_hit,
        });
        if let Some(src) = p.source {
            self.rumble_requests.push((src, if final_hit { 500 } else { 60 }));
        }
        let at = pos + Vec3::new(self.rng.range(-2.5, 2.5), self.rng.range(-1.0, 3.0), -3.5);
        self.burst(at, 8, color, false);
        if final_hit {
            if let Some(b) = self.boss.as_mut() {
                b.defeated = true;
            }
            for _ in 0..6 {
                let at = pos + Vec3::new(self.rng.range(-3.0, 3.0), self.rng.range(-2.0, 4.0), self.rng.range(-3.0, 3.0));
                self.burst(at, 40, color, true);
            }
            self.targets.clear();
            self.banner = ("YOU BEAT THE PUMPKIN KING!".into(), 0.0);
            self.shake = 1.0;
            // Everyone who rode gets a party bonus.
            for p in self.players.iter_mut().filter(|p| p.joined) {
                p.score += 500;
            }
        }
    }

    // ================================================================= rendering

    pub fn frame(&self) -> FrameOut {
        FrameOut { viewproj: self.viewproj, cam: self.cam, fog: self.fog, fog_range: self.fog_range, lamp: self.lamp }
    }

    pub fn static_mesh(&self) -> Vec<Vertex> {
        self.world.build_mesh(&self.track)
    }

    pub fn draw_world(&self, out: &mut Vec<Vertex>) {
        let m = &self.models;
        // The cart in front of the riders.
        let d = match self.state {
            State::Title => 0.0,
            State::Ride => self.track.dist_at_time(self.ride_t),
            State::Boss | State::Victory => self.track.stop_dist,
            State::Outro => self.track.stop_dist + 0.9 * self.state_t * self.state_t,
            State::Results => self.track.stop_dist + 20.0,
        };
        let f = self.track.frame_at(d);
        let basis = Mat4::from_cols(f.right.extend(0.0), f.up.extend(0.0), f.fwd.extend(0.0), Vec3::ZERO.extend(1.0));
        let cart_m = Mat4::from_translation(f.pos + f.up * 0.3 + f.fwd * 1.7) * basis;
        let light = self.world.light_at(f.pos + Vec3::Y * 1.0);
        m.cart.draw(out, &cart_m, light.max(Vec3::splat(0.25)), 0.0, None);

        for t in &self.targets {
            let s = t.scale.max(0.001);
            let to_cam = self.cam - t.pos;
            let yaw = to_cam.x.atan2(to_cam.z);
            let light = self.world.light_at(t.pos);
            let base = Mat4::from_translation(t.pos) * Mat4::from_rotation_y(yaw) * Mat4::from_scale(Vec3::splat(s));
            let a = t.age;
            // Gentle pulse so little players can tell what to aim at.
            let hl = 0.12 + 0.12 * (a * 6.0 + t.phase).sin();
            match t.kind {
                Kind::Ghost => {
                    let tint = if zone_at(t.pos) == Zone::Crypt { Some([0.7, 1.0, 0.8]) } else { None };
                    let mm = base * Mat4::from_rotation_z((a * 2.0 + t.phase).sin() * 0.12) * Mat4::from_translation(-Vec3::Y * m.ghost.size.y * 0.5);
                    m.ghost.draw(out, &mm, light, hl, tint);
                }
                Kind::Pumpkin => {
                    let mm = base * Mat4::from_rotation_z((a * 3.0 + t.phase).sin() * 0.12) * Mat4::from_translation(-Vec3::Y * 0.56);
                    m.pumpkin.draw(out, &mm, light, hl, None);
                }
                Kind::Bat => {
                    let body = base * Mat4::from_translation(-Vec3::Y * 0.3);
                    m.bat_body.draw(out, &body, light, hl, None);
                    let flap = (a * 18.0 + t.phase).sin() * 0.8;
                    let lw = base * Mat4::from_translation(Vec3::new(-0.27, 0.0, 0.0)) * Mat4::from_rotation_z(flap) * Mat4::from_translation(Vec3::new(-0.36, 0.0, 0.0));
                    m.bat_wing.draw(out, &lw, light, hl, None);
                    let rw = base * Mat4::from_translation(Vec3::new(0.27, 0.0, 0.0)) * Mat4::from_rotation_z(-flap) * Mat4::from_rotation_y(std::f32::consts::PI) * Mat4::from_translation(Vec3::new(-0.36, 0.0, 0.0));
                    m.bat_wing.draw(out, &rw, light, hl, None);
                }
                Kind::Candy => {
                    let mm = Mat4::from_translation(t.pos) * Mat4::from_rotation_y(a * 5.0) * Mat4::from_rotation_x((a * 3.0).sin() * 0.5) * Mat4::from_scale(Vec3::splat(s)) * Mat4::from_translation(-Vec3::Y * 0.26);
                    m.candy.draw(out, &mm, light, hl, None);
                    if (a * 10.0) as i32 % 2 == 0 {
                        let sp = t.pos + Vec3::new((a * 7.0).sin() * 0.7, (a * 9.0).cos() * 0.4, (a * 5.0).cos() * 0.7);
                        push_box(out, &(Mat4::from_translation(sp) * Mat4::from_scale(Vec3::splat(0.08))), T_WHITE, [1.0, 0.95, 0.6], light, 1.0);
                    }
                }
                Kind::Skeleton => {
                    let torso = base * Mat4::from_translation(Vec3::new(0.0, -0.85, 0.0));
                    m.torso.draw(out, &torso, light, hl, None);
                    let head = base * Mat4::from_translation(Vec3::new(0.0, -0.04, 0.0)) * Mat4::from_rotation_z((a * 4.0).sin() * 0.15);
                    m.skull.draw(out, &head, light, hl, None);
                    let wave = (a * 8.0).sin() * 0.6 + 2.4;
                    let la = base * Mat4::from_translation(Vec3::new(-0.38, -0.12, 0.0)) * Mat4::from_rotation_z(-wave) * Mat4::from_translation(Vec3::new(0.0, -0.8, 0.0));
                    m.arm.draw(out, &la, light, hl, None);
                    let ra = base * Mat4::from_translation(Vec3::new(0.38, -0.12, 0.0)) * Mat4::from_rotation_z(0.15) * Mat4::from_translation(Vec3::new(0.0, -0.8, 0.0));
                    m.arm.draw(out, &ra, light, hl, None);
                }
                Kind::Spider => {
                    // Silk thread from the ceiling.
                    let top = t.a;
                    let len = (top.y - t.pos.y).max(0.05);
                    let thread = Mat4::from_translation(Vec3::new(t.pos.x, t.pos.y + len * 0.5, t.pos.z)) * Mat4::from_scale(Vec3::new(0.03, len, 0.03));
                    push_box(out, &thread, T_WHITE, [0.85, 0.85, 0.9], light, 0.5);
                    let body = base * Mat4::from_translation(-Vec3::Y * 0.22);
                    m.spider.draw(out, &body, light, hl, None);
                    for k in 0..8 {
                        let side = if k < 4 { -1.0 } else { 1.0 };
                        let j = (k % 4) as f32;
                        let wig = (a * 9.0 + j * 1.3).sin() * 0.3;
                        let leg = base
                            * Mat4::from_translation(Vec3::new(side * 0.3, 0.25 - j * 0.16, 0.0))
                            * Mat4::from_rotation_z(side * (0.9 + wig))
                            * Mat4::from_translation(Vec3::new(side * 0.3, 0.0, 0.0))
                            * Mat4::from_scale(Vec3::new(0.6, 0.06, 0.06));
                        push_box(out, &leg, T_WHITE, [0.17, 0.13, 0.2], light, 0.2);
                    }
                }
                Kind::Book => {
                    let open = 0.35 + 0.9 * (0.5 + 0.5 * (a * 10.0 + t.phase).sin());
                    let flight = base * Mat4::from_rotation_x(0.4);
                    let half = &m.book_half[t.variant % 3];
                    let r = flight * Mat4::from_rotation_z(open) * Mat4::from_translation(Vec3::new(0.25, 0.0, 0.0));
                    half.draw(out, &r, light, hl, None);
                    let l = flight * Mat4::from_rotation_y(std::f32::consts::PI) * Mat4::from_rotation_z(open) * Mat4::from_translation(Vec3::new(0.25, 0.0, 0.0));
                    half.draw(out, &l, light, hl, None);
                }
            }
        }

        // The Pumpkin King.
        if let Some(b) = &self.boss {
            let (head, hands) = Self::boss_parts(b);
            let mut s = 1.0;
            let mut spin = 0.0;
            if self.state == State::Victory || self.state == State::Outro {
                let k = if self.state == State::Victory { self.state_t } else { 99.0 };
                s = (1.0 - k / 1.6).max(0.0);
                spin = k * 8.0;
            }
            if s > 0.0 {
                let to_cam = self.cam - head;
                let yaw = to_cam.x.atan2(to_cam.z) + spin;
                let wob = (b.t * 25.0).sin() * 0.15 * b.wobble;
                let light = Vec3::splat(0.95);
                let rage = 1.0 - b.hp / b.max_hp;
                let model = if rage > 0.5 { &m.boss_head_angry } else { &m.boss_head };
                let hm = Mat4::from_translation(head)
                    * Mat4::from_rotation_y(yaw)
                    * Mat4::from_rotation_z(wob + (b.t * 1.3).sin() * 0.06)
                    * Mat4::from_scale(Vec3::splat(s))
                    * Mat4::from_translation(-Vec3::Y * 3.2);
                model.draw(out, &hm, light, b.flash * 0.8, None);
                let cm = Mat4::from_translation(head - Vec3::Y * 3.4 * s)
                    * Mat4::from_rotation_y(yaw)
                    * Mat4::from_scale(Vec3::splat(s))
                    * Mat4::from_translation(-Vec3::Y * 4.4);
                m.boss_cape.draw(out, &cm, light, b.flash * 0.5, None);
                for (k, h) in hands.iter().enumerate() {
                    let side = if k == 0 { -1.0 } else { 1.0 };
                    let hm = Mat4::from_translation(head + (*h - head) * s)
                        * Mat4::from_rotation_y(yaw)
                        * Mat4::from_rotation_z(side * (0.3 + (b.t * 3.0 + k as f32).sin() * 0.4))
                        * Mat4::from_scale(Vec3::splat(s))
                        * Mat4::from_translation(-Vec3::Y * 1.0);
                    m.boss_hand.draw(out, &hm, light, b.flash * 0.6, None);
                }
            }
        }

        for p in &self.particles {
            let k = 1.0 - (p.age / p.life);
            let sz = p.size * (0.4 + 0.6 * k);
            let mm = Mat4::from_translation(p.pos) * Mat4::from_rotation_y(p.age * 7.0) * Mat4::from_rotation_x(p.age * 5.0) * Mat4::from_scale(Vec3::splat(sz));
            push_box(out, &mm, T_WHITE, p.color, Vec3::splat(0.5), p.glow);
        }
    }

    pub fn draw_ui(&self, ui: &mut Ui) {
        let (w, h) = (ui.w, ui.h);
        let unit = ui.unit;
        let white = [1.0, 1.0, 1.0, 1.0];
        let orange = [1.0, 0.6, 0.1, 1.0];

        // Score popups (projected world text).
        for p in &self.popups {
            if let Some((sp, _)) = self.project(p.pos) {
                let a = 1.0 - smooth((p.age - 0.8) / 0.5);
                let sc = if p.big { 2.0 } else { 1.4 } * (1.0 + (1.0 - smooth(p.age / 0.15)) * 0.6);
                ui.text(&p.text, sp.x, sp.y, sc, [p.color[0], p.color[1], p.color[2], a], Align::Center);
            }
        }

        match self.state {
            State::Title => self.draw_title(ui),
            State::Results => self.draw_results(ui),
            _ => {}
        }

        if matches!(self.state, State::Ride | State::Boss | State::Victory | State::Outro) {
            // Countdown.
            if self.state == State::Ride && self.ride_t < 3.6 {
                let n = 3 - (self.ride_t / 1.0) as i32;
                let (txt, col) = if n > 0 { (format!("{n}"), orange) } else { ("GO!".to_string(), [0.4, 1.0, 0.4, 1.0]) };
                let pulse = 1.0 + (1.0 - (self.ride_t.fract())) * 0.5;
                ui.text(&txt, w * 0.5, h * 0.36, 6.0 * pulse, col, Align::Center);
            }
            // Zone banner.
            let bt = self.banner.1;
            if bt >= 0.0 && bt < 3.0 {
                let a = smooth(bt / 0.3) * (1.0 - smooth((bt - 2.4) / 0.6));
                let by = if self.boss.is_some() { h * 0.68 } else { h * 0.16 };
                ui.text_fit(&self.banner.0, w * 0.5, by, 2.6, w * 0.9, [1.0, 0.75, 0.3, a], Align::Center);
            }
            // Boss bar and taunts.
            if let Some(b) = &self.boss {
                if !b.defeated && self.state == State::Boss {
                    let bw = w * 0.5;
                    let x = w * 0.25;
                    let y = 12.0 * unit;
                    ui.text("THE PUMPKIN KING", w * 0.5, y - 9.0 * unit, 1.4, orange, Align::Center);
                    ui.panel(x, y, bw, 7.0 * unit, [0.1, 0.03, 0.12, 0.85], [0.0, 0.0, 0.0, 0.9]);
                    let f = (b.hp / b.max_hp).max(0.0);
                    ui.rect(x + unit * 1.5, y + unit * 1.5, (bw - unit * 3.0) * f, 4.0 * unit, [1.0, 0.45 + 0.3 * f, 0.05, 1.0]);
                    if b.taunt > 3.5 && b.t > 3.0 {
                        if let Some((sp, _)) = self.project(b.pos + Vec3::Y * 5.5) {
                            let ty = sp.y.max(y + 14.0 * unit);
                            ui.text_fit(b.taunt_text, sp.x.clamp(w * 0.3, w * 0.7), ty, 1.6, w * 0.6, [1.0, 0.9, 0.4, 1.0], Align::Center);
                        }
                    }
                }
            }
            self.draw_scores(ui);
        }

        // Fade to black at the end of the ride.
        if self.state == State::Outro {
            let a = smooth((self.state_t - 1.5) / 2.5);
            ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, a]);
        }

        // Crosshairs on top of everything.
        if !matches!(self.state, State::Outro | State::Results) {
            for (i, p) in self.players.iter().enumerate() {
                if !p.aim_visible || (!p.joined && self.state != State::Title) {
                    continue;
                }
                let c = PLAYER_COLORS[i];
                let alpha = if p.joined { 1.0 } else { 0.45 };
                let size = 16.0 * unit * (1.0 + p.kick * 0.35);
                ui.sprite(T_RING, p.aim.x, p.aim.y, size + unit * 2.0, [0.0, 0.0, 0.0, 0.7 * alpha]);
                ui.sprite(T_RING, p.aim.x, p.aim.y, size, [c[0], c[1], c[2], alpha]);
                ui.rect(p.aim.x - unit, p.aim.y - unit, unit * 2.0, unit * 2.0, [1.0, 1.0, 1.0, alpha]);
                ui.text(&format!("P{}", i + 1), p.aim.x + size * 0.45, p.aim.y + size * 0.3, 0.9, [c[0], c[1], c[2], alpha], Align::Left);
            }
        }

        if self.paused {
            ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
            ui.text("PAUSED", w * 0.5, h * 0.4, 4.0, orange, Align::Center);
            ui.text("PRESS HOME TO KEEP RIDING", w * 0.5, h * 0.4 + 40.0 * unit, 1.3, white, Align::Center);
            ui.text("PRESS - TO GO BACK TO THE START", w * 0.5, h * 0.4 + 52.0 * unit, 1.0, [0.8, 0.8, 0.8, 1.0], Align::Center);
        }

        if self.show_debug {
            let mut y = 4.0 * unit;
            let mut lines = vec![
                format!("FPS {:.0}  STATE {:?}", self.fps, self.state),
                format!("RIDE {:.1}S / {:.0}S  TARGETS {}", self.ride_t, RIDE_SECONDS, self.targets.len()),
                format!("PARTICLES {}  SPAWNS {}/{}", self.particles.len(), self.next_spawn, self.spawns.len()),
            ];
            if let Some(b) = &self.boss {
                lines.push(format!("BOSS HP {:.0}/{:.0}  T {:.1}", b.hp, b.max_hp, b.t));
            }
            lines.extend(self.debug_lines.iter().cloned());
            for l in lines {
                ui.text(&l, 4.0 * unit, y, 0.75, [0.7, 1.0, 0.7, 0.9], Align::Left);
                y += 7.0 * unit;
            }
        }
    }

    fn draw_scores(&self, ui: &mut Ui) {
        let (w, h, unit) = (ui.w, ui.h, ui.unit);
        let pw = w / 4.0;
        let ph = 18.0 * unit;
        for (i, p) in self.players.iter().enumerate() {
            let x = i as f32 * pw + unit * 3.0;
            let y = h - ph - unit * 3.0;
            let c = PLAYER_COLORS[i];
            if p.joined {
                ui.panel(x, y, pw - unit * 6.0, ph, [c[0] * 0.25, c[1] * 0.25, c[2] * 0.25, 0.8], [c[0], c[1], c[2], 1.0]);
                ui.text(&format!("P{}", i + 1), x + unit * 4.0, y + unit * 5.0, 1.0, [c[0], c[1], c[2], 1.0], Align::Left);
                ui.text(&format!("{}", p.shown.round() as u32), x + pw - unit * 10.0, y + unit * 3.5, 1.6, [1.0, 1.0, 1.0, 1.0], Align::Right);
            } else {
                ui.panel(x, y, pw - unit * 6.0, ph, [0.0, 0.0, 0.0, 0.35], [c[0] * 0.5, c[1] * 0.5, c[2] * 0.5, 0.6]);
                let txt = if self.debug { "PRESS A / 1-4" } else { "PRESS A" };
                ui.text(txt, x + (pw - unit * 6.0) * 0.5, y + unit * 6.0, 0.8, [c[0], c[1], c[2], 0.7], Align::Center);
            }
        }
    }

    fn draw_title(&self, ui: &mut Ui) {
        let (w, h, unit) = (ui.w, ui.h, ui.unit);
        let t = self.state_t;
        let bob = (t * 2.0).sin() * 2.0 * unit;
        ui.rect(0.0, h * 0.1, w, 52.0 * unit, [0.0, 0.0, 0.0, 0.35]);
        ui.text_fit("HAUNTED HOLLOW", w * 0.5, h * 0.12 + bob, 5.0, w * 0.92, [1.0, 0.55, 0.08, 1.0], Align::Center);
        ui.text("THE SPOOKY COASTER RIDE", w * 0.5, h * 0.12 + 46.0 * unit, 1.5, [0.75, 0.55, 1.0, 1.0], Align::Center);

        let pw = w / 4.0;
        let y = h * 0.58;
        for (i, p) in self.players.iter().enumerate() {
            let c = PLAYER_COLORS[i];
            let x = i as f32 * pw + pw * 0.5;
            let bw = pw - 12.0 * unit;
            let a = if p.joined { 1.0 } else { 0.5 };
            ui.panel(x - bw * 0.5, y, bw, 34.0 * unit, [c[0] * 0.2, c[1] * 0.2, c[2] * 0.2, 0.8 * a], [c[0], c[1], c[2], a]);
            ui.text(&format!("PLAYER {}", i + 1), x, y + 5.0 * unit, 1.1, [c[0], c[1], c[2], a], Align::Center);
            ui.text(PLAYER_NAMES[i], x, y + 14.0 * unit, 0.8, [c[0], c[1], c[2], a], Align::Center);
            let status = if p.joined {
                "READY!"
            } else if (t * 2.0) as i32 % 2 == 0 {
                if self.debug { "CLICK / PRESS A" } else { "PRESS A" }
            } else {
                ""
            };
            ui.text(status, x, y + 24.0 * unit, 1.0, [1.0, 1.0, 1.0, 1.0], Align::Center);
        }
        let msg = if self.joined_count() == 0 {
            "PRESS A ON YOUR WII REMOTE TO JOIN".to_string()
        } else {
            let left = 15.0 - self.title_join_t.unwrap_or(0.0);
            let key = if self.debug { "PRESS + OR ENTER TO START" } else { "PRESS + TO START" };
            format!("{key}   ({:.0})", left.max(0.0).ceil())
        };
        ui.text_fit(&msg, w * 0.5, h * 0.86, 1.3, w * 0.92, [1.0, 0.9, 0.5, 1.0], Align::Center);
        ui.text_fit("POINT AT THE SCREEN AND PRESS B TO POP THE SPOOKS!", w * 0.5, h * 0.92, 0.8, w * 0.92, [0.8, 0.8, 0.9, 0.9], Align::Center);
    }

    fn draw_results(&self, ui: &mut Ui) {
        let (w, h, unit) = (ui.w, ui.h, ui.unit);
        ui.rect(0.0, 0.0, w, h, [0.04, 0.02, 0.07, 0.92]);
        ui.text_fit("WHAT A SPOOKTACULAR RIDE!", w * 0.5, h * 0.08, 2.6, w * 0.92, [1.0, 0.55, 0.08, 1.0], Align::Center);
        let mut order: Vec<usize> = (0..4).filter(|&i| self.players[i].joined).collect();
        order.sort_by(|a, b| self.players[*b].score.cmp(&self.players[*a].score));
        let medal = [("1ST", [1.0, 0.82, 0.2]), ("2ND", [0.8, 0.82, 0.9]), ("3RD", [0.85, 0.55, 0.3]), ("4TH", [0.6, 0.6, 0.7])];
        let row_h = 34.0 * unit;
        let top = h * 0.24;
        let reveal = self.state_t;
        for (rank, &i) in order.iter().enumerate() {
            let appear = smooth((reveal - 0.4 * (order.len() - rank) as f32) / 0.4);
            if appear <= 0.0 {
                continue;
            }
            let p = &self.players[i];
            let c = PLAYER_COLORS[i];
            let y = top + rank as f32 * (row_h + 6.0 * unit);
            let x = w * 0.12;
            let rw = w * 0.76;
            ui.panel(x, y, rw, row_h, [c[0] * 0.2, c[1] * 0.2, c[2] * 0.2, 0.9 * appear], [c[0], c[1], c[2], appear]);
            let (mt, mc) = medal[rank];
            ui.text(mt, x + 6.0 * unit, y + 9.0 * unit, 2.0, [mc[0], mc[1], mc[2], appear], Align::Left);
            ui.text(&format!("PLAYER {}", i + 1), x + 50.0 * unit, y + 6.0 * unit, 1.3, [c[0], c[1], c[2], appear], Align::Left);
            ui.text(self.award(i), x + 50.0 * unit, y + 20.0 * unit, 1.0, [1.0, 1.0, 1.0, appear], Align::Left);
            ui.text(&format!("{}", p.score), x + rw - 6.0 * unit, y + 6.0 * unit, 2.0, [1.0, 1.0, 1.0, appear], Align::Right);
            let acc = if p.shots > 0 { p.hits * 100 / p.shots } else { 0 };
            ui.text(&format!("{} POPS  {}% AIM", p.hits, acc), x + rw - 6.0 * unit, y + 23.0 * unit, 0.8, [0.85, 0.85, 0.9, appear], Align::Right);
        }
        if self.state_t > 4.0 && (self.state_t * 2.0) as i32 % 2 == 0 {
            ui.text("PRESS A TO RIDE AGAIN!", w * 0.5, h * 0.9, 1.5, [1.0, 0.9, 0.5, 1.0], Align::Center);
        }
    }

    fn award(&self, i: usize) -> &'static str {
        let p = &self.players[i];
        let mut best = 8;
        let mut best_v = 0;
        // Boss hits count a little less so the boss doesn't always win.
        for (k, &v) in p.counts.iter().enumerate() {
            let v = if k == 7 { v / 3 } else { v };
            if v > best_v {
                best_v = v;
                best = k;
            }
        }
        AWARDS[best]
    }
}
