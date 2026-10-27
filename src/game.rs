//! Gameplay: the ride, spooky targets, the Pumpkin King, scoring and all screens.
//! No one can lose: targets just pop into candy and every player earns a score.

use crate::atlas::*;
use crate::audio::{Music, Sfx, SoundEvent};
use crate::highscores::{HighScores, MAX_INITIALS};
use crate::hud::{Align, Ui};
use crate::mesh::{hash3, push_box, push_box_tiles, push_quad, Models, FACES};
use crate::rng::Rng;
use crate::track::{Track, RIDE_SECONDS};
use crate::vk::Vertex;
use crate::world::{zone_at, World, Zone, DOOR_FLOOR, DOOR_HALF, DOOR_Z};
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
    /// A haunted painting that peels off a wall of the house and floats at the riders.
    Portrait,
}

impl Kind {
    fn points(self) -> u32 {
        match self {
            Kind::Pumpkin | Kind::Ghost => 100,
            Kind::Bat | Kind::Book | Kind::Spider => 150,
            Kind::Skeleton | Kind::Portrait => 200,
            Kind::Candy => 500,
        }
    }
    fn radius(self) -> f32 {
        match self {
            Kind::Ghost => 0.85,
            Kind::Skeleton => 0.8,
            Kind::Pumpkin => 0.7,
            Kind::Spider => 0.7 * 1.5,
            Kind::Bat | Kind::Book => 0.6 * 1.5,
            Kind::Candy => 0.6,
            Kind::Portrait => 0.75,
        }
    }
    fn index(self) -> usize {
        self as usize
    }
    /// Drawing scale: spiders, bats and books are 50% bigger than their models.
    fn size(self) -> f32 {
        match self {
            Kind::Spider | Kind::Bat | Kind::Book => 1.5,
            _ => 1.0,
        }
    }
}

/// Award per enemy kind (in `Kind` order), then the boss, then the fallback.
const AWARDS: [&str; 10] = [
    "PUMPKIN POPPER",
    "GHOST CATCHER",
    "BAT BOPPER",
    "BOOKWORM",
    "BONE BONKER",
    "SPIDER SPOTTER",
    "CANDY CHAMP",
    "ART CRITIC",
    "BOSS BUSTER",
    "SPOOKY STAR",
];

/// Bats in each batch of the swarm on the big drop.
const SWARM_BATCH: usize = 3;
/// Index of boss hits in `Player::counts` (after one slot per enemy kind).
const BOSS_COUNT: usize = 8;
/// Seconds a haunted portrait spends peeling off its wall before it flies.
const PORTRAIT_PEEL: f32 = 0.8;

/// Keys of the shootable letter grid: A-Z, then delete and done.
const KEYS: [&str; 28] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V",
    "W", "X", "Y", "Z", "DEL", "END",
];
const KEY_DEL: usize = 26;
const KEY_END: usize = 27;
const KEY_COLS: usize = 7;
/// How long riders get to enter their initials before the rest are filled in.
const INITIALS_SECONDS: f32 = 45.0;

#[derive(Clone, Default)]
struct Initials {
    letters: String,
    /// When (in `state_t`) this rider shot END.
    done_at: Option<f32>,
}

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
    /// Hits per enemy kind, with boss hits last (`BOSS_COUNT`).
    counts: [u32; 9],
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
            counts: [0; 9],
            bot_target: None,
            bot_think: 0.0,
        }
    }
    fn reset_score(&mut self) {
        self.score = 0;
        self.shown = 0.0;
        self.hits = 0;
        self.shots = 0;
        self.counts = [0; 9];
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum State {
    Title,
    /// "Pull B to shoot" lesson: every player pops a practice target before the countdown.
    Tutorial,
    Ride,
    Boss,
    Victory,
    Outro,
    Results,
    /// Riders who made the high-score table shoot the letters of their initials.
    Initials,
    /// The high-score table, before going back to the title.
    HighScores,
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
    /// Part of the bat swarm on the big drop (placed by `spawn_swarm_bat`).
    swarm: bool,
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
    pub sounds: Vec<SoundEvent>,
    last_spawn_sound: [f32; 8],
    pub spawn_stats: (u32, u32),
    /// How many of each kind actually appeared this ride (for testing).
    pub spawn_kinds: [u32; 8],
    lane: usize,
    /// Fireplace and brazier flames, animated every frame.
    fires: Vec<Vec3>,
    /// Witch's cauldrons, whose brew bubbles every frame.
    cauldrons: Vec<Vec3>,
    /// Cloud layer over the graveyard as (x start, z, length in cells); drifts every frame.
    clouds: Vec<(f32, f32, f32)>,
    /// Translucent mist puffs: graveyard, crypt and the hole down into it.
    mist: Vec<Puff>,
    /// How far the boss doors are open (0 shut .. 1 wide open).
    door_open: f32,
    retries: Vec<(f32, Kind, u64, u32)>,
    high: HighScores,
    high_path: Option<std::path::PathBuf>,
    /// Initials being entered by each rider who made the table this round.
    initials: [Option<Initials>; 4],
    /// When each key of the letter grid was last shot, and by whom.
    key_flash: [(f32, usize); KEYS.len()],
    /// Rows of the table that are new this round, and whose they are.
    high_new: Vec<(usize, usize)>,
    cam_dir: Vec3,
    cam_up: Vec3,
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
    /// When (in `state_t`) each player popped their practice target.
    practice: [Option<f32>; 4],
    pub rumble_requests: Vec<(SourceId, u32)>,
    pub led_requests: Vec<(SourceId, usize)>,
    pub bots: u32,
    screen: Vec2,
    pub fps: f32,
    pub debug_lines: Vec<String>,
    shake: f32,
}

/// When (seconds into the victory) the boss doors start to open, and how long it takes.
const DOOR_START: f32 = 1.8;
const DOOR_TIME: f32 = 2.4;
/// Length of the ride out through the doors before the score screen.
const OUTRO_TIME: f32 = 4.8;
/// Outro cart distance past the boss stop: d = OUTRO_ACCEL * t^2 (doors at ~30 m).
const OUTRO_ACCEL: f32 = 2.6;

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
        Zone::Lair => ([0.015, 0.005, 0.025], (24.0, 80.0), 0.45),
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
            practice: [None; 4],
            rumble_requests: vec![],
            led_requests: vec![],
            bots,
            screen: Vec2::new(1280.0, 720.0),
            fps: 0.0,
            debug_lines: vec![],
            shake: 0.0,
            sounds: vec![],
            last_spawn_sound: [-9.0; 8],
            spawn_stats: (0, 0),
            spawn_kinds: [0; 8],
            lane: 0,
            door_open: 0.0,
            fires: vec![],
            cauldrons: vec![],
            clouds: cloud_runs(),
            mist: vec![],
            retries: vec![],
            high: HighScores::default(),
            high_path: None,
            initials: Default::default(),
            key_flash: [(-9.0, 0); KEYS.len()],
            high_new: vec![],
            cam_dir: Vec3::Z,
            cam_up: Vec3::Y,
        };
        g.fires = g.world.cells_of(crate::world::b::FIRE);
        g.cauldrons = g.world.cells_of(crate::world::b::CAULDRON);
        g.mist = mist_puffs(&g.track);
        for k in 0..bots.min(4) {
            g.players[k as usize].joined = true;
            g.players[k as usize].source = Some(SourceId::Bot(k));
        }
        g
    }

    /// Loads the high-score table from `path` and saves it back there after each round.
    pub fn set_high_score_file(&mut self, path: std::path::PathBuf) {
        self.high = HighScores::load(&path);
        self.high_path = Some(path);
    }

    fn joined_count(&self) -> usize {
        self.players.iter().filter(|p| p.joined).count()
    }

    fn start_tutorial(&mut self) {
        self.state = State::Tutorial;
        self.state_t = 0.0;
        self.ride_t = 0.0;
        self.practice = [None; 4];
        self.targets.clear();
        self.particles.clear();
    }

    /// Screen position of a player's practice target (one per score-card column).
    fn practice_pos(&self, i: usize) -> Vec2 {
        Vec2::new((i as f32 + 0.5) * self.screen.x / 4.0, self.screen.y * 0.7)
    }

    fn practice_radius(&self) -> f32 {
        (self.screen.y / 240.0).max(1.0) * 18.0
    }

    pub fn start_ride(&mut self) {
        self.state = State::Ride;
        self.state_t = 0.0;
        self.ride_t = 0.0;
        self.targets.clear();
        self.retries.clear();
        self.door_open = 0.0;
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

    /// Riders (player indices) whose score this round makes the high-score table.
    /// Computer players never count.
    fn high_score_riders(&self) -> Vec<usize> {
        let scores: Vec<u32> = self
            .players
            .iter()
            .map(|p| if p.joined && !matches!(p.source, Some(SourceId::Bot(_))) { p.score } else { 0 })
            .collect();
        self.high.qualifiers(&scores)
    }

    /// From the results: riders who made the table enter their initials, otherwise
    /// straight on to the table (or the title if there isn't one yet).
    fn leave_results(&mut self) {
        let made_it = self.high_score_riders();
        self.high_new.clear();
        self.state_t = 0.0;
        if !made_it.is_empty() {
            self.initials = Default::default();
            for i in made_it {
                self.initials[i] = Some(Initials::default());
            }
            self.key_flash = [(-9.0, 0); KEYS.len()];
            self.state = State::Initials;
        } else if !self.high.entries.is_empty() {
            self.state = State::HighScores;
        } else {
            self.go_to_title();
        }
    }

    /// Puts everyone's initials into the table (riders who didn't finish keep what they
    /// shot, or get "P1".."P4"), saves it and shows it.
    fn finish_initials(&mut self) {
        let mut order: Vec<usize> = (0..4).filter(|&i| self.initials[i].is_some()).collect();
        // Best first, so a later insert never pushes an earlier one down.
        order.sort_by(|&a, &b| self.players[b].score.cmp(&self.players[a].score));
        for i in order {
            let letters = self.initials[i].take().map(|e| e.letters).unwrap_or_default();
            let name = if letters.is_empty() { format!("P{}", i + 1) } else { letters };
            if let Some(rank) = self.high.insert(self.players[i].score, &name) {
                self.high_new.push((rank, i));
            }
        }
        if let Some(path) = &self.high_path {
            if let Err(e) = self.high.save(path) {
                eprintln!("could not save high scores to {}: {e}", path.display());
            }
        }
        self.state = State::HighScores;
        self.state_t = 0.0;
    }

    fn go_to_title(&mut self) {
        self.state = State::Title;
        self.state_t = 0.0;
        self.title_join_t = None;
        // Back on the title screen every rider has to press A again to join.
        for p in self.players.iter_mut() {
            if matches!(p.source, Some(SourceId::Bot(_))) {
                continue;
            }
            p.joined = false;
            p.aim_visible = false;
            if matches!(p.source, Some(SourceId::Wii(_))) {
                p.source = None;
            }
        }
    }

    /// Screen rectangles (x, y, w, h) of the letter grid's keys, in `KEYS` order.
    fn key_rects(&self) -> Vec<(f32, f32, f32, f32)> {
        let (w, h) = (self.screen.x, self.screen.y);
        let unit = (h / 240.0).max(1.0);
        let rows = KEYS.len().div_ceil(KEY_COLS);
        let gap = 4.0 * unit;
        let cell = ((w * 0.84 - gap * (KEY_COLS - 1) as f32) / KEY_COLS as f32).min((h * 0.46 - gap * (rows - 1) as f32) / rows as f32);
        let gw = cell * KEY_COLS as f32 + gap * (KEY_COLS - 1) as f32;
        let (x0, y0) = ((w - gw) * 0.5, h * 0.2);
        (0..KEYS.len()).map(|k| (x0 + (k % KEY_COLS) as f32 * (cell + gap), y0 + (k / KEY_COLS) as f32 * (cell + gap), cell, cell)).collect()
    }

    fn key_at(&self, p: Vec2) -> Option<usize> {
        self.key_rects().iter().position(|&(x, y, w, h)| p.x >= x && p.x < x + w && p.y >= y && p.y < y + h)
    }

    /// A rider shot at the letter grid: add a letter, delete one, or finish.
    fn initials_shot(&mut self, pi: usize, aim: Vec2) {
        // (Ignore the press that left the results screen.)
        if self.state_t < 0.6 {
            return;
        }
        let Some(k) = self.key_at(aim) else { return };
        let now = self.state_t;
        let Some(e) = self.initials[pi].as_mut().filter(|e| e.done_at.is_none()) else { return };
        let ok = match k {
            KEY_DEL => e.letters.pop().is_some(),
            KEY_END if !e.letters.is_empty() => {
                e.done_at = Some(now);
                true
            }
            KEY_END => false,
            _ if e.letters.len() < MAX_INITIALS => {
                e.letters.push_str(KEYS[k]);
                true
            }
            _ => false,
        };
        let pan = (aim.x / self.screen.x * 2.0 - 1.0) * 0.8;
        if ok {
            self.key_flash[k] = (self.total_t, pi);
            let sfx = if k == KEY_END { Sfx::Ding } else { Sfx::Pop };
            self.sfx(sfx, 0.6, pan, 0.9 + pi as f32 * 0.12);
            if let Some(src) = self.players[pi].source {
                self.rumble_requests.push((src, 60));
            }
        } else {
            self.sfx(Sfx::Bonk, 0.4, pan, 1.0);
        }
    }

    /// Debug helper: jump straight to the boss.
    pub fn skip_to_boss(&mut self) {
        if matches!(self.state, State::Title | State::Tutorial) {
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
            (t_foyer + 0.3, t_lib - 0.6, vec![(Ghost, 5), (Portrait, 3), (Bat, 2)], 1.0),
            (t_lib + 0.3, t_din - 0.6, vec![(Book, 6), (Portrait, 2), (Ghost, 2), (Bat, 1)], 1.0),
            (t_din + 0.3, t_drop - 0.3, vec![(Skeleton, 5), (Portrait, 2), (Ghost, 2), (Bat, 1)], 1.0),
            (t_drop + 0.2, t_crypt - 0.3, vec![(Bat, 1)], 0.6),
            (t_crypt + 0.2, t_lair - 0.4, vec![(Spider, 5), (Skeleton, 3), (Ghost, 3), (Bat, 2)], 1.0),
            (t_lair, RIDE_SECONDS - 3.0, vec![(Pumpkin, 3), (Bat, 2)], 1.0),
        ];
        let mut rng = Rng::new(4242);
        let mut spawns = vec![];
        // Scheduled a bit denser than v2 (1.25 s) so that ~25% more targets actually
        // appear once spots that aren't clearly visible are skipped.
        let interval = 0.84 / (1.0 + 0.32 * (n - 1.0));
        let zones = [Zone::Graveyard, Zone::Foyer, Zone::Library, Zone::Dining, Zone::Drop, Zone::Crypt, Zone::Lair];
        for (si, (t0, t1, pool, density)) in sections.into_iter().enumerate() {
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
                // There were too many ghosts: one in five becomes a different enemy.
                if kind == Ghost && rng.chance(0.2) {
                    kind = Self::ghost_substitute(zones[si.min(zones.len() - 1)], &mut rng);
                }
                let side = rng.sign();
                spawns.push(Spawn { t, kind, seed: rng.next_u32() as u64, side, swarm: false });
                if rng.chance(0.22 + 0.08 * (n - 1.0)) {
                    spawns.push(Spawn { t: t + 0.15, kind, seed: rng.next_u32() as u64, side: -side, swarm: false });
                }
                t += interval / density * rng.range(0.7, 1.3);
            }
            // One golden candy bonus per section.
            if t1 - t0 > 4.0 {
                spawns.push(Spawn { t: rng.range(t0 + 1.0, t1 - 1.0), kind: Candy, seed: rng.next_u32() as u64, side: rng.sign(), swarm: false });
            }
        }
        // Bats burst up the shaft at the riders in three batches: out of the hole as the
        // cart nears it, at the lip, and on the way down (always the same number, however
        // many are riding).
        for start in [t_drop - 0.5, t_drop + 0.4, t_drop + 1.3] {
            for k in 0..SWARM_BATCH {
                let t = start + 0.5 * k as f32 / SWARM_BATCH as f32 + rng.range(0.0, 0.05);
                spawns.push(Spawn { t, kind: Bat, seed: rng.next_u32() as u64, side: 1.0, swarm: true });
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

    /// Camera pose (eye, look direction, up) for the cart at track distance `d`.
    fn pose_at(&self, d: f32, speed: f32) -> (Vec3, Vec3, Vec3) {
        let f = self.track.frame_at(d);
        let ahead = self.track.pos_at(d + 4.0);
        let mut dir = (ahead - f.pos).normalize_or_zero().lerp(f.fwd, 0.4).normalize_or_zero();
        if dir == Vec3::ZERO {
            dir = Vec3::Z;
        }
        let eye = f.pos + Vec3::Y * 1.65;
        let roll = (self.track.turn_rate(d) * speed * 0.22).clamp(-0.3, 0.3);
        let right = dir.cross(Vec3::Y).normalize_or_zero();
        let up0 = right.cross(dir).normalize_or_zero();
        let up = (up0 * roll.cos() + right * roll.sin()).normalize();
        (eye, dir, up)
    }

    fn proj(&self) -> Mat4 {
        let aspect = self.screen.x / self.screen.y.max(1.0);
        Mat4::perspective_rh(70f32.to_radians(), aspect, 0.05, 800.0)
    }

    /// Predicted camera `ahead` seconds from now (used to place targets in view).
    /// Returns (eye, right, up, view-projection).
    fn view_ahead(&self, ahead: f32) -> (Vec3, Vec3, Vec3, Mat4) {
        let (eye, dir, up) = if self.state == State::Ride && self.ride_t + ahead < RIDE_SECONDS {
            let t = self.ride_t + ahead;
            let d = self.track.dist_at_time(t);
            let speed = (self.track.dist_at_time(t + 0.1) - d) * 10.0;
            self.pose_at(d, speed)
        } else {
            let (eye, dir, up) = self.pose_at(self.track.stop_dist, 0.0);
            let boss = self.boss.as_ref().map(|b| b.pos).unwrap_or(Vec3::new(0.0, -9.5, 200.0));
            let to = (Vec3::new(boss.x * 0.5, -9.5, boss.z) - eye).normalize_or_zero();
            (eye, dir.lerp(to, 0.3).normalize(), up)
        };
        let right = dir.cross(up).normalize_or_zero();
        (eye, right, up, self.proj() * Mat4::look_to_rh(eye, dir, up))
    }

    /// True if nothing that hides a target lies between the two points.
    fn clear_line(&self, a: Vec3, b: Vec3) -> bool {
        let len = a.distance(b);
        let dir = (b - a) / len.max(1e-3);
        let mut t = 0.4;
        while t < len - 0.15 {
            if self.world.blocks_view(a + dir * t) {
                return false;
            }
            t += 0.25;
        }
        true
    }

    /// Rough size of each target: (half width, extent below its position, extent above).
    fn extents(kind: Kind) -> (f32, f32, f32) {
        match kind {
            Kind::Ghost => (0.85, 0.75, 0.8),
            Kind::Pumpkin => (0.65, 0.5, 0.9),
            Kind::Skeleton => (0.55, 0.9, 1.0),
            Kind::Spider => (1.1, 0.55, 0.8),
            Kind::Book => (0.95, 0.5, 0.9),
            Kind::Bat => (1.1, 0.6, 0.55),
            Kind::Candy => (0.6, 0.35, 0.35),
            Kind::Portrait => (0.6, 0.75, 0.75),
        }
    }

    /// The target's whole body is in open air (not inside a wall, the floor or a prop).
    fn body_clear(&self, kind: Kind, p: Vec3) -> bool {
        let (hw, below, above) = Self::extents(kind);
        for dy in [-below + 0.08, (above - below) * 0.5, above] {
            for dx in [-hw, 0.0, hw] {
                for dz in [-hw, 0.0, hw] {
                    if self.world.occupied(p + Vec3::new(dx, dy, dz)) {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Screen position (NDC) and clip w of a point if it's comfortably on screen:
    /// not at the edges, not under the score panels or the boss health bar.
    fn on_screen(vp: &Mat4, p: Vec3) -> Option<(Vec2, f32)> {
        let c = *vp * p.extend(1.0);
        if c.w < 3.0 || c.w > 38.0 {
            return None;
        }
        let n = c.xyz() / c.w;
        if n.x.abs() < 0.86 && n.y > -0.62 && n.y < 0.74 {
            Some((n.truncate(), c.w))
        } else {
            None
        }
    }

    /// Every part of the target (centre and four edges) can be seen from the eye.
    fn fully_visible(&self, eye: Vec3, right: Vec3, up: Vec3, kind: Kind, p: Vec3) -> bool {
        let (hw, below, above) = Self::extents(kind);
        let mid = p + up * ((above - below) * 0.5);
        [mid, mid + right * hw * 0.8, mid - right * hw * 0.8, p + up * above * 0.8, p - up * below * 0.8]
            .iter()
            .all(|&q| self.clear_line(eye, q))
    }

    /// Where a target is at a given age (ignoring appear/leave animation).
    fn motion(t: &Target, a: f32) -> Vec3 {
        let ph = t.phase;
        match t.kind {
            Kind::Ghost => t.home + Vec3::new((a * 1.3 + ph).sin() * 0.6, (a * 2.1 + ph).sin() * 0.35, 0.0),
            Kind::Pumpkin => t.home + Vec3::Y * ((a * 3.0 + ph).sin() * 0.06),
            Kind::Skeleton => t.home,
            Kind::Spider => t.home + Vec3::Y * (a * 2.0).sin() * 0.12,
            Kind::Book => t.home + Vec3::new((a * 1.6 + ph).cos() * 0.9, (a * 3.0).sin() * 0.3, (a * 1.6 + ph).sin() * 0.9),
            Kind::Bat => t.a.lerp(t.b, (a / t.life).min(1.0)) + Vec3::Y * (a * 5.0 + ph).sin() * 0.4,
            Kind::Candy => t.a.lerp(t.b, (a / t.life).min(1.0)) + Vec3::Y * (a * 6.0).sin() * 0.3,
            // Hangs just off its wall while it peels away, then glides at the riders.
            Kind::Portrait => {
                if a < PORTRAIT_PEEL {
                    t.a
                } else {
                    let u = ((a - PORTRAIT_PEEL) / (t.life - PORTRAIT_PEEL)).clamp(0.0, 1.0);
                    t.a.lerp(t.b, u * u * (3.0 - 2.0 * u)) + Vec3::Y * (a * 2.5 + ph).sin() * 0.12 * (u * 3.0).min(1.0)
                }
            }
        }
    }

    /// Screen "lanes" new targets are steered toward, so they spread across the
    /// screen instead of bunching in the middle.
    const LANES: [f32; 10] = [-0.6, 0.55, -0.2, 0.3, 0.72, -0.45, 0.05, 0.45, -0.72, -0.05];

    /// Picks a spot for a new target that every player can see and shoot for its whole
    /// life: in open air, never behind walls or props, not overlapping other targets,
    /// and spread out across the screen. Candidates are tested against the camera's
    /// predicted path.
    /// Something other than a ghost that suits the current part of the ride.
    fn ghost_substitute(zone: Zone, rng: &mut Rng) -> Kind {
        let pool: &[Kind] = match zone {
            Zone::Graveyard => &[Kind::Bat, Kind::Pumpkin],
            Zone::Foyer => &[Kind::Bat, Kind::Spider, Kind::Portrait],
            Zone::Library => &[Kind::Book, Kind::Bat, Kind::Spider, Kind::Portrait],
            Zone::Dining => &[Kind::Skeleton, Kind::Bat, Kind::Spider, Kind::Portrait],
            Zone::Drop | Zone::Crypt => &[Kind::Spider, Kind::Skeleton, Kind::Bat],
            Zone::Lair => &[Kind::Pumpkin, Kind::Bat],
        };
        pool[rng.int(0, pool.len() as i32 - 1) as usize]
    }

    fn spawn_target(&mut self, kind: Kind, seed: u64, _side_sign: f32, near: Option<Vec3>) -> bool {
        let mut rng = Rng::new(seed);
        // Pumpkins/skeletons with no floor nearby turn into ghosts; one in five of
        // those becomes a different enemy instead.
        let mut pick = Rng::new(seed ^ 0x6a09_e667);
        let ghost_sub = if near.is_none() && pick.chance(0.2) { Some(Self::ghost_substitute(self.zone, &mut pick)) } else { None };
        let lane = Self::LANES[self.lane % Self::LANES.len()];
        let aspect = self.screen.x / self.screen.y.max(1.0);
        let focal = 1.0 / 35f32.to_radians().tan();
        // Predicted camera at each sample time (shared by all candidates).
        let sample_frac = [0.0, 0.25, 0.5, 0.75, 1.0];
        let mut best: Option<(f32, Target)> = None;
        let mut valid = 0;
        for attempt in 0..96 {
            if valid >= 8 {
                break;
            }
            // No floor for them to stand on near the track? A ghost takes their place.
            let kind = if attempt >= 32 && matches!(kind, Kind::Pumpkin | Kind::Skeleton) {
                match ghost_sub {
                    Some(sub) if attempt < 56 && !matches!(sub, Kind::Pumpkin | Kind::Skeleton) => sub,
                    Some(_) => Kind::Bat,
                    None => Kind::Ghost,
                }
            } else {
                kind
            };
            // Mostly aim for this spawn's lane; every third try anywhere on screen.
            let lane_x = if attempt % 3 == 2 { rng.range(-0.75, 0.75) } else { lane };
            let Some(t) = self.candidate(kind, &mut rng, lane_x, near) else { continue };
            let (hw, _, _) = Self::extents(kind);
            let mut ok = true;
            let mut xs = 0.0;
            let mut min_sep = f32::MAX;
            for &f in &sample_frac {
                let age = (t.life * f).clamp(0.3, t.life - 0.05);
                let (eye, right, up, vp) = self.view_ahead(age);
                let p = Self::motion(&t, age);
                let Some((n, w)) = Self::on_screen(&vp, p) else {
                    ok = false;
                    break;
                };
                if !self.body_clear(kind, p) || !self.fully_visible(eye, right, up, kind, p) {
                    ok = false;
                    break;
                }
                xs += n.x;
                // Keep clear of every other target, in the world and on screen.
                let r_me = hw * focal / w;
                for o in &self.targets {
                    let (ohw, _, _) = Self::extents(o.kind);
                    let op = Self::motion(o, o.age + age);
                    if op.distance(p) < (hw + ohw) * 1.25 {
                        ok = false;
                        break;
                    }
                    if let Some((on, ow)) = Self::on_screen(&vp, op) {
                        let d = Vec2::new((n.x - on.x) * aspect, n.y - on.y).length();
                        let need = (r_me + ohw * focal / ow) * 1.1;
                        if d < need {
                            ok = false;
                            break;
                        }
                        min_sep = min_sep.min(d - need);
                    }
                }
                if !ok {
                    break;
                }
            }
            if !ok {
                continue;
            }
            valid += 1;
            let x_mean = xs / sample_frac.len() as f32;
            let score = min_sep.min(0.6) - (x_mean - lane).abs();
            if best.as_ref().map_or(true, |b| score > b.0) {
                best = Some((score, t));
            }
        }
        match best {
            Some((_, t)) => {
                self.lane += 1;
                self.spawn_sound(t.kind, t.home);
                self.spawn_kinds[t.kind.index()] += 1;
                self.targets.push(t);
                self.spawn_stats.0 += 1;
                true
            }
            None => {
                self.spawn_stats.1 += 1;
                false
            }
        }
    }

    /// Builds a candidate target. The spot is chosen on screen first (at screen x
    /// `lane_x`, as seen by the camera a little after the target appears) and then
    /// projected into the world, so targets land where players can see them.
    fn candidate(&self, kind: Kind, rng: &mut Rng, lane_x: f32, near: Option<Vec3>) -> Option<Target> {
        let life = match kind {
            Kind::Skeleton => rng.range(3.6, 4.6),
            Kind::Bat => rng.range(3.4, 4.4),
            Kind::Candy => 2.8,
            _ => rng.range(4.4, 5.8),
        };
        let anchor = life * 0.5;
        let (eye, right, up, _) = self.view_ahead(anchor);
        let dir = up.cross(right).normalize_or_zero();
        let aspect = self.screen.x / self.screen.y.max(1.0);
        let focal = 1.0 / 35f32.to_radians().tan();
        let depth = if near.is_some() { rng.range(9.0, 15.0) } else { rng.range(6.5, 15.0) };
        let sx = (lane_x + rng.range(-0.12, 0.12)).clamp(-0.78, 0.78);
        let sy = rng.range(-0.25, 0.5);
        let mut base = eye + dir * depth + right * (sx * depth * aspect / focal) + up * (sy * depth / focal);
        // Track frame alongside the spot (for floor height and "is it over the rails?").
        let cam_d = if self.state == State::Ride && self.ride_t + anchor < RIDE_SECONDS {
            self.track.dist_at_time(self.ride_t + anchor)
        } else {
            self.track.stop_dist
        };
        let f = self.track.frame_at(cam_d + depth);
        let tright = Vec3::new(f.right.x, 0.0, f.right.z).normalize_or_zero();
        let tfwd = Vec3::new(f.fwd.x, 0.0, f.fwd.z).normalize_or_zero();
        let ty = f.pos.y;
        let side = (base - f.pos).dot(tright);
        let over_track = side.abs() < 1.6 && near.is_none();
        let floor = self.floor_below(base + Vec3::Y * 1.0);
        let mut t = Target {
            kind,
            home: base,
            a: base,
            b: base,
            age: 0.0,
            life,
            leave: 0.0,
            phase: rng.range(0.0, 6.28),
            variant: rng.int(0, 2) as usize,
            pos: base,
            scale: 0.0,
            dead: false,
        };
        match kind {
            Kind::Pumpkin | Kind::Skeleton => {
                if near.is_none() {
                    if floor < ty - 1.2 || floor > ty + 1.5 || over_track {
                        return None;
                    }
                    base.y = floor + if kind == Kind::Pumpkin { 0.56 } else { 0.95 };
                }
                t.home = base;
            }
            Kind::Ghost | Kind::Book => {
                if near.is_none() {
                    let lo = ty + if over_track { 2.9 } else { 1.4 };
                    base.y = base.y.clamp(lo, ty + 4.8).max(floor + 1.4);
                }
                t.home = base;
            }
            Kind::Spider => {
                if near.is_none() {
                    let lo = ty + if over_track { 2.9 } else { 1.3 };
                    base.y = base.y.clamp(lo, ty + 4.0);
                }
                t.home = base;
                let ceil = self.ceiling_above(base).unwrap_or(base.y + 6.0);
                if ceil - base.y > 12.0 {
                    return None; // nothing to hang from
                }
                t.a = Vec3::new(base.x, ceil, base.z);
            }
            Kind::Portrait => {
                // Only in the house's rooms, from a stretch of plain wall beside the track.
                let in_house = |p: Vec3| matches!(zone_at(p), Zone::Foyer | Zone::Library | Zone::Dining);
                if near.is_some() || !in_house(base) {
                    return None;
                }
                base.y = base.y.clamp(ty + 1.6, ty + 4.2);
                let s = if side.abs() > 0.3 { side.signum() } else if rng.chance(0.5) { 1.0 } else { -1.0 };
                let (home, n) = self.wall_spot(base, tright * s)?;
                if !in_house(home + n) {
                    return None;
                }
                t.home = home;
                t.a = home + n * 0.7;
                // It ends up hovering just in front of the cart, off to its own side.
                let (eye_end, right_end, up_end, _) = self.view_ahead(life * 0.97);
                let fwd_end = up_end.cross(right_end).normalize_or_zero();
                let off = (t.a - eye_end).dot(right_end).signum();
                t.b = eye_end + fwd_end * 4.5 + right_end * off * 1.1 + up_end * 0.3;
            }
            Kind::Bat | Kind::Candy => {
                if near.is_none() {
                    base.y = base.y.clamp(ty + 1.8, ty + 4.8);
                }
                let span = rng.range(2.0, 3.5);
                let s = if rng.chance(0.5) { 1.0 } else { -1.0 };
                let (r, fw) = if near.is_some() { (right, dir) } else { (tright, tfwd) };
                t.home = base;
                t.a = base + r * (s * span);
                t.b = base - r * (s * span) + fw * rng.range(-1.0, 1.0) + Vec3::Y * rng.range(-0.6, 0.8);
            }
        }
        // Must stay in open air for its whole life (finer steps for flyers).
        // Flying books belong to the library and never leave it.
        let steps = if matches!(kind, Kind::Bat | Kind::Candy) { 10 } else { 6 };
        for i in 0..=steps {
            let p = Self::motion(&t, t.life * i as f32 / steps as f32);
            if !self.body_clear(kind, p) || (kind == Kind::Book && zone_at(p) != Zone::Library) {
                return None;
            }
        }
        t.pos = Self::motion(&t, 0.0);
        Some(t)
    }

    /// Casts sideways from `from` to the first wall and returns the point on its surface
    /// and the wall's outward normal, if a whole painting's worth of wall is there.
    fn wall_spot(&self, from: Vec3, dir: Vec3) -> Option<(Vec3, Vec3)> {
        if self.world.solid_at(from) {
            return None;
        }
        let mut prev = from;
        for i in 1..=160 {
            let p = from + dir * (i as f32 * 0.1);
            if !self.world.solid_at(p) {
                prev = p;
                continue;
            }
            let (c, pc) = (p.floor(), prev.floor());
            let (n, home) = if c.x != pc.x {
                let n = Vec3::new((pc.x - c.x).signum(), 0.0, 0.0);
                (n, Vec3::new(if n.x > 0.0 { c.x + 1.0 } else { c.x }, p.y, p.z))
            } else if c.z != pc.z {
                let n = Vec3::new(0.0, 0.0, (pc.z - c.z).signum());
                (n, Vec3::new(p.x, p.y, if n.z > 0.0 { c.z + 1.0 } else { c.z }))
            } else {
                return None;
            };
            let along = n.cross(Vec3::Y);
            let solid_behind = [(-0.55, -0.7), (0.55, -0.7), (-0.55, 0.7), (0.55, 0.7), (0.0, 0.0)]
                .iter()
                .all(|&(du, dv)| self.world.solid_at(home - n * 0.3 + along * du + Vec3::Y * dv));
            return solid_behind.then_some((home, n));
        }
        None
    }

    /// One bat of the drop swarm: it starts further down the shaft and flies up past
    /// the cart, ending just above and behind where the riders will be. Unlike normal
    /// targets, swarm bats may crowd together; they only have to stay clear of the walls.
    fn spawn_swarm_bat(&mut self, seed: u64) {
        let mut rng = Rng::new(seed);
        let d_now = self.track.dist_at_time(self.ride_t);
        for _ in 0..12 {
            let life = rng.range(1.1, 1.6);
            let fa = self.track.frame_at(d_now + rng.range(9.0, 15.0));
            let d_end = self.track.dist_at_time((self.ride_t + life).min(RIDE_SECONDS));
            let fb = self.track.frame_at(d_end - rng.range(1.5, 3.5));
            // (World up, not the track's: on the steep drop that leans far forward.)
            let a = fa.pos + fa.right * rng.range(-1.2, 1.2) + Vec3::Y * rng.range(1.2, 3.0);
            let b = fb.pos + fb.right * rng.range(-1.4, 1.4) + Vec3::Y * rng.range(2.6, 3.4);
            let t = Target {
                kind: Kind::Bat,
                home: a,
                a,
                b,
                age: 0.0,
                life,
                leave: 0.0,
                phase: rng.range(0.0, 6.28),
                variant: rng.int(0, 2) as usize,
                pos: a,
                scale: 0.0,
                dead: false,
            };
            // A bat's full extents don't fit the steep shaft, so check a tighter core.
            let clear = |p: Vec3| {
                [-0.45, 0.0, 0.45].iter().all(|&dx| {
                    [-0.4, 0.0, 0.4].iter().all(|&dy| [-0.45, 0.0, 0.45].iter().all(|&dz| !self.world.occupied(p + Vec3::new(dx, dy, dz))))
                })
            };
            if (0..=10).all(|i| clear(Self::motion(&t, life * i as f32 / 10.0))) {
                let pan = self.pan_of(a);
                self.sfx(Sfx::Squeak, 0.22, pan, rng.range(0.85, 1.35));
                self.spawn_kinds[Kind::Bat.index()] += 1;
                self.spawn_stats.0 += 1;
                self.targets.push(t);
                return;
            }
        }
        self.spawn_stats.1 += 1;
    }

    fn spawn_sound(&mut self, kind: Kind, at: Vec3) {
        let (sfx, vol) = match kind {
            Kind::Ghost => (Sfx::Ghost, 0.35),
            Kind::Bat => (Sfx::Squeak, 0.3),
            Kind::Skeleton => (Sfx::Rattle, 0.45),
            Kind::Spider => (Sfx::Zip, 0.3),
            Kind::Book => (Sfx::Flutter, 0.4),
            Kind::Pumpkin => (Sfx::Boing, 0.35),
            Kind::Candy => (Sfx::Bonus, 0.25),
            Kind::Portrait => (Sfx::Rattle, 0.3),
        };
        let k = kind.index();
        if self.total_t - self.last_spawn_sound[k] < 1.2 {
            return;
        }
        self.last_spawn_sound[k] = self.total_t;
        let pan = self.pan_of(at);
        self.sfx(sfx, vol, pan, 0.9 + (self.total_t * 7.3).fract() * 0.2);
    }

    pub fn sfx(&mut self, sfx: Sfx, vol: f32, pan: f32, pitch: f32) {
        self.sounds.push(SoundEvent { sfx, vol, pan, pitch });
    }

    fn pan_of(&self, p: Vec3) -> f32 {
        let c = self.viewproj * p.extend(1.0);
        if c.w <= 0.1 {
            0.0
        } else {
            (c.x / c.w).clamp(-1.0, 1.0) * 0.8
        }
    }

    /// Which music should be playing right now.
    pub fn music(&self) -> Option<Music> {
        Some(match self.state {
            State::Title | State::Tutorial => Music::Title,
            State::Ride => Music::Ride,
            State::Boss => Music::Boss,
            State::Victory | State::Outro => Music::Victory,
            State::Results | State::Initials | State::HighScores => Music::Results,
        })
    }

    /// 0..1 loudness of the cart's rattle.
    pub fn cart_noise(&self) -> f32 {
        match self.state {
            State::Ride if !self.paused => {
                let d = self.track.dist_at_time(self.ride_t);
                ((self.track.dist_at_time(self.ride_t + 0.1) - d) * 10.0 / 6.0).clamp(0.0, 1.0)
            }
            State::Outro => 0.6,
            _ => 0.0,
        }
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
                    self.start_tutorial();
                }
            }
            State::Tutorial => {
                // Off we go a moment after the last practice target pops (+ skips the lesson).
                let hits: Vec<_> = (0..4).filter(|&i| self.players[i].joined).map(|i| self.practice[i]).collect();
                let last = hits.iter().copied().try_fold(0.0f32, |m, h| h.map(|h| m.max(h)));
                let skip = inputs.iter().any(|i| i.start) && self.state_t > 0.5;
                if hits.is_empty() || skip || last.map_or(false, |l| self.state_t - l > 0.8) {
                    self.start_ride();
                }
            }
            State::Ride => {
                let before = self.ride_t;
                self.ride_t += dt;
                // Countdown beeps.
                for k in 0..4 {
                    let at = k as f32;
                    if before < at + 0.001 && self.ride_t >= at + 0.001 && before > 0.0 || (k == 0 && before == 0.0) {
                        self.sfx(if k == 3 { Sfx::Go } else { Sfx::Beep }, 0.6, 0.0, 1.0);
                    }
                }
                while self.next_spawn < self.spawns.len() && self.spawns[self.next_spawn].t <= self.ride_t {
                    let s = &self.spawns[self.next_spawn];
                    let (k, seed, side, swarm) = (s.kind, s.seed, s.side, s.swarm);
                    if swarm {
                        self.spawn_swarm_bat(seed);
                    } else if !self.spawn_target(k, seed, side, None) {
                        self.retries.push((self.ride_t + 0.35, k, seed.wrapping_add(1), 3));
                    }
                    self.next_spawn += 1;
                }
                // A spawn that found no good spot tries again a moment later.
                let due: Vec<_> = self.retries.iter().copied().filter(|r| r.0 <= self.ride_t).collect();
                self.retries.retain(|r| r.0 > self.ride_t);
                for (_, k, seed, left) in due {
                    if !self.spawn_target(k, seed, 1.0, None) && left > 1 && self.ride_t < RIDE_SECONDS - 3.0 {
                        self.retries.push((self.ride_t + 0.35, k, seed.wrapping_add(7), left - 1));
                    }
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
                    self.sfx(Sfx::Rumble, 0.8, 0.0, 1.0);
                    self.sfx(Sfx::Laugh, 0.9, 0.0, 1.0);
                }
            }
            State::Boss => self.update_boss(dt),
            State::Victory => {
                // The king vanishes, then the grand doors behind him swing open.
                let before = self.state_t - dt;
                if before < DOOR_START && self.state_t >= DOOR_START {
                    self.sfx(Sfx::Rumble, 0.9, 0.0, 0.8);
                    self.shake = 0.4;
                }
                self.door_open = smooth((self.state_t - DOOR_START) / DOOR_TIME);
                if self.state_t > DOOR_START + DOOR_TIME + 0.3 {
                    self.state = State::Outro;
                    self.state_t = 0.0;
                }
            }
            State::Outro => {
                // The cart rolls out through the open doors, then on to the scores.
                self.door_open = 1.0;
                if self.state_t > OUTRO_TIME {
                    self.state = State::Results;
                    self.state_t = 0.0;
                }
            }
            State::Results => {
                // A "ding" as each row of the scoreboard appears.
                let rows = self.joined_count();
                for rank in 0..rows {
                    let at = 0.4 * (rows - rank) as f32;
                    if self.state_t - dt < at && self.state_t >= at {
                        self.sfx(Sfx::Ding, 0.5, 0.0, 1.0 + 0.12 * (rows - rank) as f32);
                    }
                }
                let go = inputs.iter().any(|i| i.join || i.start || i.fire_pressed);
                if (go && self.state_t > 4.0) || self.state_t > 45.0 || (self.bots > 0 && self.state_t > 6.0) {
                    self.leave_results();
                }
            }
            State::Initials => {
                let entering: Vec<&Initials> = self.initials.iter().flatten().collect();
                let last = entering.iter().try_fold(0.0f32, |m, e| e.done_at.map(|d| m.max(d)));
                if last.map_or(false, |l| self.state_t - l > 0.8) || self.state_t > INITIALS_SECONDS {
                    self.finish_initials();
                }
            }
            State::HighScores => {
                let go = inputs.iter().any(|i| i.join || i.start || i.fire_pressed);
                if (go && self.state_t > 2.0) || self.state_t > 12.0 || (self.bots > 0 && self.state_t > 5.0) {
                    self.go_to_title();
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
            // A painting is already full size on its wall; it only shrinks away at the end.
            t.scale = if t.kind == Kind::Portrait { 1.0 - smooth(t.leave / 0.4) } else { appear };
            let mut pos = Self::motion(t, t.age);
            match t.kind {
                Kind::Pumpkin => pos -= Vec3::Y * (1.0 - appear) * 0.9,
                Kind::Skeleton => pos -= Vec3::Y * (1.0 - appear) * 1.7,
                Kind::Spider => {
                    let top = t.a.y - 0.4;
                    pos.y = top + (pos.y - top) * appear;
                }
                _ => {}
            }
            t.pos = pos;
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
                if to.dot(self.cam_dir) < -1.0 {
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
                self.sfx(Sfx::Join, 0.6, 0.0, 1.0 + slot as f32 * 0.1);
            }
        }
        for inp in inputs {
            let slot = self.players.iter().position(|p| p.source == Some(inp.id));
            match slot {
                Some(s) => {
                    let p = &mut self.players[s];
                    if !p.joined && (inp.join || inp.fire_pressed) {
                        p.joined = true;
                        self.sounds.push(SoundEvent { sfx: Sfx::Join, vol: 0.6, pan: 0.0, pitch: 1.0 + s as f32 * 0.1 });
                    }
                    let p = &mut self.players[s];
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
                            self.sfx(Sfx::Join, 0.6, 0.0, 1.0 + free as f32 * 0.1);
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
        let practice: Vec<Option<Vec2>> = (0..4)
            .map(|i| (self.state == State::Tutorial && self.practice[i].is_none() && self.state_t > 1.5).then(|| self.practice_pos(i)))
            .collect();
        let mut rng = self.rng.clone();
        for (i, p) in self.players.iter_mut().enumerate() {
            let Some(SourceId::Bot(_)) = p.source else { continue };
            p.aim_visible = true;
            p.bot_think -= dt;
            if p.bot_think <= 0.0 {
                p.bot_think = rng.range(0.25, 0.6);
                let mut cands = targets.clone();
                cands.extend(practice[i]);
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
        let (d, speed) = match self.state {
            State::Title | State::Tutorial => (0.0, 0.0),
            State::Ride => {
                let d = self.track.dist_at_time(self.ride_t);
                (d, (self.track.dist_at_time(self.ride_t + 0.1) - d) * 10.0)
            }
            State::Boss | State::Victory => (self.track.stop_dist, 0.0),
            State::Outro => {
                let t = self.state_t;
                (self.track.stop_dist + OUTRO_ACCEL * t * t, 2.0 * OUTRO_ACCEL * t)
            }
            State::Results | State::Initials | State::HighScores => {
                (self.track.stop_dist + OUTRO_ACCEL * OUTRO_TIME * OUTRO_TIME, 0.0)
            }
        };
        let f = self.track.frame_at(d);
        let (mut eye, mut dir, mut up) = self.pose_at(d, speed);
        if self.state == State::Title {
            let s = self.state_t * 0.25;
            dir = Vec3::new(s.sin() * 0.25, 0.08 + (s * 0.7).sin() * 0.03, 1.0).normalize();
            eye = f.pos + Vec3::Y * 1.9;
            up = Vec3::Y;
        }
        let door_centre = Vec3::new(0.0, DOOR_FLOOR + 6.5, DOOR_Z);
        if let (Some(b), true) = (&self.boss, matches!(self.state, State::Boss | State::Victory)) {
            let to = (b.pos - eye).normalize_or_zero();
            let boss_look = dir.lerp(to, 0.3).normalize();
            // Once the king is gone, turn to face the doors.
            let k = if self.state == State::Victory { smooth((self.state_t - 1.0) / 1.5) } else { 0.0 };
            let door_look = (door_centre - eye).normalize_or_zero();
            dir = boss_look.lerp(door_look, k).normalize();
        }
        if self.state == State::Outro {
            let door_look = (door_centre - eye).normalize_or_zero();
            if door_look.dot(dir) > 0.0 && eye.z < door_centre.z - 2.0 {
                dir = door_look.lerp(dir, smooth(self.state_t / 1.5)).normalize();
            }
        }
        self.cam_dir = dir;
        self.cam_up = up;
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
        self.viewproj = self.proj() * view;

        // Zone banners and fog transitions.
        let z = zone_at(f.pos);
        if z != self.zone && self.state == State::Ride {
            self.zone = z;
            self.banner = (zone_name(z).into(), 0.0);
            // (No sound on the drop into the crypt.)
            if z != Zone::Drop {
                self.sfx(Sfx::Whoosh, 0.5, 0.0, 1.0);
            }
        }
        if self.state == State::Title {
            self.zone = Zone::Graveyard;
        }
        let (mut fc, mut fr, lamp) = zone_fog(zone_at(f.pos));
        let mut rate = 1.5;
        // Once the boss doors open, the hall beyond fades away into pure darkness.
        let doors_opening = self.state == State::Victory && self.state_t > DOOR_START;
        if doors_opening || matches!(self.state, State::Outro | State::Results | State::Initials | State::HighScores) {
            fc = [0.0, 0.0, 0.0];
            fr = if self.state == State::Victory { (22.0, 72.0) } else { (8.0, 36.0) };
            rate = if self.state == State::Victory { 1.5 } else { 2.5 };
        }
        let k = 1.0 - (-dt * rate).exp();
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
        let can_shoot = matches!(self.state, State::Tutorial | State::Ride | State::Boss | State::Initials);
        // Typing initials is one letter per pull of the trigger: no auto-fire.
        let can_auto = self.state != State::Initials;
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
            let auto = can_auto && fire_held && p.held > 0.35 && p.cooldown <= 0.0;
            if !(p.joined && p.aim_visible && can_shoot && (fire_pressed || auto) && p.cooldown <= 0.0) {
                continue;
            }
            p.cooldown = if auto { 0.28 } else { 0.11 };
            p.shots += 1;
            p.kick = 1.0;
            let aim = p.aim;
            let pan = (aim.x / self.screen.x * 2.0 - 1.0) * 0.8;
            self.sfx(Sfx::Shot, 0.18, pan, 1.0 + i as f32 * 0.07);
            self.shoot(i, aim);
        }
    }

    fn shoot(&mut self, pi: usize, aim: Vec2) {
        let color = PLAYER_COLORS[pi];
        if self.state == State::Initials {
            self.initials_shot(pi, aim);
            return;
        }
        if self.state == State::Tutorial {
            // Each player can only pop their own practice target.
            if self.practice[pi].is_none() && aim.distance(self.practice_pos(pi)) <= self.practice_radius() {
                self.practice[pi] = Some(self.state_t);
                if let Some(dir) = self.ray_dir(aim) {
                    self.burst(self.cam + dir * 4.0, 30, color, true);
                }
                let pan = (aim.x / self.screen.x * 2.0 - 1.0) * 0.8;
                self.sfx(Sfx::Pop, 0.7, pan, 0.9 + pi as f32 * 0.12);
                self.sfx(Sfx::Poof, 0.5, pan, 1.0);
                if let Some(src) = self.players[pi].source {
                    self.rumble_requests.push((src, 80));
                }
                return;
            }
        }
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
            let pan = (aim.x / self.screen.x * 2.0 - 1.0) * 0.8;
            self.sfx(Sfx::Pop, 0.7, pan, 0.9 + pi as f32 * 0.12);
            // Every normal enemy pops with the same "poof" as the ghost; the golden
            // candy keeps its special bonus jingle.
            match t.kind {
                Kind::Candy => self.sfx(Sfx::Bonus, 0.7, pan, 1.0),
                _ => self.sfx(Sfx::Poof, 0.5, pan, 1.0),
            }
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
            self.sounds.push(SoundEvent { sfx: Sfx::Laugh, vol: 0.7, pan: 0.0, pitch: 0.95 + (b.t * 0.37).fract() * 0.15 });
        }
        // Summon helpers at 75%, 50% and 25% health (extra points for everyone).
        let thresholds = [0.75, 0.5, 0.25];
        let mut summon = false;
        if (b.summons_done as usize) < thresholds.len() && b.hp / b.max_hp < thresholds[b.summons_done as usize] {
            b.summons_done += 1;
            b.taunt = 3.0;
            b.taunt_text = "GET THEM, MY BATS!";
            self.sounds.push(SoundEvent { sfx: Sfx::Laugh, vol: 0.8, pan: 0.0, pitch: 1.1 });
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
        p.counts[BOSS_COUNT] += 1;
        let pts = if final_hit { 1000 } else { 25 };
        self.sounds.push(SoundEvent { sfx: Sfx::Bonk, vol: 0.55, pan: 0.0, pitch: 0.9 + pi as f32 * 0.1 + (1.0 - b.hp / b.max_hp) * 0.3 });
        if final_hit {
            self.sounds.push(SoundEvent { sfx: Sfx::Boom, vol: 1.0, pan: 0.0, pitch: 1.0 });
        }
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

    /// The doors only need drawing once the ride is underground near the lair.
    fn camera_near_lair(&self) -> bool {
        self.cam.z > 150.0 && self.cam.y < -5.0
    }

    pub fn frame(&self) -> FrameOut {
        FrameOut { viewproj: self.viewproj, cam: self.cam, fog: self.fog, fog_range: self.fog_range, lamp: self.lamp }
    }

    pub fn static_mesh(&self) -> Vec<Vertex> {
        self.world.build_mesh(&self.track)
    }

    /// Flat Minecraft-style clouds drifting slowly over the graveyard. Unfogged (they're
    /// far beyond the fog), so they fade into the sky colour by hand with distance.
    fn draw_clouds(&self, out: &mut Vec<Vertex>) {
        let sky = zone_fog(Zone::Graveyard).0;
        let drift = self.total_t * CLOUD_SPEED;
        let span = CLOUD_X1 - CLOUD_X0;
        let uv = tile_uv(T_WHITE);
        for &(x0, z, len) in &self.clouds {
            let x = CLOUD_X0 + (x0 - CLOUD_X0 + drift).rem_euclid(span);
            let lo = Vec3::new(x, CLOUD_Y, z);
            let size = Vec3::new(len * CLOUD_CELL, CLOUD_THICK, CLOUD_CELL);
            let mid = lo + size * 0.5;
            let fade = smooth((mid - self.cam).length() / 420.0) * 0.85;
            for (n, o, r, u) in FACES.iter() {
                // Lit from above by the moon: bright tops, dim sides, dark bellies.
                let k = match n[1] { 1 => 2.6, -1 => 1.5, _ => 2.0 };
                let c = [0, 1, 2].map(|i| sky[i] * k + (sky[i] - sky[i] * k) * fade);
                let corner = |v: Vec3| lo + v * size;
                let (o, r, u) = (Vec3::from(*o), Vec3::from(*r), Vec3::from(*u));
                let pts = [corner(o), corner(o + r), corner(o + r + u), corner(o + u)];
                push_quad(out, pts, uv, [[c[0], c[1], c[2], 2.0]; 4]);
            }
        }
    }

    /// Translucent mist (drawn by the blended mist pipeline), only near where the camera is.
    pub fn draw_mist(&self, out: &mut Vec<Vertex>) {
        let t = self.total_t;
        for p in self.mist.iter().filter(|p| p.zones.contains(&self.zone)) {
            // Each puff wanders slowly around its spot and breathes in and out.
            let mut c = p.pos + Vec3::new((t * 0.11 + p.ph).sin() * p.wander, 0.0, (t * 0.08 + p.ph * 1.7).cos() * p.wander * 0.8);
            let mut a = p.alpha + 0.08 * (t * 0.35 + p.ph * 2.3).sin();
            // Rising wisps float up from their spot and fade out at the top, over and over.
            if p.rise > 0.0 {
                let u = (t * 0.22 + p.ph).fract();
                c.y += u * p.rise;
                a *= (u * std::f32::consts::PI).sin();
            }
            // Upright, turned about Y to face the camera.
            let to = Vec3::new(self.cam.x - c.x, 0.0, self.cam.z - c.z).normalize_or_zero();
            let right = Vec3::Y.cross(to).normalize_or_zero() * p.half.x;
            let up = Vec3::Y * p.half.y;
            let pts = [c - right - up, c + right - up, c + right + up, c - right + up];
            push_quad(out, pts, [0.0, 0.0, 1.0, 1.0], [[p.tint[0], p.tint[1], p.tint[2], a]; 4]);
        }
    }

    pub fn draw_world(&self, out: &mut Vec<Vertex>) {
        let m = &self.models;
        // Flickering flames in the fireplace and the braziers.
        let t = self.total_t;
        for (i, &f) in self.fires.iter().enumerate() {
            if f.distance(self.cam) > 70.0 {
                continue;
            }
            let seed = i as f32 * 1.7;
            // Five flame tongues across the block, each dancing at its own pace.
            for k in 0..5 {
                let ph = seed + k as f32 * 2.3;
                let ox = (k as f32 - 2.0) * 0.17 + (t * 3.1 + ph).sin() * 0.04;
                let oz = ((k * 7 % 5) as f32 - 2.0) * 0.12;
                let h = 0.45 + 0.28 * (t * 7.3 + ph).sin().abs() + 0.12 * (t * 13.7 + ph * 1.3).sin();
                let w = 0.2 + 0.04 * (t * 9.0 + ph).sin();
                let sway = (t * 4.0 + ph).sin() * 0.05;
                let base = f + Vec3::new(ox, 0.0, oz);
                // Orange body, then a yellow tip.
                let body = Mat4::from_translation(base + Vec3::new(sway * 0.5, h * 0.35, 0.0)) * Mat4::from_scale(Vec3::new(w, h * 0.7, w));
                push_box(out, &body, T_WHITE, [1.0, 0.42, 0.06], Vec3::ONE, 1.0);
                let tip = Mat4::from_translation(base + Vec3::new(sway, h * 0.8, 0.0)) * Mat4::from_scale(Vec3::new(w * 0.6, h * 0.4, w * 0.6));
                push_box(out, &tip, T_WHITE, [1.0, 0.86, 0.32], Vec3::ONE, 1.0);
            }
            // A couple of embers drifting up and fading.
            for k in 0..3 {
                let ph = seed * 3.1 + k as f32 * 1.9;
                let u = (t * 0.7 + ph * 0.37).fract();
                let p = f + Vec3::new((ph * 5.0).sin() * 0.3 + (t * 2.0 + ph).sin() * 0.08, 0.5 + u * 1.4, (ph * 3.0).cos() * 0.25);
                let s = 0.07 * (1.0 - u);
                let e = Mat4::from_translation(p) * Mat4::from_scale(Vec3::splat(s));
                push_box(out, &e, T_WHITE, [1.0, 0.6 - 0.3 * u, 0.15], Vec3::ONE, 1.0);
            }
        }

        // Boiling brew: the surface heaves, bubbles swell and pop, and green steam curls up.
        for (i, &c) in self.cauldrons.iter().enumerate() {
            if c.distance(self.cam) > 70.0 {
                continue;
            }
            let seed = i as f32 * 2.9;
            let level = 0.735 + 0.012 * (t * 2.3 + seed).sin();
            let surf = Mat4::from_translation(c + Vec3::Y * level) * Mat4::from_scale(Vec3::new(0.76, 0.04, 0.76));
            push_box(out, &surf, T_SLIME, [1.0; 3], Vec3::ONE, 1.0);
            // Each bubble grows in place for a while, then pops and reappears somewhere else.
            for k in 0..6 {
                let ph = seed + k as f32 * 1.37;
                let rate = 0.8 + 0.5 * (ph * 3.7).sin().abs();
                let cyc = t * rate + ph;
                let (n, u) = (cyc.floor(), cyc.fract());
                let a = (n * 2.39 + ph * 5.1).sin() * 3.14;
                let r = 0.08 + 0.22 * (n * 1.71 + ph).sin().abs();
                let s = 0.05 + 0.13 * u * u;
                let p = c + Vec3::new(a.cos() * r, level + s * 0.3, a.sin() * r);
                let b = Mat4::from_translation(p) * Mat4::from_scale(Vec3::new(s, s * 0.8, s));
                push_box(out, &b, T_SLIME, [0.75 + 0.25 * u; 3], Vec3::ONE, 1.0);
            }
            // Wisps of steam drifting up and fading out.
            for k in 0..4 {
                let ph = seed * 1.3 + k as f32 * 2.2;
                let u = (t * 0.45 + ph * 0.29).fract();
                let p = c + Vec3::new(
                    (ph * 4.0).sin() * 0.2 + (t * 1.5 + ph).sin() * 0.1 * u,
                    0.8 + u * 1.3,
                    (ph * 2.0).cos() * 0.2,
                );
                let s = 0.08 + 0.14 * u;
                let g = 0.75 * (1.0 - u);
                let w = Mat4::from_translation(p) * Mat4::from_scale(Vec3::splat(s));
                push_box(out, &w, T_WHITE, [0.25 * g, g, 0.4 * g], Vec3::ONE, 1.0);
            }
        }

        if self.zone == Zone::Graveyard {
            self.draw_clouds(out);
        }

        // The grand doors behind the Pumpkin King (each leaf swings into the hall).
        if self.camera_near_lair() {
            let swing = self.door_open * 1.75;
            let light = self.world.light_at(Vec3::new(0.0, DOOR_FLOOR + 3.0, DOOR_Z - 1.5)).max(Vec3::splat(0.35));
            let left = Mat4::from_translation(Vec3::new(-DOOR_HALF, DOOR_FLOOR, DOOR_Z))
                * Mat4::from_rotation_y(-swing)
                * Mat4::from_translation(Vec3::new(DOOR_HALF * 0.5, 0.0, 0.0));
            let right = Mat4::from_translation(Vec3::new(DOOR_HALF, DOOR_FLOOR, DOOR_Z))
                * Mat4::from_rotation_y(swing)
                * Mat4::from_translation(Vec3::new(-DOOR_HALF * 0.5, 0.0, 0.0));
            m.door[0].draw(out, &left, light, 0.0, None);
            m.door[1].draw(out, &right, light, 0.0, None);
        }

        for t in &self.targets {
            let s = t.scale.max(0.001);
            let to_cam = self.cam - t.pos;
            let yaw = to_cam.x.atan2(to_cam.z);
            let light = self.world.light_at(t.pos);
            let base = Mat4::from_translation(t.pos) * Mat4::from_rotation_y(yaw) * Mat4::from_scale(Vec3::splat(s * t.kind.size()));
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
                    // Eight legs: each goes up and out to a knee, then bends down.
                    for k in 0..8 {
                        let side = if k < 4 { -1.0 } else { 1.0 };
                        let j = (k % 4) as f32;
                        let wig = (a * 9.0 + j * 1.3 + side).sin() * 0.18;
                        let hip = Vec3::new(side * 0.34, 0.0, 0.3 - j * 0.2);
                        let spread = Mat4::from_rotation_y(side * (0.45 - j * 0.3));
                        let up_ang = 0.6 + wig;
                        let upper = base
                            * Mat4::from_translation(hip)
                            * spread
                            * Mat4::from_rotation_z(side * up_ang)
                            * Mat4::from_translation(Vec3::new(side * 0.18, 0.0, 0.0))
                            * Mat4::from_scale(Vec3::new(0.38, 0.06, 0.06));
                        push_box(out, &upper, T_WHITE, [0.2, 0.15, 0.24], light, 0.25);
                        let knee = Vec3::new(side * 0.36 * up_ang.cos(), 0.36 * up_ang.sin(), 0.0);
                        let lower = base
                            * Mat4::from_translation(hip)
                            * spread
                            * Mat4::from_translation(knee)
                            * Mat4::from_rotation_z(-side * (1.15 - wig))
                            * Mat4::from_translation(Vec3::new(side * 0.24, 0.0, 0.0))
                            * Mat4::from_scale(Vec3::new(0.5, 0.05, 0.05));
                        push_box(out, &lower, T_WHITE, [0.2, 0.15, 0.24], light, 0.25);
                    }
                }
                Kind::Portrait => {
                    // Starts flat on the wall, peels away from it while turning to face
                    // the cart, then tips and sways as it floats closer.
                    let n = (t.a - t.home).normalize_or_zero();
                    let peel = smooth(a / PORTRAIT_PEEL);
                    let pos = if a < PORTRAIT_PEEL { t.home + n * (0.07 + 0.63 * peel) } else { t.pos };
                    let wall_yaw = n.x.atan2(n.z);
                    let turn = smooth((a - 0.3) / 1.2);
                    let d = (yaw - wall_yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                    let sway = (a * 1.9 + t.phase).sin() * 0.14 * turn;
                    let mm = Mat4::from_translation(pos)
                        * Mat4::from_rotation_y(wall_yaw + d * turn)
                        * Mat4::from_rotation_z(sway)
                        * Mat4::from_rotation_x(-0.25 * peel * (1.0 - turn))
                        * Mat4::from_scale(Vec3::splat(s));
                    let face = if t.variant % 2 == 0 { T_PORTRAIT } else { T_PORTRAIT2 };
                    let frame = Mat4::from_scale(Vec3::new(1.2, 1.5, 0.12));
                    push_box_tiles(out, &(mm * frame), [T_GOLD, T_GOLD, T_GOLD, T_GOLD, face, T_DARKPLANK], [1.0; 3], light, hl);
                    // The painted eyes glow brighter as it closes in.
                    let glare = 0.6 + 0.4 * (a * 7.0 + t.phase).sin().abs();
                    for ex in [-0.0375, 0.1125] {
                        let e = mm * Mat4::from_translation(Vec3::new(ex, 0.14, 0.065)) * Mat4::from_scale(Vec3::new(0.085, 0.085, 0.02));
                        push_box(out, &e, T_WHITE, [1.0, 0.25 * glare, 0.15 * glare], light, 1.0);
                    }
                }
                Kind::Book => {
                    let open = 0.35 + 0.9 * (0.5 + 0.5 * (a * 10.0 + t.phase).sin());
                    let flight = base * Mat4::from_rotation_x(0.4);
                    let half = &m.book_half[t.variant % 3];
                    // Each half hinges on the spine at the page surface, so the pages of one
                    // half can never poke through the cover of the other.
                    let hinge = Mat4::from_translation(Vec3::new(0.25, -0.2, 0.0));
                    let r = flight * Mat4::from_rotation_z(open) * hinge;
                    half.draw(out, &r, light, hl, None);
                    let l = flight * Mat4::from_rotation_y(std::f32::consts::PI) * Mat4::from_rotation_z(open) * hinge;
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
                // Tip the head down toward the riders so his face looks at them.
                let horiz = Vec2::new(to_cam.x, to_cam.z).length().max(0.1);
                let pitch = ((head.y - self.cam.y) / horiz).atan().clamp(0.0, 0.6);
                let hm = Mat4::from_translation(head)
                    * Mat4::from_rotation_y(yaw)
                    * Mat4::from_rotation_x(pitch)
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
            State::Tutorial => self.draw_tutorial(ui),
            State::Results => self.draw_results(ui),
            State::Initials => self.draw_initials(ui),
            State::HighScores => self.draw_high_scores(ui),
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
                    // Name on top, bar underneath with a clear gap.
                    let name_y = 4.0 * unit;
                    let y = name_y + 8.0 * 1.4 * unit + 3.0 * unit;
                    ui.text("THE PUMPKIN KING", w * 0.5, name_y, 1.4, orange, Align::Center);
                    ui.panel(x, y, bw, 7.0 * unit, [0.1, 0.03, 0.12, 0.85], [0.0, 0.0, 0.0, 0.9]);
                    let f = (b.hp / b.max_hp).max(0.0);
                    ui.rect(x + unit * 1.5, y + unit * 1.5, (bw - unit * 3.0) * f, 4.0 * unit, [1.0, 0.45 + 0.3 * f, 0.05, 1.0]);
                    if b.taunt > 3.5 && b.t > 3.0 {
                        if let Some((sp, _)) = self.project(b.pos + Vec3::Y * 5.5) {
                            let ty = sp.y.max(y + 13.0 * unit);
                            ui.text_fit(b.taunt_text, sp.x.clamp(w * 0.3, w * 0.7), ty, 1.6, w * 0.6, [1.0, 0.9, 0.4, 1.0], Align::Center);
                        }
                    }
                }
            }
            self.draw_scores(ui);
        }

        // Fade to black at the end of the ride.
        if self.state == State::Outro {
            let a = smooth((self.state_t - (OUTRO_TIME - 1.6)) / 1.4);
            ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, a]);
        }

        // Crosshairs on top of everything.
        if !matches!(self.state, State::Outro | State::Results | State::HighScores) {
            for (i, p) in self.players.iter().enumerate() {
                if !p.aim_visible || (!p.joined && self.state != State::Title) {
                    continue;
                }
                // Only riders still entering initials aim at the letter grid.
                if self.state == State::Initials && !self.initials[i].as_ref().map_or(false, |e| e.done_at.is_none()) {
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
        let ph = (18.0 * unit).min(h * 0.09);
        let pad = unit * 3.0;
        for (i, p) in self.players.iter().enumerate() {
            let x = i as f32 * pw + pad;
            let bw = pw - pad * 2.0;
            let y = h - ph - pad;
            let c = PLAYER_COLORS[i];
            let inner = bw - unit * 8.0;
            if p.joined {
                ui.panel(x, y, bw, ph, [c[0] * 0.25, c[1] * 0.25, c[2] * 0.25, 0.8], [c[0], c[1], c[2], 1.0]);
                let label = format!("P{}", i + 1);
                let lw = ui.text_width(&label, 1.0).min(inner * 0.3);
                ui.text_fit(&label, x + unit * 4.0, y + ph * 0.5 - 4.0 * unit, 1.0, inner * 0.3, [c[0], c[1], c[2], 1.0], Align::Left);
                let score = format!("{}", p.shown.round() as u32);
                let sw = inner - lw - unit * 4.0;
                let sc = (1.6f32).min(sw / ui.text_width(&score, 1.0)).min(ph / (8.0 * unit) * 0.75);
                ui.text(&score, x + bw - unit * 4.0, y + ph * 0.5 - 4.0 * sc * unit, sc, [1.0, 1.0, 1.0, 1.0], Align::Right);
            } else {
                ui.panel(x, y, bw, ph, [0.0, 0.0, 0.0, 0.35], [c[0] * 0.5, c[1] * 0.5, c[2] * 0.5, 0.6]);
                let txt = if self.debug { "KEY 1-4" } else { "PRESS A" };
                ui.text_fit(txt, x + bw * 0.5, y + ph * 0.5 - 3.2 * unit, 0.8, inner, [c[0], c[1], c[2], 0.7], Align::Center);
            }
        }
    }

    fn draw_title(&self, ui: &mut Ui) {
        let (w, h, unit) = (ui.w, ui.h, ui.unit);
        let t = self.state_t;
        let bob = (t * 2.0).sin() * 2.0 * unit;
        ui.rect(0.0, h * 0.1, w, 52.0 * unit, [0.0, 0.0, 0.0, 0.35]);
        ui.text_fit("HAUNTED HOLLOW", w * 0.5, h * 0.12 + bob, 5.0, w * 0.92, [1.0, 0.55, 0.08, 1.0], Align::Center);
        ui.text_fit("THE SPOOKY COASTER RIDE", w * 0.5, h * 0.12 + 46.0 * unit, 1.5, w * 0.9, [0.75, 0.55, 1.0, 1.0], Align::Center);
        if let Some(best) = self.high.entries.first() {
            let line = format!("TOP SCORE  {}  {}", best.initials, best.score);
            ui.text_fit(&line, w * 0.5, h * 0.12 + 62.0 * unit, 1.1, w * 0.9, [1.0, 0.82, 0.2, 1.0], Align::Center);
        }

        // Four player cards in a row; every line is fitted to its card.
        let pw = w / 4.0;
        let card_h = 40.0 * unit;
        let y = h * 0.56;
        for (i, p) in self.players.iter().enumerate() {
            let c = PLAYER_COLORS[i];
            let x = i as f32 * pw + pw * 0.5;
            let bw = pw - 8.0 * unit;
            let inner = bw - 8.0 * unit;
            let a = if p.joined { 1.0 } else { 0.55 };
            let pulse = if p.joined { 1.0 + 0.04 * (t * 6.0).sin() } else { 1.0 };
            ui.panel(x - bw * 0.5, y, bw, card_h, [c[0] * 0.2, c[1] * 0.2, c[2] * 0.2, 0.85 * a], [c[0], c[1], c[2], a]);
            ui.text_fit(&format!("PLAYER {}", i + 1), x, y + 5.0 * unit, 1.1, inner, [c[0], c[1], c[2], a], Align::Center);
            ui.text_fit(PLAYER_NAMES[i], x, y + 16.0 * unit, 0.8, inner, [c[0], c[1], c[2], a], Align::Center);
            if p.joined {
                ui.text_fit("READY!", x, y + 27.0 * unit, 1.1 * pulse, inner, [1.0, 1.0, 1.0, 1.0], Align::Center);
            } else if (t * 1.5) as i32 % 2 == 0 {
                let key = if self.debug { "KEY 1-4" } else { "PRESS A" };
                ui.text_fit(key, x, y + 28.0 * unit, 0.9, inner, [1.0, 1.0, 1.0, 0.9], Align::Center);
            }
        }
        let msg = if self.joined_count() == 0 {
            if self.debug { "CLICK OR PRESS A TO JOIN".to_string() } else { "PRESS A ON YOUR WII REMOTE TO JOIN".to_string() }
        } else {
            let left = 15.0 - self.title_join_t.unwrap_or(0.0);
            let key = if self.debug { "PRESS + OR ENTER TO START" } else { "PRESS + TO START" };
            format!("{key}  ({:.0})", left.max(0.0).ceil())
        };
        ui.text_fit(&msg, w * 0.5, h * 0.84, 1.3, w * 0.9, [1.0, 0.9, 0.5, 1.0], Align::Center);
    }

    fn draw_tutorial(&self, ui: &mut Ui) {
        let (w, h, unit) = (ui.w, ui.h, ui.unit);
        let t = self.state_t;
        let white = [1.0, 1.0, 1.0, 1.0];
        let orange = [1.0, 0.6, 0.1, 1.0];
        // Layout first, so the backing panel can be sized to fit everything on it.
        let u = (unit * 1.5).min(w / 170.0); // the remote's own pixel size
        let (rw, rh) = (100.0 * u, 16.0 * u);
        let rx = w * 0.5 - rw * 0.6;
        let top = h * 0.05;
        let ry = top + 28.0 * unit + 16.0 * u; // room for the heading and the A badge above the remote
        let hint_y = ry + rh + 17.0 * u;
        let panel_h = hint_y + 26.0 * unit - top;
        ui.rect(0.0, top, w, panel_h, [0.02, 0.0, 0.04, 0.85]);
        ui.rect(0.0, top, w, unit, [1.0, 0.6, 0.1, 0.6]);
        ui.rect(0.0, top + panel_h - unit, w, unit, [1.0, 0.6, 0.1, 0.6]);
        let head = if self.debug { "CLICK TO SHOOT!" } else { "PRESS A OR PULL B TO SHOOT!" };
        ui.text_fit(head, w * 0.5, top + 7.0 * unit, 2.2, w * 0.9, orange, Align::Center);

        // A Wii Remote side on, pointing right at the screen, pressing A and B once a second.
        let since = t.fract();
        let squeeze = since < 0.18;
        let glow = 0.75 + 0.25 * (t * 8.0).sin();
        let lit = [1.0, 0.55 * glow, 0.1, 1.0];
        let outline = [0.22, 0.22, 0.27, 1.0];
        let shade = |k: f32| [0.97 * k, 0.97 * k, 0.99 * k, 1.0];
        // Wrist strap hanging from the back end (behind the body).
        for k in 0..16 {
            let a = k as f32 / 15.0 * std::f32::consts::PI;
            let (px, py) = (rx - 3.0 * u - a.sin() * 5.0 * u, ry + rh * 0.6 + (1.0 - a.cos()) * 6.0 * u);
            ui.rect(px, py, 1.4 * u, 1.4 * u, [0.55, 0.6, 0.7, 1.0]);
        }
        ui.round_rect(rx - 4.0 * u, ry + rh * 0.45, 4.0 * u, 3.0 * u, u, [0.35, 0.37, 0.45, 1.0]);
        // Top buttons stand proud of the body, so draw them first: 1 and 2 at the back,
        // then - HOME +, the big A, and the D-pad near the front.
        for k in 0..2 {
            let bx = rx + (16.0 + k as f32 * 8.0) * u;
            ui.round_rect(bx - u * 0.5, ry - 2.5 * u, 5.0 * u, 3.5 * u, u, outline);
            ui.rect(bx, ry - 2.0 * u, 4.0 * u, 2.0 * u, shade(0.82));
        }
        for k in 0..3 {
            let bx = rx + (44.0 + k as f32 * 6.0) * u;
            let c = if k == 1 { [0.35, 0.6, 1.0, 1.0] } else { shade(0.8) };
            ui.round_rect(bx - u * 0.5, ry - 1.8 * u, 4.0 * u, 2.8 * u, u, outline);
            ui.rect(bx, ry - 1.3 * u, 3.0 * u, 1.3 * u, c);
        }
        // The A button glows like the trigger and sinks in on each press.
        let ax = rx + 66.0 * u;
        let sink = if squeeze { 1.5 * u } else { 0.0 };
        ui.round_rect(ax - 0.5 * u, ry - 3.5 * u + sink, 9.0 * u, 4.5 * u - sink, 2.0 * u, outline);
        ui.round_rect(ax + 0.5 * u, ry - 2.7 * u + sink, 7.0 * u, 3.0 * u - sink, 1.5 * u, lit);
        ui.rect(ax + 1.5 * u, ry - 2.3 * u + sink, 3.0 * u, 0.8 * u, [1.0, 0.85, 0.5, 1.0]);
        let (alx, aly) = (ax + 4.0 * u, ry - 3.5 * u - 12.0 * u);
        ui.round_rect(alx - 4.5 * u, aly - 0.5 * u, 9.0 * u, 9.0 * u, 2.5 * u, [0.0, 0.0, 0.0, 1.0]);
        ui.round_rect(alx - 4.0 * u, aly, 8.0 * u, 8.0 * u, 2.0 * u, [1.0, 0.6 * glow + 0.1, 0.1, 1.0]);
        let abs = 8.0 * u / (8.0 * unit) * 0.8;
        ui.text("A", alx, aly + 4.0 * u - 4.0 * abs * unit, abs, white, Align::Center);
        ui.rect(alx - 0.5 * u, aly + 8.5 * u, u, 2.5 * u, orange);
        let dx = rx + 80.0 * u;
        ui.round_rect(dx - 0.5 * u, ry - 3.0 * u, 10.0 * u, 4.0 * u, u, outline);
        ui.rect(dx + 0.5 * u, ry - 2.2 * u, 8.0 * u, 2.4 * u, [0.32, 0.32, 0.38, 1.0]);
        ui.rect(dx + 3.0 * u, ry - 3.6 * u, 4.0 * u, 1.5 * u, outline);
        ui.rect(dx + 3.5 * u, ry - 3.0 * u, 3.0 * u, 1.0 * u, [0.4, 0.4, 0.46, 1.0]);
        // Body: rounded, lit from above, with a black IR cap on the front.
        ui.round_rect(rx - u, ry - u, rw + 2.0 * u, rh + 2.0 * u, 4.5 * u, outline);
        ui.round_rect(rx, ry, rw, rh, 3.5 * u, shade(0.9));
        ui.rect(rx + 3.0 * u, ry + 1.0 * u, rw - 12.0 * u, 3.0 * u, shade(1.0));
        ui.rect(rx + 2.0 * u, ry + rh - 6.0 * u, rw - 9.0 * u, 3.0 * u, shade(0.8));
        ui.rect(rx + 3.5 * u, ry + rh - 3.0 * u, rw - 11.0 * u, 2.0 * u, shade(0.68));
        ui.round_rect(rx + rw - 8.0 * u, ry, 8.0 * u, rh, 3.5 * u, [0.07, 0.05, 0.1, 1.0]);
        ui.rect(rx + rw - 8.0 * u, ry, 3.0 * u, rh, [0.07, 0.05, 0.1, 1.0]);
        ui.rect(rx + rw - 5.5 * u, ry + 2.5 * u, 1.5 * u, rh * 0.35, [0.35, 0.28, 0.5, 1.0]);
        // Expansion port at the back and the four player lights, the first one lit.
        ui.rect(rx, ry + 5.0 * u, 1.5 * u, rh - 10.0 * u, [0.3, 0.3, 0.35, 1.0]);
        for k in 0..4 {
            let lit = k == 0 && (t * 2.0).fract() < 0.7;
            let c = if lit { [0.4, 0.8, 1.0, 1.0] } else { [0.62, 0.66, 0.75, 1.0] };
            ui.rect(rx + (5.0 + k as f32 * 3.5) * u, ry + 6.5 * u, 2.0 * u, 1.2 * u, c);
        }

        // The B trigger sits in a recess underneath, below the D-pad, and glows.
        let drop = if squeeze { 1.5 } else { 5.0 } * u;
        let bx = rx + 78.0 * u;
        let by = ry + rh;
        ui.rect(bx - 7.0 * u, by - 0.5 * u, 14.0 * u, 1.5 * u, [0.3, 0.3, 0.36, 1.0]);
        ui.round_rect(bx - 5.0 * u, by - 0.5 * u, 10.0 * u, drop + 1.5 * u, 1.5 * u, outline);
        ui.round_rect(bx - 4.0 * u, by, 8.0 * u, drop, 1.2 * u, [1.0, 0.55 * glow, 0.1, 1.0]);
        ui.rect(bx - 3.0 * u, by + 0.5 * u, 2.0 * u, (drop - 1.5 * u).max(0.5 * u), [1.0, 0.85, 0.5, 1.0]);
        // "B" badge pointing at the trigger.
        let (lx, ly) = (bx + 8.0 * u, by + 2.0 * u);
        ui.rect(bx + 4.5 * u, ly + 3.0 * u, 3.5 * u, u, orange);
        ui.round_rect(lx - 0.5 * u, ly - 0.5 * u, 9.0 * u, 9.0 * u, 2.5 * u, [0.0, 0.0, 0.0, 1.0]);
        ui.round_rect(lx, ly, 8.0 * u, 8.0 * u, 2.0 * u, [1.0, 0.6 * glow + 0.1, 0.1, 1.0]);
        let bs = 8.0 * u / (8.0 * unit) * 0.8;
        ui.text("B", lx + 4.0 * u, ly + 4.0 * u - 4.0 * bs * unit, bs, white, Align::Center);
        // The shot: a dotted beam out of the front and a little pop on every squeeze.
        let bx0 = rx + rw + 4.0 * u;
        let beam_y = ry + rh * 0.5;
        for k in 0..5 {
            let x = bx0 + k as f32 * 6.0 * u;
            let a = if since < 0.4 { 1.0 - since / 0.4 } else { 0.25 };
            ui.rect(x, beam_y - u, 3.0 * u, 2.0 * u, [1.0, 0.9, 0.5, a]);
        }
        let star = if since < 0.3 { 1.0 + since * 3.0 } else { 0.0 };
        if star > 0.0 {
            ui.sprite(T_STAR, bx0 + 36.0 * u, beam_y, 12.0 * u * star, [1.0, 0.85, 0.3, 1.0 - since / 0.3]);
        }
        let hint = if self.debug { "AIM WITH THE MOUSE - HOLD TO RAPID FIRE" } else { "POINT AT THE SCREEN - HOLD A OR B TO RAPID FIRE" };
        ui.text_fit(hint, w * 0.5, hint_y, 1.0, w * 0.9, [0.9, 0.9, 1.0, 1.0], Align::Center);
        ui.text_fit("EVERYONE SHOOT YOUR TARGET TO START THE RIDE!", w * 0.5, hint_y + 11.0 * unit, 1.2, w * 0.9, [1.0, 0.9, 0.5, 1.0], Align::Center);

        // One practice target per player, in their colour.
        let r = self.practice_radius();
        for (i, p) in self.players.iter().enumerate() {
            if !p.joined {
                continue;
            }
            let c = PLAYER_COLORS[i];
            let pos = self.practice_pos(i);
            match self.practice[i] {
                None => {
                    let bob = (t * 3.0 + i as f32).sin() * 2.0 * unit;
                    let pulse = 1.0 + 0.06 * (t * 5.0 + i as f32).sin();
                    let (x, y) = (pos.x, pos.y + bob);
                    for (k, (s, col)) in [(2.0, [c[0], c[1], c[2], 1.0]), (1.4, white), (0.8, [c[0], c[1], c[2], 1.0])].iter().enumerate() {
                        let sz = r * s * pulse;
                        if k == 0 {
                            ui.sprite(T_RING, x, y, sz + 2.0 * unit, [0.0, 0.0, 0.0, 0.7]);
                        }
                        ui.sprite(T_RING, x, y, sz, *col);
                    }
                    ui.rect(x - 2.0 * unit, y - 2.0 * unit, 4.0 * unit, 4.0 * unit, white);
                    ui.text_fit(&format!("P{}", i + 1), x, y + r + 4.0 * unit, 1.0, w * 0.22, [c[0], c[1], c[2], 1.0], Align::Center);
                }
                Some(at) => {
                    let k = smooth((t - at) / 0.25);
                    let sc = 1.6 + (1.0 - k) * 0.8;
                    ui.text_fit("READY!", pos.x, pos.y - 4.0 * sc * unit, sc, w * 0.22, [c[0], c[1], c[2], k], Align::Center);
                }
            }
        }
        if t > 4.0 {
            let key = if self.debug { "PRESS + OR ENTER TO SKIP" } else { "PRESS + TO SKIP" };
            ui.text_fit(key, w * 0.5, h * 0.9, 0.8, w * 0.9, [0.7, 0.7, 0.8, 0.8], Align::Center);
        }
    }

    fn draw_results(&self, ui: &mut Ui) {
        let (w, h, unit) = (ui.w, ui.h, ui.unit);
        ui.rect(0.0, 0.0, w, h, [0.04, 0.02, 0.07, 0.94]);
        ui.text_fit("WHAT A SPOOKTACULAR RIDE!", w * 0.5, h * 0.07, 2.6, w * 0.92, [1.0, 0.55, 0.08, 1.0], Align::Center);
        let mut order: Vec<usize> = (0..4).filter(|&i| self.players[i].joined).collect();
        order.sort_by(|a, b| self.players[*b].score.cmp(&self.players[*a].score));
        let medal = [("1ST", [1.0, 0.82, 0.2]), ("2ND", [0.8, 0.82, 0.9]), ("3RD", [0.85, 0.55, 0.3]), ("4TH", [0.6, 0.6, 0.7])];
        let top = h * 0.2;
        let avail = h * 0.66;
        let gap = 5.0 * unit;
        let row_h = ((avail - gap * 3.0) / 4.0).min(36.0 * unit);
        let rw = (w * 0.86).min(row_h * 9.0);
        let x = (w - rw) * 0.5;
        // Column layout: medal | name + award | score + stats.
        let pad = row_h * 0.18;
        let col_medal = rw * 0.17;
        let col_name = rw * 0.43;
        let col_score = rw - col_medal - col_name - pad * 3.0;
        let line = row_h / (8.0 * unit); // font scale that fills the row height
        let made_it = self.high_score_riders();
        for (rank, &i) in order.iter().enumerate() {
            let appear = smooth((self.state_t - 0.4 * (order.len() - rank) as f32) / 0.4);
            if appear <= 0.0 {
                continue;
            }
            let p = &self.players[i];
            let c = PLAYER_COLORS[i];
            let y = top + rank as f32 * (row_h + gap);
            ui.panel(x, y, rw, row_h, [c[0] * 0.2, c[1] * 0.2, c[2] * 0.2, 0.9 * appear], [c[0], c[1], c[2], appear]);
            let (mt, mc) = medal[rank];
            let ms = (line * 0.42).min(col_medal / ui.text_width(mt, 1.0));
            ui.text(mt, x + pad, y + (row_h - 8.0 * ms * unit) * 0.5, ms, [mc[0], mc[1], mc[2], appear], Align::Left);
            let nx = x + pad * 2.0 + col_medal;
            let name = format!("PLAYER {}", i + 1);
            let ns = (line * 0.3).min(col_name / ui.text_width(&name, 1.0));
            ui.text(&name, nx, y + row_h * 0.16, ns, [c[0], c[1], c[2], appear], Align::Left);
            let award = self.award(i);
            let asc = (line * 0.22).min(col_name / ui.text_width(award, 1.0));
            ui.text(award, nx, y + row_h * 0.6, asc, [1.0, 1.0, 1.0, appear], Align::Left);
            let rx = x + rw - pad;
            let score = format!("{}", p.score);
            let ss = (line * 0.36).min(col_score / ui.text_width(&score, 1.0));
            ui.text(&score, rx, y + row_h * 0.12, ss, [1.0, 1.0, 1.0, appear], Align::Right);
            let acc = if p.shots > 0 { p.hits * 100 / p.shots } else { 0 };
            let stats = format!("{} POPS {}% AIM", p.hits, acc);
            let sts = (line * 0.18).min(col_score / ui.text_width(&stats, 1.0));
            ui.text(&stats, rx, y + row_h * 0.68, sts, [0.85, 0.85, 0.9, appear], Align::Right);
            // A flashing tag across the top edge of the card for a new high score.
            if made_it.contains(&i) {
                let tag = "NEW HIGH SCORE!";
                let ts = (line * 0.16).max(0.6);
                let (tw, th) = (ui.text_width(tag, ts) + 8.0 * unit, 8.0 * ts * unit + 4.0 * unit);
                let tx = x + rw * 0.5 - tw * 0.5;
                let ty = y - th * 0.5;
                let k = 0.75 + 0.25 * (self.state_t * 6.0).sin();
                ui.panel(tx, ty, tw, th, [1.0 * k, 0.5 * k, 0.05, appear], [1.0, 0.9, 0.4, appear]);
                ui.text(tag, x + rw * 0.5, ty + 2.0 * unit, ts, [1.0, 1.0, 1.0, appear], Align::Center);
            }
        }
        if self.state_t > 4.0 && (self.state_t * 2.0) as i32 % 2 == 0 {
            let msg = match (made_it.is_empty(), self.debug) {
                (false, true) => "CLICK TO ENTER YOUR HIGH SCORE!",
                (false, false) => "PRESS A TO ENTER YOUR HIGH SCORE!",
                (true, true) => "CLICK TO RIDE AGAIN!",
                (true, false) => "PRESS A TO RIDE AGAIN!",
            };
            ui.text_fit(msg, w * 0.5, h * 0.9, 1.5, w * 0.9, [1.0, 0.9, 0.5, 1.0], Align::Center);
        }
    }

    fn draw_initials(&self, ui: &mut Ui) {
        let (w, h, unit) = (ui.w, ui.h, ui.unit);
        let t = self.state_t;
        ui.rect(0.0, 0.0, w, h, [0.04, 0.02, 0.07, 0.94]);
        let pulse = 1.0 + 0.04 * (t * 5.0).sin();
        ui.text_fit("NEW HIGH SCORE!", w * 0.5, h * 0.04, 2.6 * pulse, w * 0.9, [1.0, 0.55, 0.08, 1.0], Align::Center);
        let how = if self.debug { "CLICK THE LETTERS OF YOUR NAME, THEN END" } else { "SHOOT THE LETTERS OF YOUR NAME, THEN END" };
        ui.text_fit(how, w * 0.5, h * 0.04 + 26.0 * unit, 1.0, w * 0.9, [0.85, 0.85, 0.95, 1.0], Align::Center);
        let left = (INITIALS_SECONDS - t).max(0.0).ceil();
        if left <= 15.0 {
            ui.text(&format!("{left:.0}"), w - 6.0 * unit, h * 0.04, 1.6, [1.0, 0.4, 0.3, 1.0], Align::Right);
        }

        // The letter grid. A key lights up in the colour of whoever is aiming at it.
        let entering = |i: usize| self.initials[i].as_ref().map_or(false, |e| e.done_at.is_none());
        for (k, &(x, y, cw, ch)) in self.key_rects().iter().enumerate() {
            let over = (0..4).find(|&i| {
                let a = self.players[i].aim;
                entering(i) && self.players[i].aim_visible && a.x >= x && a.x < x + cw && a.y >= y && a.y < y + ch
            });
            let (ft, fp) = self.key_flash[k];
            let flash = (1.0 - (self.total_t - ft) / 0.3).max(0.0);
            let fc = PLAYER_COLORS[fp];
            let mut fill = [0.12 + fc[0] * 0.6 * flash, 0.06 + fc[1] * 0.6 * flash, 0.18 + fc[2] * 0.6 * flash, 0.95];
            let border = match over {
                Some(i) => {
                    let c = PLAYER_COLORS[i];
                    fill = [fill[0] + c[0] * 0.15, fill[1] + c[1] * 0.15, fill[2] + c[2] * 0.15, 0.95];
                    [c[0], c[1], c[2], 1.0]
                }
                None => [0.5, 0.32, 0.7, 1.0],
            };
            ui.panel(x, y, cw, ch, fill, border);
            ui.panel(x + unit, y + unit, cw - 2.0 * unit, ch - 2.0 * unit, fill, border);
            let label = KEYS[k];
            let col = match k {
                KEY_DEL => [1.0, 0.45, 0.4, 1.0],
                KEY_END => [0.45, 1.0, 0.5, 1.0],
                _ => [1.0, 1.0, 1.0, 1.0],
            };
            let sc = (ch * 0.5 / (8.0 * unit)).min(cw * 0.8 / ui.text_width(label, 1.0));
            ui.text(label, x + cw * 0.5, y + (ch - 8.0 * sc * unit) * 0.5, sc, col, Align::Center);
        }

        // Each rider's initials so far, in their score-card column.
        let pw = w / 4.0;
        let pad = 3.0 * unit;
        let ph = 42.0 * unit;
        for (i, e) in self.initials.iter().enumerate() {
            let Some(e) = e else { continue };
            let c = PLAYER_COLORS[i];
            let (x, bw, y) = (i as f32 * pw + pad, pw - pad * 2.0, h - ph - pad);
            ui.panel(x, y, bw, ph, [c[0] * 0.25, c[1] * 0.25, c[2] * 0.25, 0.9], [c[0], c[1], c[2], 1.0]);
            let head = format!("P{}  {}", i + 1, self.players[i].score);
            ui.text_fit(&head, x + bw * 0.5, y + 4.0 * unit, 1.0, bw - 6.0 * unit, [c[0], c[1], c[2], 1.0], Align::Center);
            let (sw, sg) = ((14.0 * unit).min(bw / 4.0), 3.0 * unit);
            let sx = x + (bw - (sw * 3.0 + sg * 2.0)) * 0.5;
            let sy = y + 15.0 * unit;
            for k in 0..MAX_INITIALS {
                let bx = sx + k as f32 * (sw + sg);
                ui.rect(bx, sy, sw, sw, [0.0, 0.0, 0.0, 0.5]);
                if let Some(ch) = e.letters.get(k..k + 1) {
                    let sc = sw * 0.7 / (8.0 * unit);
                    ui.text(ch, bx + sw * 0.5, sy + (sw - 8.0 * sc * unit) * 0.5, sc, [1.0, 1.0, 1.0, 1.0], Align::Center);
                } else if k == e.letters.len() && e.done_at.is_none() && (t * 3.0) as i32 % 2 == 0 {
                    ui.rect(bx + 2.0 * unit, sy + sw - 3.0 * unit, sw - 4.0 * unit, 2.0 * unit, [c[0], c[1], c[2], 1.0]);
                }
            }
            if e.done_at.is_some() {
                ui.text_fit("DONE!", x + bw * 0.5, y + ph - 10.0 * unit, 1.0, bw - 6.0 * unit, [1.0, 1.0, 1.0, 1.0], Align::Center);
            }
        }
    }

    fn draw_high_scores(&self, ui: &mut Ui) {
        let (w, h, unit) = (ui.w, ui.h, ui.unit);
        let t = self.state_t;
        ui.rect(0.0, 0.0, w, h, [0.04, 0.02, 0.07, 0.94]);
        ui.text_fit("HIGH SCORES", w * 0.5, h * 0.05, 2.8, w * 0.9, [1.0, 0.55, 0.08, 1.0], Align::Center);
        let top = h * 0.17;
        let row_h = h * 0.68 / crate::highscores::TABLE_SIZE as f32;
        let sc = (row_h * 0.62 / (8.0 * unit)).min(w * 0.6 / ui.text_width("10.  AAA  0000000", 1.0));
        let (x_rank, x_name, x_score) = (w * 0.5 - 15.0 * 8.0 * sc * unit * 0.5, w * 0.5 - 5.0 * 8.0 * sc * unit * 0.5, w * 0.5 + 15.0 * 8.0 * sc * unit * 0.5);
        for (rank, e) in self.high.entries.iter().enumerate() {
            let a = smooth((t - 0.08 * rank as f32) / 0.3);
            if a <= 0.0 {
                continue;
            }
            let y = top + rank as f32 * row_h + (row_h - 8.0 * sc * unit) * 0.5;
            let new = self.high_new.iter().find(|n| n.0 == rank).map(|n| n.1);
            let col = match new {
                Some(i) => {
                    let c = PLAYER_COLORS[i];
                    let k = 0.75 + 0.25 * (t * 6.0).sin();
                    [c[0] * k + (1.0 - k), c[1] * k + (1.0 - k), c[2] * k + (1.0 - k), a]
                }
                None if rank == 0 => [1.0, 0.82, 0.2, a],
                None => [0.9, 0.9, 0.95, a],
            };
            if let Some(i) = new {
                let c = PLAYER_COLORS[i];
                ui.rect(w * 0.12, top + rank as f32 * row_h + row_h * 0.08, w * 0.76, row_h * 0.84, [c[0] * 0.25, c[1] * 0.25, c[2] * 0.25, 0.7 * a]);
                ui.text(&format!("P{}", i + 1), w * 0.14, y, sc * 0.8, [c[0], c[1], c[2], a], Align::Left);
            }
            ui.text(&format!("{}.", rank + 1), x_rank, y, sc, col, Align::Left);
            ui.text(&e.initials, x_name, y, sc, col, Align::Left);
            ui.text(&format!("{}", e.score), x_score, y, sc, col, Align::Right);
        }
        if t > 2.0 && (t * 2.0) as i32 % 2 == 0 {
            let msg = if self.debug { "CLICK TO RIDE AGAIN!" } else { "PRESS A TO RIDE AGAIN!" };
            ui.text_fit(msg, w * 0.5, h * 0.9, 1.5, w * 0.9, [1.0, 0.9, 0.5, 1.0], Align::Center);
        }
    }

    fn award(&self, i: usize) -> &'static str {
        let p = &self.players[i];
        let mut best = AWARDS.len() - 1;
        let mut best_v = 0;
        // Boss hits count a little less so the boss doesn't always win.
        for (k, &v) in p.counts.iter().enumerate() {
            let v = if k == BOSS_COUNT { v / 3 } else { v };
            if v > best_v {
                best_v = v;
                best = k;
            }
        }
        AWARDS[best]
    }
}

const CLOUD_CELL: f32 = 12.0;
const CLOUD_Y: f32 = 82.0;
const CLOUD_THICK: f32 = 5.0;
const CLOUD_X0: f32 = -330.0;
const CLOUD_X1: f32 = 330.0;
const CLOUD_SPEED: f32 = 1.2;

/// One mist puff.
struct Puff {
    pos: Vec3,
    /// Half width and half height of the quad.
    half: Vec2,
    ph: f32,
    alpha: f32,
    tint: [f32; 3],
    /// How far it drifts around its spot.
    wander: f32,
    /// If > 0, the puff keeps rising this far and fading out.
    rise: f32,
    /// Camera zones it's drawn in.
    zones: &'static [Zone],
}

/// Mist over the cemetery (station up to the house), a green haze over the crypt floor,
/// and green wisps curling up out of the hole in the dining-hall floor and down the shaft.
fn mist_puffs(track: &Track) -> Vec<Puff> {
    const GREY: [f32; 3] = [0.5, 0.55, 0.68];
    const GREEN: [f32; 3] = [0.35, 0.75, 0.42];
    let mut rng = Rng::new(4242);
    let mut out = vec![];
    let mut puff = |ph: f32, pos: Vec3, half: Vec2, alpha, tint, wander, rise, zones| {
        out.push(Puff { pos, half, ph, alpha, tint, wander, rise, zones });
    };
    // Graveyard: wide, low puffs just above the grass (ground top is y = 1).
    for _ in 0..140 {
        let (x, z) = (rng.range(-30.0, 30.0), rng.range(-28.0, 36.0));
        let half = Vec2::new(rng.range(3.0, 6.0), 1.0);
        puff(rng.range(0.0, 6.28), Vec3::new(x, 1.0 + half.y * 0.85, z), half, 0.2, GREY, 1.8, 0.0, &[Zone::Graveyard]);
    }
    // Crypt floor (top at y = -17), seen from the bottom of the drop onwards.
    for _ in 0..90 {
        let (x, z) = (rng.range(-13.0, 13.0), rng.range(141.0, 175.0));
        let half = Vec2::new(rng.range(2.5, 5.0), 0.9);
        puff(rng.range(0.0, 6.28), Vec3::new(x, -17.0 + half.y * 0.85, z), half, 0.22, GREEN, 1.4, 0.0, &[Zone::Drop, Zone::Crypt]);
    }
    // Wisps rising out of the hole the track dives into (dining floor at y = 1, z ~128-131).
    for _ in 0..18 {
        let pos = Vec3::new(rng.range(-1.8, 1.8), rng.range(-1.5, 0.5), rng.range(127.5, 131.5));
        let half = Vec2::new(rng.range(1.2, 2.2), rng.range(0.8, 1.3));
        let rise = rng.range(2.5, 4.0);
        puff(rng.range(0.0, 6.28), pos, half, 0.32, GREEN, 0.5, rise, &[Zone::Library, Zone::Dining, Zone::Drop]);
    }
    // A glowing haze filling the shaft itself, following the track down.
    for k in 0..16 {
        let z = 130.0 + k as f32 * 0.75;
        let rail = track.pos_at(track.dist_at_time(track.time_at_z(z)));
        let pos = Vec3::new(rail.x + rng.range(-1.5, 1.5), rail.y + 1.2 + rng.range(-0.4, 0.8), z);
        let half = Vec2::new(rng.range(1.5, 2.5), 1.2);
        puff(rng.range(0.0, 6.28), pos, half, 0.2, GREEN, 0.4, 0.0, &[Zone::Dining, Zone::Drop]);
    }
    out
}

/// Blobby cloud cover from smoothed value noise, merged into runs along x.
fn cloud_runs() -> Vec<(f32, f32, f32)> {
    let noise = |x: i32, z: i32| {
        let (gx, gz) = (x.div_euclid(4), z.div_euclid(4));
        let (fx, fz) = (x.rem_euclid(4) as f32 / 4.0, z.rem_euclid(4) as f32 / 4.0);
        let h = |a: i32, b: i32| hash3(a, 77, b);
        let top = h(gx, gz) + (h(gx + 1, gz) - h(gx, gz)) * fx;
        let bot = h(gx, gz + 1) + (h(gx + 1, gz + 1) - h(gx, gz + 1)) * fx;
        top + (bot - top) * fz + (hash3(x, 78, z) - 0.5) * 0.15
    };
    let nx = ((CLOUD_X1 - CLOUD_X0) / CLOUD_CELL) as i32;
    let mut runs = vec![];
    for cz in -14..34 {
        let mut start = None;
        for cx in 0..=nx {
            let on = cx < nx && noise(cx, cz) > 0.6;
            match (on, start) {
                (true, None) => start = Some(cx),
                (false, Some(s)) => {
                    runs.push((CLOUD_X0 + s as f32 * CLOUD_CELL, cz as f32 * CLOUD_CELL, (cx - s) as f32));
                    start = None;
                }
                _ => {}
            }
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wii(id: u32, a: bool) -> SourceInput {
        SourceInput {
            id: SourceId::Wii(id),
            aim: Some(Vec2::new(640.0, 360.0)),
            fire: false,
            fire_pressed: false,
            join: a,
            start: false,
            pause: false,
            quit_to_title: false,
        }
    }

    #[test]
    fn players_must_rejoin_after_the_results_screen() {
        let track = Track::new();
        let world = crate::world::generate(&track);
        let mut g = Game::new(track, world, false, 0);
        let screen = Vec2::new(1280.0, 720.0);
        // Two remotes join on the title screen.
        g.update(0.016, &[wii(0, true), wii(1, true)], screen, None);
        assert_eq!(g.joined_count(), 2);
        // Skip to the results and press A to leave them.
        g.state = State::Results;
        g.state_t = 10.0;
        g.update(0.016, &[wii(0, true), wii(1, false)], screen, None);
        assert_eq!(g.state, State::Title);
        assert_eq!(g.joined_count(), 0, "nobody should still be joined");
        // Pressing A again joins a player afresh.
        g.update(0.016, &[wii(0, false), wii(1, false)], screen, None);
        g.update(0.016, &[wii(0, false), wii(1, true)], screen, None);
        assert_eq!(g.joined_count(), 1);
    }

    #[test]
    fn riders_who_make_the_table_shoot_their_initials() {
        let track = Track::new();
        let world = crate::world::generate(&track);
        let mut g = Game::new(track, world, false, 0);
        let screen = Vec2::new(1280.0, 720.0);
        g.update(0.016, &[wii(0, true), wii(1, true)], screen, None);
        g.players[0].score = 5000;
        g.players[1].score = 3000;
        g.state = State::Results;
        g.state_t = 10.0;
        g.update(0.016, &[wii(0, true), wii(1, false)], screen, None);
        assert_eq!(g.state, State::Initials);
        let aim = |id: u32, at: Vec2| SourceInput { aim: Some(at), ..wii(id, false) };
        let key = |g: &Game, k: usize| {
            let (x, y, w, h) = g.key_rects()[k];
            Vec2::new(x + w * 0.5, y + h * 0.5)
        };
        let mut shoot = |g: &mut Game, k: usize| {
            let at = key(g, k);
            for _ in 0..20 {
                g.update(0.016, &[aim(0, at), wii(1, false)], screen, None);
            }
            g.update(0.016, &[SourceInput { fire: true, fire_pressed: true, ..aim(0, at) }, wii(1, false)], screen, None);
            g.update(0.016, &[aim(0, at), wii(1, false)], screen, None);
        };
        // Player 1 spells "AL" (a fourth letter would be ignored), then shoots END.
        for _ in 0..40 {
            g.update(0.016, &[wii(0, false), wii(1, false)], screen, None);
        }
        for k in [0, 11, 11, 11, 11, KEY_DEL, KEY_DEL, 11, KEY_END] {
            shoot(&mut g, k);
        }
        assert_eq!(g.initials[0].as_ref().unwrap().letters, "AL");
        assert!(g.initials[0].as_ref().unwrap().done_at.is_some());
        assert_eq!(g.state, State::Initials, "still waiting for player 2");
        // Player 2 never finishes: after the time limit they go in as "P2".
        while g.state == State::Initials {
            g.update(0.1, &[wii(0, false), wii(1, false)], screen, None);
        }
        assert_eq!(g.state, State::HighScores);
        let names: Vec<_> = g.high.entries.iter().map(|e| (e.score, e.initials.as_str())).collect();
        assert_eq!(names, vec![(5000, "AL"), (3000, "P2")]);
        assert_eq!(g.high_new, vec![(0, 0), (1, 1)]);
    }

    #[test]
    fn a_swarm_of_bats_flies_up_the_big_drop() {
        let track = Track::new();
        let world = crate::world::generate(&track);
        let mut g = Game::new(track, world, false, 1);
        let screen = Vec2::new(1280.0, 720.0);
        let (t_drop, t_crypt) = (g.track.time_at_z(127.5), g.track.time_at_z(140.0));
        let mut before = None;
        let mut most = 0;
        for _ in 0..150 * 60 {
            g.update(1.0 / 60.0, &[], screen, None);
            if g.state != State::Ride {
                continue;
            }
            if before.is_none() && g.ride_t >= t_drop - 1.5 {
                before = Some(g.spawn_kinds[Kind::Bat.index()]);
            }
            if g.ride_t >= t_drop - 1.5 {
                most = most.max(g.targets.iter().filter(|t| t.kind == Kind::Bat).count());
            }
            if g.ride_t > t_crypt {
                break;
            }
        }
        let swarm = g.spawn_kinds[Kind::Bat.index()] - before.unwrap();
        assert!(swarm >= 7, "only {swarm} of the 9 swarm bats flew up the drop");
        assert!(most >= 3, "at most {most} bats on screen at once");
    }

    #[test]
    fn haunted_portraits_come_off_the_walls_of_the_house() {
        let track = Track::new();
        let world = crate::world::generate(&track);
        let mut g = Game::new(track, world, false, 2);
        let screen = Vec2::new(1280.0, 720.0);
        let t_drop = g.track.time_at_z(127.5);
        let mut zones = vec![];
        for _ in 0..150 * 60 {
            g.update(1.0 / 60.0, &[], screen, None);
            for t in g.targets.iter().filter(|t| t.kind == Kind::Portrait) {
                zones.push(zone_at(t.home));
            }
            if g.state == State::Ride && g.ride_t > t_drop {
                break;
            }
        }
        assert!(g.spawn_kinds[Kind::Portrait.index()] >= 3, "only {} portraits appeared", g.spawn_kinds[Kind::Portrait.index()]);
        assert!(zones.iter().all(|z| matches!(z, Zone::Foyer | Zone::Library | Zone::Dining)));
        assert!(zones.contains(&Zone::Dining), "no portraits in the dining hall");
    }

    #[test]
    fn every_player_must_pop_their_practice_target_before_the_ride() {
        let track = Track::new();
        let world = crate::world::generate(&track);
        let mut g = Game::new(track, world, false, 0);
        let screen = Vec2::new(1280.0, 720.0);
        g.update(0.016, &[wii(0, true), wii(1, true)], screen, None);
        let mut start = wii(0, false);
        start.start = true;
        g.update(0.016, &[start, wii(1, false)], screen, None);
        assert_eq!(g.state, State::Tutorial);
        let aim = |id: u32, at: Vec2| SourceInput { aim: Some(at), ..wii(id, false) };
        let shot = |id: u32, at: Vec2| SourceInput { fire: true, fire_pressed: true, ..aim(id, at) };
        let (p0, p1) = (g.practice_pos(0), g.practice_pos(1));
        // Player 1 can't pop player 2's target, only their own.
        for _ in 0..30 {
            g.update(0.016, &[aim(0, p1), wii(1, false)], screen, None);
        }
        g.update(0.016, &[shot(0, p1), wii(1, false)], screen, None);
        g.update(0.016, &[wii(0, false), wii(1, false)], screen, None);
        assert!(g.practice[1].is_none());
        for _ in 0..30 {
            g.update(0.016, &[aim(0, p0), wii(1, false)], screen, None);
        }
        g.update(0.016, &[shot(0, p0), wii(1, false)], screen, None);
        assert!(g.practice[0].is_some());
        for _ in 0..120 {
            g.update(0.016, &[wii(0, false), wii(1, false)], screen, None);
        }
        assert_eq!(g.state, State::Tutorial, "waits for player 2");
        for _ in 0..30 {
            g.update(0.016, &[wii(0, false), aim(1, p1)], screen, None);
        }
        g.update(0.016, &[wii(0, false), shot(1, p1)], screen, None);
        for _ in 0..60 {
            g.update(0.016, &[wii(0, false), wii(1, false)], screen, None);
        }
        assert_eq!(g.state, State::Ride);
    }
}
