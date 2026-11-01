//! The voxel world: blocks, Minecraft-style flood-fill block lighting (coloured) and
//! moonlight, meshing, and what every level shares (the ride's tunnel, the boss doorway,
//! wall decorations). Each level builds its own scenery around the same track and the
//! same parts of the ride: `halloween` (graveyard, haunted mansion, crypt, the Pumpkin
//! King's lair) and `christmas` (snowy village, Santa's workshop, ice cave, the Snow
//! King's palace).

mod christmas;
mod halloween;

use crate::atlas::*;
use crate::level::Level;
use crate::mesh::{push_box, push_box_tiles, push_quad, shade, FACES};
use crate::rng::Rng;
use crate::track::Track;
use crate::vk::Vertex;
use glam::{Mat4, Vec3};
use std::collections::VecDeque;


pub mod b {
    pub const AIR: u8 = 0;
    pub const GRASS: u8 = 1;
    pub const DIRT: u8 = 2;
    pub const STONE: u8 = 3;
    pub const COBBLE: u8 = 4;
    pub const MOSSY: u8 = 5;
    pub const STONEBRICK: u8 = 6;
    pub const PLANK: u8 = 7;
    pub const DARKPLANK: u8 = 8;
    pub const LOG: u8 = 9;
    pub const WALL_PURPLE: u8 = 10;
    pub const WALL_GREEN: u8 = 11;
    pub const CARPET: u8 = 12;
    pub const BOOKSHELF: u8 = 13;
    pub const PUMPKIN: u8 = 14;
    pub const WEB: u8 = 15;
    pub const WINDOW: u8 = 16;
    pub const WINDOW_LIT: u8 = 17;
    pub const GRAVE: u8 = 18;
    pub const ROOF: u8 = 19;
    pub const CRYPT: u8 = 20;
    pub const PURPLE: u8 = 21;
    pub const TORCH: u8 = 22;
    pub const SOUL_TORCH: u8 = 23;
    pub const LANTERN: u8 = 24;
    pub const CLOTH: u8 = 25;
    pub const PORTRAIT: u8 = 26;
    pub const FENCE: u8 = 27;
    pub const CANDLE: u8 = 28;
    pub const AMETHYST: u8 = 29;
    pub const BONES: u8 = 30;
    pub const FIRE: u8 = 31;
    pub const HAY: u8 = 32;
    pub const PORTRAIT2: u8 = 33;
    pub const TALLGRASS: u8 = 34;
    pub const DEADBUSH: u8 = 35;
    pub const MUSHROOM: u8 = 36;
    pub const CLOCK: u8 = 37;
    pub const CANDY: u8 = 38;
    pub const IRON: u8 = 39;
    pub const CAKE: u8 = 40;
    pub const POT: u8 = 41;
    pub const BANNER: u8 = 42;
    pub const CHAIN: u8 = 43;
    pub const SKULLS: u8 = 44;
    pub const CAULDRON: u8 = 45;
    pub const CANDY2: u8 = 46;
    pub const PAINTING3: u8 = 47;
    pub const SHIELD: u8 = 48;
    pub const GOLD: u8 = 49;
    pub const MIRROR: u8 = 50;
    /// Gold that catches the light (boss door frame and pillar bands).
    pub const GOLD_GLOW: u8 = 51;
    // Christmas.
    /// Snow-covered ground.
    pub const SNOW: u8 = 52;
    pub const SNOW_BLOCK: u8 = 53;
    pub const ICE: u8 = 54;
    pub const ICE_BRICK: u8 = 55;
    pub const PINE: u8 = 56;
    /// Pine needles with snow on top.
    pub const PINE_SNOW: u8 = 57;
    pub const CANDY_CANE: u8 = 58;
    pub const PRESENT_RED: u8 = 59;
    pub const PRESENT_GREEN: u8 = 60;
    pub const GINGERBREAD: u8 = 61;
    /// A string of coloured fairy lights.
    pub const LIGHTS: u8 = 62;
    pub const GARLAND: u8 = 63;
    pub const STOCKING: u8 = 64;
    pub const WREATH: u8 = 65;
    pub const ICICLE: u8 = 66;
    pub const ICE_CRYSTAL: u8 = 67;
    /// A torch with a pale blue flame.
    pub const FROST_TORCH: u8 = 68;
    /// The glowing star on top of a Christmas tree.
    pub const XMAS_STAR: u8 = 69;
    pub const WALL_RED: u8 = 70;
    pub const WALL_PINE: u8 = 71;
    pub const ROOF_RED: u8 = 72;
    pub const TOYSHELF: u8 = 73;
    /// A snowman's head (its face is on the side given by the block's meta, like a pumpkin's).
    pub const SNOWMAN_HEAD: u8 = 74;
    pub const XMAS_ART: u8 = 75;
    pub const XMAS_ART2: u8 = 76;
    pub const XMAS_ART3: u8 = 77;
    /// White icing.
    pub const FROSTING: u8 = 78;
    /// Red brick.
    pub const BRICK: u8 = 79;
}

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Cube,
    Glass,
    Cross,
    Torch,
    Post,
    Candle,
    Lantern,
    Bones,
    Small,
    Banner,
    Cauldron,
    /// Fairy lights strung through the block.
    Lights,
}

#[derive(Clone, Copy)]
struct Props {
    opaque: bool,
    shape: Shape,
    tiles: [u8; 3], // top, side, bottom
    emit: [u8; 3],
    glow: bool,
    size: [f32; 3],
}

fn props(id: u8) -> Props {
    let cube = |t: u8| Props { opaque: true, shape: Shape::Cube, tiles: [t, t, t], emit: [0; 3], glow: false, size: [1.0; 3] };
    match id {
        b::GRASS => Props { tiles: [T_GRASS_TOP, T_GRASS_SIDE, T_DIRT], ..cube(0) },
        b::DIRT => cube(T_DIRT),
        b::STONE => cube(T_STONE),
        b::COBBLE => cube(T_COBBLE),
        b::MOSSY => cube(T_MOSSY),
        b::STONEBRICK => cube(T_STONEBRICK),
        b::PLANK => cube(T_PLANK),
        b::DARKPLANK => cube(T_DARKPLANK),
        b::LOG => Props { tiles: [T_LOG_TOP, T_LOG, T_LOG_TOP], ..cube(0) },
        b::WALL_PURPLE => cube(T_WALL_PURPLE),
        b::WALL_GREEN => cube(T_WALL_GREEN),
        b::CARPET => cube(T_CARPET),
        b::BOOKSHELF => Props { tiles: [T_PLANK, T_BOOKSHELF, T_PLANK], ..cube(0) },
        b::PUMPKIN => Props { tiles: [T_PUMPKIN_TOP, T_PUMPKIN_SIDE, T_PUMPKIN_TOP], emit: [14, 9, 3], ..cube(0) },
        b::WEB => Props { opaque: false, shape: Shape::Cross, ..cube(T_COBWEB) },
        b::WINDOW => Props { opaque: false, shape: Shape::Glass, ..cube(T_WINDOW) },
        b::WINDOW_LIT => Props { emit: [12, 8, 3], glow: true, ..cube(T_WINDOW_LIT) },
        b::GRAVE => Props { tiles: [T_STONE, T_GRAVE_PLAIN, T_STONE], ..cube(0) },
        b::ROOF => cube(T_ROOF),
        b::CRYPT => cube(T_CRYPT),
        b::PURPLE => cube(T_PURPLE),
        b::TORCH => Props { opaque: false, shape: Shape::Torch, emit: [15, 11, 6], ..cube(T_WHITE) },
        b::SOUL_TORCH => Props { opaque: false, shape: Shape::Torch, emit: [4, 14, 11], ..cube(T_WHITE) },
        b::LANTERN => Props { opaque: false, shape: Shape::Lantern, emit: [15, 13, 9], glow: true, ..cube(T_LANTERN) },
        b::CLOTH => cube(T_CLOTH),
        b::PORTRAIT => Props { tiles: [T_DARKPLANK, T_PORTRAIT, T_DARKPLANK], ..cube(0) },
        b::PORTRAIT2 => Props { tiles: [T_DARKPLANK, T_PORTRAIT2, T_DARKPLANK], ..cube(0) },
        b::FENCE => Props { opaque: false, shape: Shape::Post, ..cube(T_METAL) },
        b::CANDLE => Props { opaque: false, shape: Shape::Candle, emit: [11, 8, 4], ..cube(T_WHITE) },
        b::AMETHYST => Props { emit: [11, 5, 15], glow: true, ..cube(T_AMETHYST) },
        b::BONES => Props { opaque: false, shape: Shape::Bones, ..cube(T_BONE) },
        b::FIRE => Props { opaque: false, shape: Shape::Cross, emit: [15, 9, 3], glow: true, ..cube(T_FIRE) },
        b::HAY => Props { tiles: [T_HAY, T_HAY, T_HAY], ..cube(0) },
        b::TALLGRASS => Props { opaque: false, shape: Shape::Cross, ..cube(T_TALLGRASS) },
        b::DEADBUSH => Props { opaque: false, shape: Shape::Cross, ..cube(T_DEADBUSH) },
        b::MUSHROOM => Props { opaque: false, shape: Shape::Cross, emit: [5, 2, 2], ..cube(T_MUSHROOM) },
        b::CLOCK => Props { tiles: [T_DARKPLANK, T_CLOCK, T_DARKPLANK], ..cube(0) },
        b::CANDY => cube(T_CANDY),
        b::CANDY2 => cube(T_CANDY2),
        b::PAINTING3 => Props { tiles: [T_DARKPLANK, T_PAINTING3, T_DARKPLANK], ..cube(0) },
        b::SHIELD => Props { tiles: [T_DARKPLANK, T_SHIELD, T_DARKPLANK], ..cube(0) },
        b::GOLD => Props { glow: false, ..cube(T_GOLD) },
        b::GOLD_GLOW => Props { glow: true, ..cube(T_GOLD) },
        b::MIRROR => Props { tiles: [T_GOLD, T_MIRROR, T_GOLD], ..cube(0) },
        b::IRON => cube(T_IRON),
        b::SKULLS => cube(T_SKULLS),
        b::CAKE => Props { opaque: false, shape: Shape::Small, tiles: [T_CAKE_TOP, T_CAKE_SIDE, T_CAKE_SIDE], size: [0.8, 0.5, 0.8], ..cube(0) },
        b::POT => Props { opaque: false, shape: Shape::Small, tiles: [T_POT, T_POT, T_POT], size: [0.5, 0.7, 0.5], ..cube(0) },
        b::BANNER => Props { opaque: false, shape: Shape::Banner, ..cube(T_BANNER) },
        b::CHAIN => Props { opaque: false, shape: Shape::Cross, ..cube(T_CHAIN) },
        b::CAULDRON => Props { opaque: false, shape: Shape::Cauldron, emit: [3, 13, 5], ..cube(T_IRON) },
        b::SNOW => Props { tiles: [T_SNOW_TOP, T_SNOW_SIDE, T_DIRT], ..cube(0) },
        b::SNOW_BLOCK | b::SNOWMAN_HEAD => cube(T_SNOW_TOP),
        b::ICE => cube(T_ICE),
        b::ICE_BRICK => cube(T_ICE_BRICK),
        b::PINE => cube(T_PINE),
        b::PINE_SNOW => Props { tiles: [T_SNOW_TOP, T_PINE_SNOW, T_PINE], ..cube(0) },
        b::CANDY_CANE => cube(T_CANDY_CANE),
        b::PRESENT_RED => Props { opaque: false, shape: Shape::Small, tiles: [T_PRESENT_RED_TOP, T_PRESENT_RED, T_PRESENT_RED], size: [0.8, 0.75, 0.8], ..cube(0) },
        b::PRESENT_GREEN => Props { opaque: false, shape: Shape::Small, tiles: [T_PRESENT_GREEN_TOP, T_PRESENT_GREEN, T_PRESENT_GREEN], size: [0.65, 0.55, 0.65], ..cube(0) },
        b::GINGERBREAD => cube(T_GINGERBREAD),
        b::FROSTING => cube(T_FROSTING),
        b::BRICK => cube(T_BRICK),
        b::LIGHTS => Props { opaque: false, shape: Shape::Lights, emit: [13, 11, 9], glow: true, ..cube(T_WHITE) },
        b::GARLAND => Props { opaque: false, shape: Shape::Banner, ..cube(T_GARLAND) },
        b::STOCKING => Props { opaque: false, shape: Shape::Banner, ..cube(T_STOCKING) },
        b::WREATH => Props { opaque: false, shape: Shape::Banner, ..cube(T_WREATH) },
        b::ICICLE => Props { opaque: false, shape: Shape::Cross, ..cube(T_ICICLE) },
        b::ICE_CRYSTAL => Props { emit: [5, 12, 15], glow: true, ..cube(T_ICE_CRYSTAL) },
        b::FROST_TORCH => Props { opaque: false, shape: Shape::Torch, emit: [7, 11, 15], ..cube(T_WHITE) },
        b::XMAS_STAR => Props { opaque: false, shape: Shape::Cross, emit: [15, 13, 6], glow: true, ..cube(T_XSTAR) },
        b::WALL_RED => cube(T_WALL_RED),
        b::WALL_PINE => cube(T_WALL_PINE),
        // Snow on every red roof.
        b::ROOF_RED => Props { tiles: [T_SNOW_TOP, T_ROOF_RED, T_ROOF_RED], ..cube(0) },
        b::TOYSHELF => Props { tiles: [T_PLANK, T_TOYSHELF, T_PLANK], ..cube(0) },
        b::XMAS_ART => Props { tiles: [T_DARKPLANK, T_XMAS_ART, T_DARKPLANK], ..cube(0) },
        b::XMAS_ART2 => Props { tiles: [T_DARKPLANK, T_XMAS_ART2, T_DARKPLANK], ..cube(0) },
        b::XMAS_ART3 => Props { tiles: [T_DARKPLANK, T_XMAS_ART3, T_DARKPLANK], ..cube(0) },
        _ => Props { opaque: false, shape: Shape::Cube, tiles: [0; 3], emit: [0; 3], glow: false, size: [1.0; 3] },
    }
}

pub struct World {
    level: Level,
    /// The blocks stored: from `lo` (inclusive) to `hi` (exclusive). Outside them is
    /// stone below and air above.
    lo: [i32; 3],
    hi: [i32; 3],
    blocks: Vec<u8>,
    meta: Vec<u8>,
    light: Vec<[u8; 4]>, // r, g, b, sky
}

const MOON: Vec3 = Vec3::new(0.36, 0.40, 0.62);
const AMBIENT: Vec3 = Vec3::new(0.06, 0.05, 0.08);

fn curve(l: u8) -> f32 {
    if l == 0 { 0.0 } else { 0.86f32.powi(15 - l as i32) }
}

impl World {
    fn new(level: Level) -> World {
        let (lo, hi) = level.bounds();
        let n = ((hi[0] - lo[0]) * (hi[1] - lo[1]) * (hi[2] - lo[2])) as usize;
        World { level, lo, hi, blocks: vec![0; n], meta: vec![0; n], light: vec![[0; 4]; n] }
    }

    /// Size of the stored box along each axis.
    fn size(&self) -> [i32; 3] {
        [self.hi[0] - self.lo[0], self.hi[1] - self.lo[1], self.hi[2] - self.lo[2]]
    }

    #[inline]
    fn idx(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        let ([x0, y0, z0], [x1, y1, z1]) = (self.lo, self.hi);
        if x < x0 || x >= x1 || y < y0 || y >= y1 || z < z0 || z >= z1 {
            None
        } else {
            let [sx, sy, _] = self.size();
            Some((((z - z0) * sy + (y - y0)) * sx + (x - x0)) as usize)
        }
    }

    pub fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        match self.idx(x, y, z) {
            Some(i) => self.blocks[i],
            None => {
                if y < self.lo[1] { b::STONE } else { b::AIR }
            }
        }
    }

    /// True if the sky is straight above this spot (nothing solid over it).
    pub fn open_sky(&self, p: Vec3) -> bool {
        match self.idx(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32) {
            Some(i) => self.light[i][3] == 15,
            None => p.y >= self.hi[1] as f32,
        }
    }

    /// Positions (block bottom centres) of every block of kind `id`, for things animated per frame.
    pub fn cells_of(&self, id: u8) -> Vec<Vec3> {
        let mut out = vec![];
        let ([x0, y0, z0], [x1, y1, z1]) = (self.lo, self.hi);
        for z in z0..z1 {
            for y in y0..y1 {
                for x in x0..x1 {
                    if self.get(x, y, z) == id {
                        out.push(Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5));
                    }
                }
            }
        }
        out
    }

    pub fn solid_at(&self, p: Vec3) -> bool {
        props(self.get(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)).opaque
    }

    /// True if anything (wall, prop, web, candle...) is in this spot. Low ground cover
    /// like grass doesn't count. Used to keep targets from spawning inside things.
    pub fn occupied(&self, p: Vec3) -> bool {
        let id = self.get(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        // Thin things (candle stubs, hanging chains) are fine to float next to.
        !matches!(id, b::AIR | b::TALLGRASS | b::DEADBUSH | b::MUSHROOM | b::CANDLE | b::CHAIN)
    }

    /// True if this spot would hide a target behind it (walls and chunky props).
    pub fn blocks_view(&self, p: Vec3) -> bool {
        let id = self.get(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        props(id).opaque
            || matches!(
                id,
                b::FENCE | b::LANTERN | b::CAULDRON | b::POT | b::CAKE | b::BANNER | b::PRESENT_RED | b::PRESENT_GREEN | b::GARLAND | b::STOCKING | b::WREATH
            )
    }

    fn set(&mut self, x: i32, y: i32, z: i32, id: u8) {
        if let Some(i) = self.idx(x, y, z) {
            self.blocks[i] = id;
            self.meta[i] = 0;
        }
    }

    fn set_meta(&mut self, x: i32, y: i32, z: i32, id: u8, m: u8) {
        if let Some(i) = self.idx(x, y, z) {
            self.blocks[i] = id;
            self.meta[i] = m;
        }
    }

    fn fill(&mut self, x0: i32, x1: i32, y0: i32, y1: i32, z0: i32, z1: i32, id: u8) {
        for z in z0..=z1 {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    self.set(x, y, z, id);
                }
            }
        }
    }

    /// Hollow box: floor, walls, ceiling, air inside (bounds inclusive).
    fn room(&mut self, x0: i32, x1: i32, y0: i32, y1: i32, z0: i32, z1: i32, wall: u8, floor: u8, ceil: u8) {
        for z in z0..=z1 {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let id = if y == y0 {
                        floor
                    } else if y == y1 {
                        ceil
                    } else if x == x0 || x == x1 || z == z0 || z == z1 {
                        wall
                    } else {
                        b::AIR
                    };
                    self.set(x, y, z, id);
                }
            }
        }
    }

    fn is_air(&self, x: i32, y: i32, z: i32) -> bool {
        self.get(x, y, z) == b::AIR
    }

    /// Light colour at a world position (for entities).
    pub fn light_at(&self, p: Vec3) -> Vec3 {
        let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        let mut l = self.light_cell(x, y, z);
        if props(self.get(x, y, z)).opaque {
            l = self.light_cell(x, y + 1, z);
        }
        l
    }

    fn light_cell(&self, x: i32, y: i32, z: i32) -> Vec3 {
        match self.idx(x, y, z) {
            Some(i) => {
                let l = self.light[i];
                Vec3::new(curve(l[0]), curve(l[1]), curve(l[2])) + MOON * curve(l[3]) + AMBIENT
            }
            None => MOON + AMBIENT,
        }
    }

    // ------------------------------------------------------------------ lighting

    fn compute_light(&mut self) {
        let n = self.blocks.len();
        let opaque: Vec<bool> = self.blocks.iter().map(|&id| props(id).opaque).collect();
        // Sky light: straight down from the top until something opaque.
        let mut q: VecDeque<usize> = VecDeque::new();
        let ([x0, y0, z0], [x1, y1, z1]) = (self.lo, self.hi);
        for z in z0..z1 {
            for x in x0..x1 {
                for y in (y0..y1).rev() {
                    let i = self.idx(x, y, z).unwrap();
                    if opaque[i] {
                        break;
                    }
                    self.light[i][3] = 15;
                    q.push_back(i);
                }
            }
        }
        self.flood(&opaque, q, 3);
        // Block light per colour channel.
        for ch in 0..3 {
            let mut q = VecDeque::new();
            for i in 0..n {
                let e = props(self.blocks[i]).emit[ch];
                if e > 0 {
                    self.light[i][ch] = e;
                    q.push_back(i);
                }
            }
            self.flood(&opaque, q, ch);
        }
    }

    fn flood(&mut self, opaque: &[bool], mut q: VecDeque<usize>, ch: usize) {
        let [sx, sy, sz] = self.size();
        let strides: [isize; 6] = [1, -1, sx as isize, -(sx as isize), (sx * sy) as isize, -((sx * sy) as isize)];
        while let Some(i) = q.pop_front() {
            let l = self.light[i][ch];
            if l <= 1 {
                continue;
            }
            let x = (i as i32) % sx;
            let y = ((i as i32) / sx) % sy;
            let z = (i as i32) / (sx * sy);
            for (k, s) in strides.iter().enumerate() {
                let ok = match k {
                    0 => x + 1 < sx,
                    1 => x > 0,
                    2 => y + 1 < sy,
                    3 => y > 0,
                    4 => z + 1 < sz,
                    _ => z > 0,
                };
                if !ok {
                    continue;
                }
                let j = (i as isize + s) as usize;
                if opaque[j] {
                    continue;
                }
                if self.light[j][ch] + 1 < l {
                    self.light[j][ch] = l - 1;
                    q.push_back(j);
                }
            }
        }
    }

    // ------------------------------------------------------------------ meshing

    pub fn build_mesh(&self, track: &Track) -> Vec<Vertex> {
        let mut out = self.build_world_mesh(track);
        sky(&mut out, self.level);
        out
    }

    /// The blocks and the track, without the sky (see `sky_mesh`).
    pub fn build_world_mesh(&self, track: &Track) -> Vec<Vertex> {
        let mut out = Vec::with_capacity(1 << 20);
        let ([x0, y0, z0], [x1, y1, z1]) = (self.lo, self.hi);
        for z in z0..z1 {
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = self.idx(x, y, z).unwrap();
                    let id = self.blocks[i];
                    if id == b::AIR {
                        continue;
                    }
                    let p = props(id);
                    match p.shape {
                        Shape::Cube | Shape::Glass => self.mesh_cube(&mut out, x, y, z, id, &p, self.meta[i]),
                        _ => self.mesh_special(&mut out, x, y, z, id, &p),
                    }
                }
            }
        }
        self.mesh_track(&mut out, track);
        out
    }

    /// Just the track (the last faces of `build_world_mesh`).
    pub fn track_mesh(&self, track: &Track) -> Vec<Vertex> {
        let mut out = vec![];
        self.mesh_track(&mut out, track);
        out
    }

    fn mesh_cube(&self, out: &mut Vec<Vertex>, x: i32, y: i32, z: i32, id: u8, p: &Props, meta: u8) {
        for (fi, (n, o, r, u)) in FACES.iter().enumerate() {
            let (nx, ny, nz) = (x + n[0], y + n[1], z + n[2]);
            let nid = self.get(nx, ny, nz);
            let np = props(nid);
            if np.opaque || (p.shape == Shape::Glass && nid == id) {
                continue;
            }
            let mut tile = match fi {
                2 => p.tiles[0],
                3 => p.tiles[2],
                _ => p.tiles[1],
            };
            if id == b::PUMPKIN && fi == meta as usize {
                tile = T_PUMPKIN_FACE;
            }
            if id == b::GRAVE && fi == meta as usize {
                tile = T_GRAVE;
            }
            if id == b::SNOWMAN_HEAD && fi == meta as usize {
                tile = T_SNOWMAN_FACE;
            }
            let base = Vec3::new(x as f32, y as f32, z as f32);
            let (o, r, u) = (Vec3::from(*o), Vec3::from(*r), Vec3::from(*u));
            let pts = [base + o, base + o + r, base + o + r + u, base + o + u];
            let nv = Vec3::new(n[0] as f32, n[1] as f32, n[2] as f32);
            let s = shade(nv);
            let mut cols = [[0.0f32; 4]; 4];
            let mut aos = [0u8; 4];
            let corners = [(0, 0), (1, 0), (1, 1), (0, 1)];
            let ri = [r.x as i32, r.y as i32, r.z as i32];
            let ui = [u.x as i32, u.y as i32, u.z as i32];
            for (k, (tr, tu)) in corners.iter().enumerate() {
                let sr = 2 * tr - 1;
                let su = 2 * tu - 1;
                let a = [nx + ri[0] * sr, ny + ri[1] * sr, nz + ri[2] * sr];
                let bb = [nx + ui[0] * su, ny + ui[1] * su, nz + ui[2] * su];
                let c = [a[0] + ui[0] * su, a[1] + ui[1] * su, a[2] + ui[2] * su];
                let oa = props(self.get(a[0], a[1], a[2])).opaque;
                let ob = props(self.get(bb[0], bb[1], bb[2])).opaque;
                let oc = props(self.get(c[0], c[1], c[2])).opaque;
                let ao = if oa && ob { 0 } else { 3 - (oa as u8 + ob as u8 + oc as u8) };
                aos[k] = ao;
                let mut l = self.light_cell(nx, ny, nz);
                let mut cnt = 1.0;
                if !oa {
                    l += self.light_cell(a[0], a[1], a[2]);
                    cnt += 1.0;
                }
                if !ob {
                    l += self.light_cell(bb[0], bb[1], bb[2]);
                    cnt += 1.0;
                }
                if !oc && !(oa && ob) {
                    l += self.light_cell(c[0], c[1], c[2]);
                    cnt += 1.0;
                }
                l /= cnt;
                let aof = [0.45, 0.65, 0.82, 1.0][ao as usize];
                if p.glow || (id == b::PUMPKIN && tile == T_PUMPKIN_FACE) {
                    let g = 0.85 + 0.15 * s;
                    cols[k] = [g, g, g, 0.0];
                } else {
                    let c = l * s * aof;
                    cols[k] = [c.x, c.y, c.z, s * aof];
                }
            }
            let uv = tile_uv(tile);
            // Flip the quad diagonal to avoid ugly AO interpolation.
            if (aos[0] as i32 + aos[2] as i32) < (aos[1] as i32 + aos[3] as i32) {
                let pts2 = [pts[1], pts[2], pts[3], pts[0]];
                let cols2 = [cols[1], cols[2], cols[3], cols[0]];
                let uvs = [[uv[2], uv[3]], [uv[2], uv[1]], [uv[0], uv[1]], [uv[0], uv[3]]];
                for k in 0..4 {
                    out.push(Vertex { pos: pts2[k].to_array(), uv: uvs[k], color: cols2[k] });
                }
            } else {
                push_quad(out, pts, uv, cols);
            }
        }
    }

    fn mesh_special(&self, out: &mut Vec<Vertex>, x: i32, y: i32, z: i32, id: u8, p: &Props) {
        let c = Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5);
        let light = self.light_cell(x, y, z);
        let m = |center: Vec3, size: Vec3| Mat4::from_translation(center) * Mat4::from_scale(size);
        match p.shape {
            Shape::Torch => {
                // Lean toward an adjacent wall if there is one.
                let mut off = Vec3::ZERO;
                for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    if props(self.get(x + dx, y, z + dz)).opaque {
                        off = Vec3::new(dx as f32, 0.0, dz as f32) * 0.32;
                        break;
                    }
                }
                let flame = match id {
                    b::SOUL_TORCH => [0.45, 1.0, 0.85],
                    b::FROST_TORCH => [0.6, 0.85, 1.0],
                    _ => [1.0, 0.78, 0.3],
                };
                push_box(out, &m(c + off + Vec3::Y * 0.32, Vec3::new(0.14, 0.62, 0.14)), T_DARKPLANK, [1.0; 3], light, 0.0);
                push_box(out, &m(c + off + Vec3::Y * 0.7, Vec3::new(0.2, 0.22, 0.2)), T_WHITE, flame, light, 1.0);
            }
            Shape::Candle => {
                push_box(out, &m(c + Vec3::Y * 0.17, Vec3::new(0.18, 0.34, 0.18)), T_WHITE, [0.95, 0.92, 0.8], light, 0.3);
                push_box(out, &m(c + Vec3::Y * 0.42, Vec3::new(0.1, 0.14, 0.1)), T_WHITE, [1.0, 0.8, 0.3], light, 1.0);
            }
            Shape::Lantern => {
                push_box(out, &m(c + Vec3::Y * 0.4, Vec3::splat(0.6)), T_LANTERN, [1.0; 3], light, 1.0);
                if self.get(x, y + 1, z) == b::FENCE {
                    push_box(out, &m(c + Vec3::Y * 0.85, Vec3::new(0.08, 0.3, 0.08)), T_METAL, [0.35; 3], light, 0.0);
                }
            }
            Shape::Post => {
                let tint = [0.32, 0.3, 0.34];
                push_box(out, &m(c + Vec3::Y * 0.5, Vec3::new(0.18, 1.0, 0.18)), T_METAL, tint, light, 0.0);
                if self.get(x, y + 1, z) != b::FENCE && !props(self.get(x, y + 1, z)).opaque {
                    push_box(out, &m(c + Vec3::Y * 1.05, Vec3::new(0.1, 0.12, 0.1)), T_METAL, tint, light, 0.0);
                }
                for (dx, dz) in [(1, 0), (0, 1)] {
                    if self.get(x + dx, y, z + dz) == b::FENCE {
                        let mid = c + Vec3::new(dx as f32, 0.0, dz as f32) * 0.5;
                        let size = if dx == 1 { Vec3::new(1.0, 0.1, 0.08) } else { Vec3::new(0.08, 0.1, 1.0) };
                        push_box(out, &m(mid + Vec3::Y * 0.35, size), T_METAL, tint, light, 0.0);
                        push_box(out, &m(mid + Vec3::Y * 0.8, size), T_METAL, tint, light, 0.0);
                    }
                }
            }
            Shape::Bones => {
                let h = crate::mesh::hash3(x, y, z);
                for k in 0..3 {
                    let a = h * 6.28 + k as f32 * 2.1;
                    let pos = c + Vec3::new(a.cos() * 0.25, 0.06 + k as f32 * 0.02, a.sin() * 0.25);
                    let mm = Mat4::from_translation(pos)
                        * Mat4::from_rotation_y(a)
                        * Mat4::from_scale(Vec3::new(0.55, 0.1, 0.1));
                    push_box(out, &mm, T_BONE, [1.0; 3], light, 0.2);
                }
                let skull = Mat4::from_translation(c + Vec3::new(0.15 * h, 0.14, -0.1)) * Mat4::from_scale(Vec3::splat(0.28));
                push_box(out, &skull, T_BONE, [1.0; 3], light, 0.2);
            }
            Shape::Small => {
                let sz = Vec3::from(p.size);
                let (top, side) = (p.tiles[0], p.tiles[1]);
                let mm = m(c + Vec3::Y * sz.y * 0.5, sz);
                push_box_tiles(out, &mm, [side, side, top, side, side, side], [1.0; 3], light, 0.15);
                if id == b::CAKE {
                    push_box(out, &m(c + Vec3::Y * 0.62, Vec3::new(0.06, 0.16, 0.06)), T_WHITE, [1.0, 0.6, 0.7], light, 0.4);
                    push_box(out, &m(c + Vec3::Y * 0.74, Vec3::new(0.07, 0.08, 0.07)), T_WHITE, [1.0, 0.85, 0.3], light, 1.0);
                }
            }
            Shape::Banner => {
                let mut dir = Vec3::new(0.0, 0.0, 1.0);
                for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    if props(self.get(x + dx, y, z + dz)).opaque {
                        dir = Vec3::new(dx as f32, 0.0, dz as f32);
                        break;
                    }
                }
                let size = if dir.x != 0.0 { Vec3::new(0.06, 1.0, 0.9) } else { Vec3::new(0.9, 1.0, 0.06) };
                push_box(out, &m(c + dir * 0.45 + Vec3::Y * 0.5, size), p.tiles[1], [1.0; 3], light, 0.1);
            }
            Shape::Lights => {
                // Along the neighbouring lights, else along the wall it's hung on.
                let lit = |dx: i32, dz: i32| self.get(x + dx, y, z + dz) == b::LIGHTS;
                let wall = |dx: i32, dz: i32| props(self.get(x + dx, y, z + dz)).opaque;
                let along_x = lit(1, 0) || lit(-1, 0) || (!lit(0, 1) && !lit(0, -1) && (wall(0, 1) || wall(0, -1)));
                let (ax, off) = if along_x {
                    let side = if wall(0, 1) { 0.4 } else if wall(0, -1) { -0.4 } else { 0.0 };
                    (Vec3::X, Vec3::Z * side)
                } else {
                    let side = if wall(1, 0) { 0.4 } else if wall(-1, 0) { -0.4 } else { 0.0 };
                    (Vec3::Z, Vec3::X * side)
                };
                let mid = c + off + Vec3::Y * 0.6;
                let wire = if along_x { Vec3::new(1.0, 0.03, 0.03) } else { Vec3::new(0.03, 0.03, 1.0) };
                push_box(out, &m(mid, wire), T_WHITE, [0.15, 0.25, 0.12], light, 0.0);
                const BULBS: [[f32; 3]; 4] = [[1.0, 0.2, 0.2], [0.25, 1.0, 0.3], [0.3, 0.55, 1.0], [1.0, 0.85, 0.2]];
                for k in 0..3 {
                    let u = (k as f32 - 1.0) / 3.0;
                    let col = BULBS[((x + z + y) * 3 + k).rem_euclid(4) as usize];
                    push_box(out, &m(mid + ax * u - Vec3::Y * 0.09, Vec3::new(0.12, 0.16, 0.12)), T_WHITE, col, light, 1.0);
                }
            }
            Shape::Cauldron => {
                push_box(out, &m(c + Vec3::Y * 0.36, Vec3::new(0.9, 0.72, 0.9)), T_IRON, [0.3, 0.3, 0.34], light, 0.0);
                // The brew's surface and bubbles are animated by the game (see `cauldrons`).
            }
            // Fire is animated by the game each frame (see `cells_of`), not baked in.
            Shape::Cross if id == b::FIRE => {}
            Shape::Cross => {
                let uv = tile_uv(p.tiles[1]);
                let col = if p.glow { [1.0, 1.0, 1.0, 0.0] } else { [light.x, light.y, light.z, 1.0] };
                let base = Vec3::new(x as f32, y as f32, z as f32);
                for (a, bb) in [
                    (Vec3::new(0.1, 0.0, 0.1), Vec3::new(0.9, 0.0, 0.9)),
                    (Vec3::new(0.9, 0.0, 0.1), Vec3::new(0.1, 0.0, 0.9)),
                ] {
                    let pts = [base + a, base + bb, base + bb + Vec3::Y, base + a + Vec3::Y];
                    push_quad(out, pts, uv, [col; 4]);
                    let back = [pts[1], pts[0], pts[3], pts[2]];
                    push_quad(out, back, uv, [col; 4]);
                }
            }
            _ => {}
        }
    }

    fn mesh_track(&self, out: &mut Vec<Vertex>, track: &Track) {
        let samples = track.samples(0.5);
        for w in samples.windows(2) {
            let (a, bf) = (w[0], w[1]);
            let fwd = (bf.pos - a.pos).normalize_or_zero();
            if fwd == Vec3::ZERO {
                continue;
            }
            let right = fwd.cross(Vec3::Y).normalize_or_zero();
            let up = right.cross(fwd).normalize_or_zero();
            let len = a.pos.distance(bf.pos) + 0.02;
            let mid = (a.pos + bf.pos) * 0.5;
            let light = self.light_at(mid + Vec3::Y * 0.5);
            for side in [-0.55f32, 0.55] {
                let c = mid + right * side + up * 0.12;
                // (right, up, -fwd) keeps the basis right-handed; with +fwd it was
                // mirrored, which flipped the faces so their fronts were culled.
                let m = Mat4::from_cols(
                    (right * 0.1).extend(0.0),
                    (up * 0.12).extend(0.0),
                    (-fwd * len).extend(0.0),
                    c.extend(1.0),
                );
                push_box(out, &m, T_METAL, [0.9, 0.9, 1.0], light, 0.0);
            }
        }
        for f in track.samples(0.9) {
            let light = self.light_at(f.pos + Vec3::Y * 0.5);
            let c = f.pos + f.up * 0.03;
            let m = Mat4::from_cols(
                (f.right * 1.5).extend(0.0),
                (f.up * 0.08).extend(0.0),
                (-f.fwd * 0.28).extend(0.0),
                c.extend(1.0),
            );
            push_box(out, &m, T_DARKPLANK, [1.0; 3], light, 0.0);
        }
    }
}

/// The moon, its halo and the stars.
pub fn sky_mesh(level: Level) -> Vec<Vertex> {
    let mut out = vec![];
    sky(&mut out, level);
    out
}

fn sky(out: &mut Vec<Vertex>, level: Level) {
    // Moon and stars: drawn unlit and unfogged (alpha 2 flag in the shader).
    let (moon_at, station) = match level {
        // Up and to the left of the house, clear of the cemetery gate as seen from the station.
        Level::Halloween => (Vec3::new(-176.0, 168.0, 203.0), Vec3::new(0.0, 3.0, -22.0)),
        // Up and to the right of the forest, as seen from the station.
        Level::Christmas => (Vec3::new(130.0, 150.0, 240.0), Vec3::new(0.0, 3.0, -36.0)),
    };
    let moon = Mat4::from_translation(moon_at) * Mat4::from_scale(Vec3::splat(34.0));
    for (n, o, r, u) in FACES.iter() {
        let o = Vec3::from(*o) - 0.5;
        let (r, u) = (Vec3::from(*r), Vec3::from(*u));
        let pts = [o, o + r, o + r + u, o + u].map(|p| moon.transform_point3(p));
        let k = 0.85 + 0.15 * (n[1] as f32 * 0.5 + 0.5);
        push_quad(out, pts, tile_uv(T_MOON), [[k, k, k * 0.95, 2.0]; 4]);
    }
    // A blocky two-step halo just behind it, facing the station.
    let to = (moon_at - station).normalize();
    let right = to.cross(Vec3::Y).normalize();
    let up = right.cross(to);
    for (k, (size, col)) in [(70.0, [0.13, 0.12, 0.24]), (52.0, [0.2, 0.19, 0.32])].iter().enumerate() {
        let c = moon_at + to * (40.0 - k as f32 * 8.0);
        let (r, u) = (right * size * 0.5, up * size * 0.5);
        let pts = [c - r - u, c + r - u, c + r + u, c - r + u];
        let pts = [pts[1], pts[0], pts[3], pts[2]];
        push_quad(out, pts, tile_uv(T_WHITE), [[col[0], col[1], col[2], 2.0]; 4]);
    }
    let mut rng = Rng::new(99);
    let center = Vec3::new(0.0, 0.0, 80.0);
    for _ in 0..500 {
        let az = rng.range(0.0, std::f32::consts::TAU);
        let el = rng.range(0.08, 1.45);
        let dir = Vec3::new(az.cos() * el.cos(), el.sin(), az.sin() * el.cos());
        let p = center + dir * 340.0;
        let right = dir.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(dir).normalize_or_zero();
        let s = rng.range(0.8, 2.2);
        let b = rng.range(0.5, 1.0);
        let pts = [
            p - right * s - up * s,
            p - up * s + right * s,
            p + right * s + up * s,
            p + up * s - right * s,
        ];
        // Face toward the centre (reverse winding so it passes back-face culling).
        let pts = [pts[1], pts[0], pts[3], pts[2]];
        push_quad(out, pts, tile_uv(T_STAR), [[b, b, b * 0.9, 2.0]; 4]);
    }
}

// ====================================================================== generation

/// Which part of the ride something is in: an index into its level's sections
/// (`Theme::sections`). The level's own module names them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Zone(pub u8);

impl Zone {
    // Haunted Hollow's.
    pub const GRAVEYARD: Zone = Zone(0);
    pub const FOYER: Zone = Zone(1);
    pub const LIBRARY: Zone = Zone(2);
    pub const DINING: Zone = Zone(3);
    pub const DROP: Zone = Zone(4);
    pub const CRYPT: Zone = Zone(5);
    pub const LAIR: Zone = Zone(6);
    // Holly Hollow's.
    pub const FOREST: Zone = Zone(0);
    pub const VILLAGE: Zone = Zone(1);
    pub const ROOFTOPS: Zone = Zone(2);
    pub const CHIMNEY: Zone = Zone(3);
    pub const FACTORY: Zone = Zone(4);
    pub const LAKE: Zone = Zone(5);
    pub const PLUNGE: Zone = Zone(6);
    pub const CAVERN: Zone = Zone(7);
    pub const THRONE: Zone = Zone(8);
}

/// Which part of Haunted Hollow a point is in (the parts are cut along z).
pub fn zone_at(p: Vec3) -> Zone {
    if p.z < 39.5 {
        Zone::GRAVEYARD
    } else if p.z < 70.5 {
        Zone::FOYER
    } else if p.z < 100.5 {
        Zone::LIBRARY
    } else if p.z < 127.5 && p.y > -0.5 {
        Zone::DINING
    } else if p.z < 140.0 {
        Zone::DROP
    } else if p.z < 176.5 {
        Zone::CRYPT
    } else {
        Zone::LAIR
    }
}

pub fn generate(track: &Track, level: Level) -> World {
    let mut w = World::new(level);
    let mut rng = Rng::new(1031);
    let path: Vec<Vec3> = track.samples(1.0).iter().map(|f| f.pos).collect();
    match level {
        Level::Halloween => halloween::build(&mut w, &mut rng, &path),
        Level::Christmas => christmas::build(&mut w, &mut rng, &path),
    }
    carve(&mut w, track);
    // Torches along the walls of the tunnels the track dives down: (from z, to z, torch).
    let tunnels: &[(f32, f32, u8)] = match level {
        Level::Halloween => &[(127.0, 141.0, b::SOUL_TORCH)],
        Level::Christmas => &[(92.0, 106.0, b::TORCH), (188.0, 201.0, b::FROST_TORCH)],
    };
    after_carve(&mut w, track, tunnels);
    clear_rails(&mut w, track);
    w.compute_light();
    w
}

/// Ground everywhere: `top` over three layers of dirt over stone.
fn ground(w: &mut World, top: u8) {
    let ([x0, y0, z0], [x1, _, z1]) = (w.lo, w.hi);
    for z in z0..z1 {
        for x in x0..x1 {
            for y in y0..=0 {
                let id = if y == 0 {
                    top
                } else if y >= -3 {
                    b::DIRT
                } else {
                    b::STONE
                };
                w.set(x, y, z, id);
            }
        }
    }
}

fn near_track(path: &[Vec3], x: f32, z: f32, r: f32) -> bool {
    path.iter().any(|p| (p.x - x).powi(2) + (p.z - z).powi(2) < r * r && p.y > -2.0)
}

/// Face index (into FACES) pointing from (x,z) toward the target.
fn facing_toward(x: f32, z: f32, tx: f32, tz: f32) -> u8 {
    let dx = tx - x;
    let dz = tz - z;
    if dx.abs() > dz.abs() {
        if dx > 0.0 { 0 } else { 1 }
    } else if dz > 0.0 {
        4
    } else {
        5
    }
}

/// Door opening geometry (shared with the door models in mesh.rs): the doorway is
/// `DOOR_HALF` wide either side of its middle, straight up `DOOR_SPRING` from the floor,
/// then a round arch. The doors face along +z.
pub const DOOR_HALF: f32 = 5.0;
pub const DOOR_SPRING: f32 = 8.0;

/// Where a level's boss doors stand: the middle of the doorway's floor.
#[derive(Clone, Copy, Debug)]
pub struct Doors {
    pub x: f32,
    pub floor: f32,
    pub z: f32,
}

impl Doors {
    /// The point (x, y) of the door's plane is in the doorway (grown by `grow`).
    pub fn contains(&self, x: f32, y: f32, grow: f32) -> bool {
        in_doorway(x - self.x, y - self.floor, grow)
    }

    /// Middle of the doorway, for the camera to look at.
    pub fn centre(&self) -> Vec3 {
        Vec3::new(self.x, self.floor + 6.5, self.z)
    }
}

/// In the doorway: `x` from its middle, `y` up from its floor.
pub fn in_doorway(x: f32, y: f32, grow: f32) -> bool {
    let r = DOOR_HALF + grow;
    y >= -grow && x.abs() <= r && (y <= DOOR_SPRING || x * x + (y - DOOR_SPRING).powi(2) <= r * r)
}

/// Clears the ride's tunnel along the track (doorways appear wherever it crosses walls).
fn carve(w: &mut World, track: &Track) {
    for f in track.samples(0.25) {
        let right = Vec3::new(f.right.x, 0.0, f.right.z).normalize_or_zero();
        for ai in -10..=10 {
            let a = ai as f32 * 0.25;
            for hi in 0..=9 {
                let h = hi as f32 * 0.5;
                let p = f.pos + right * a + Vec3::Y * (h + 0.05);
                w.set(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32, b::AIR);
            }
        }
    }
}

/// Trestles under the raised track, and torches along the walls of the tunnels it dives
/// down (`tunnels`: from z, to z, which torch).
fn after_carve(w: &mut World, track: &Track, tunnels: &[(f32, f32, u8)]) {
    // Wooden trestles under elevated track sections (not on the steep drop, where a
    // post under one sample would poke up through the rails just ahead of it).
    for f in track.samples(2.0) {
        if f.fwd.y.abs() > 0.5 {
            continue;
        }
        let (x, z) = (f.pos.x.floor() as i32, f.pos.z.floor() as i32);
        let mut y = (f.pos.y - 0.05).floor() as i32 - 1;
        if !w.is_air(x, y, z) {
            continue;
        }
        let mut n = 0;
        while w.is_air(x, y, z) && n < 14 {
            w.set(x, y, z, b::LOG);
            y -= 1;
            n += 1;
        }
    }
    // Torches along the drop tunnel.
    let mut side = 1.0;
    for f in track.samples(3.0) {
        if let Some(&(_, _, torch)) = tunnels.iter().find(|t| f.pos.z > t.0 && f.pos.z < t.1) {
            // Walk out from the track until we reach a wall and mount the torch on it
            // (no wall close by: no torch, rather than one floating in mid-air).
            let right = Vec3::new(f.right.x, 0.0, f.right.z).normalize_or_zero();
            let mut step = 2.0;
            while step < 6.0 {
                let p = f.pos + right * (step * side) + Vec3::Y * 2.2;
                let next = p + right * (0.6 * side);
                let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
                if w.solid_at(next) {
                    let on_wall = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dz)| props(w.get(x + dx, y, z + dz)).opaque);
                    if w.is_air(x, y, z) && on_wall {
                        w.set(x, y, z, torch);
                    }
                    break;
                }
                step += 0.5;
            }
            side = -side;
        }
    }
}

/// Last pass: nothing may poke up through the rails or sleepers anywhere on the ride
/// (corners of blocks under steep sections, the tops of support posts...).
fn clear_rails(w: &mut World, track: &Track) {
    for f in track.samples(0.1) {
        for side in [-0.65f32, -0.5, -0.25, 0.0, 0.25, 0.5, 0.65] {
            for h in [0.06f32, 0.15, 0.24] {
                let p = f.pos + f.right * side + f.up * h;
                if w.occupied(p) {
                    w.set(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32, b::AIR);
                }
            }
        }
    }
}

fn scatter(w: &mut World, rng: &mut Rng, n: usize, xr: (i32, i32), y: i32, zr: (i32, i32), floor_ok: &[u8], pick: &dyn Fn(&mut Rng) -> u8) {
    for _ in 0..n {
        let x = rng.int(xr.0, xr.1);
        let z = rng.int(zr.0, zr.1);
        if w.is_air(x, y, z) && floor_ok.contains(&w.get(x, y - 1, z)) {
            let id = pick(rng);
            w.set(x, y, z, id);
        }
    }
}

fn float_candles(w: &mut World, rng: &mut Rng, n: usize, xr: (i32, i32), yr: (i32, i32), zr: (i32, i32)) {
    for _ in 0..n {
        let (x, y, z) = (rng.int(xr.0, xr.1), rng.int(yr.0, yr.1), rng.int(zr.0, zr.1));
        if w.is_air(x, y, z) && w.is_air(x, y - 1, z) {
            w.set(x, y, z, b::CANDLE);
        }
    }
}

/// Hangs a `banner` (two blocks tall, at heights `ys`) every `step` blocks along x = `x`.
fn banners(w: &mut World, x: i32, ys: (i32, i32), zr: (i32, i32), step: usize, banner: u8) {
    for z in (zr.0..=zr.1).step_by(step) {
        if w.is_air(x, ys.1, z) && w.is_air(x, ys.0, z) {
            w.set(x, ys.0, z, banner);
            w.set(x, ys.1, z, banner);
        }
    }
}

/// True if a wall spot is close to the ride (the tunnel carve would cut a decoration in half).
fn near_ride(path: &[Vec3], x: i32, y: i32, z: i32) -> bool {
    let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
    path.iter().any(|q| (q.x - p.x).powi(2) + (q.z - p.z).powi(2) < 4.0f32.powi(2) && (p.y - q.y) > -3.0 && (p.y - q.y) < 7.0)
}

/// The blocks a level hangs on its walls (see `decorate_walls`).
struct WallDeco {
    /// Paintings, each in a gold frame.
    art: [u8; 3],
    /// The crest in the middle of a crest-and-banners group, and whether it replaces a
    /// block of the wall (a shield) or hangs in front of it (a wreath).
    crest: (u8, bool),
    /// Hung either side of the crest, and whether they're two blocks tall.
    banner: (u8, bool),
    /// Beside mirrors and clocks.
    sconce: u8,
    /// On the wall shelves (left, middle, right).
    shelf: [u8; 3],
}

/// Hangs decorations on a room's bare walls: gold-framed paintings, a crest between
/// banners, mirrors, shelves with things on them, sconces and clocks. `x0..x1`/`z0..z1`
/// are the wall coordinates, `y0` the floor.
#[allow(clippy::too_many_arguments)]
fn decorate_walls(w: &mut World, rng: &mut Rng, path: &[Vec3], deco: &WallDeco, x0: i32, x1: i32, y0: i32, y1: i32, z0: i32, z1: i32, wall: u8, bands: &[i32]) {
    // (wall line start, tangent, inward normal, length)
    let walls = [
        ((x0, z0 + 1), (0, 1), (1, 0), z1 - z0 - 1),
        ((x1, z0 + 1), (0, 1), (-1, 0), z1 - z0 - 1),
        ((x0 + 1, z0), (1, 0), (0, 1), x1 - x0 - 1),
        ((x0 + 1, z1), (1, 0), (0, -1), x1 - x0 - 1),
    ];
    for &((sx, sz), (tx, tz), (nx, nz), len) in &walls {
        for &band in bands {
            let y = y0 + band;
            if y + 2 >= y1 {
                continue;
            }
            let mut u = 2 + rng.int(0, 2);
            while u < len - 2 {
                let at = |du: i32| (sx + tx * (u + du), sz + tz * (u + du));
                // Every cell this item touches must be bare wall with open air in front.
                let free = |w: &World, du: i32, dy: i32| -> bool {
                    let (x, z) = at(du);
                    w.get(x, y + dy, z) == wall && w.is_air(x + nx, y + dy, z + nz) && !near_ride(path, x, y + dy, z)
                };
                let pick = rng.int(0, 5);
                let placed = match pick {
                    // Gold-framed portrait or painting (3x3).
                    0 | 1 => {
                        if (-1..=1).all(|du| (-1..=1).all(|dy| free(w, du, dy))) {
                            for du in -1..=1 {
                                for dy in -1..=1 {
                                    let (x, z) = at(du);
                                    w.set(x, y + dy, z, b::GOLD);
                                }
                            }
                            let (x, z) = at(0);
                            let art = deco.art[rng.int(0, 2) as usize];
                            w.set(x, y, z, art);
                            true
                        } else {
                            false
                        }
                    }
                    // A crest between two hanging banners.
                    2 => {
                        if (-1..=1).all(|du| free(w, du, 0) && free(w, du, -1)) {
                            let (x, z) = at(0);
                            match deco.crest {
                                (crest, true) => w.set(x, y, z, crest),
                                (crest, false) => w.set(x + nx, y, z + nz, crest),
                            }
                            for du in [-1, 1] {
                                let (x, z) = at(du);
                                w.set(x + nx, y, z + nz, deco.banner.0);
                                if deco.banner.1 {
                                    w.set(x + nx, y - 1, z + nz, deco.banner.0);
                                }
                            }
                            true
                        } else {
                            false
                        }
                    }
                    // Tall gold mirror with a candle sconce either side.
                    3 => {
                        if (-1..=2).all(|dy| free(w, 0, dy)) && free(w, -1, 0) && free(w, 1, 0) {
                            let (x, z) = at(0);
                            w.set(x, y - 1, z, b::GOLD);
                            w.set(x, y, z, b::MIRROR);
                            w.set(x, y + 1, z, b::MIRROR);
                            w.set(x, y + 2, z, b::GOLD);
                            for du in [-1, 1] {
                                let (x, z) = at(du);
                                w.set(x + nx, y, z + nz, deco.sconce);
                            }
                            true
                        } else {
                            false
                        }
                    }
                    // Wall shelf with pots and a candle on it.
                    4 => {
                        if (-1..=1).all(|du| free(w, du, 0) && free(w, du, 1)) {
                            for du in -1..=1 {
                                let (x, z) = at(du);
                                w.set(x + nx, y, z + nz, b::DARKPLANK);
                                let item = deco.shelf[(du + 1) as usize];
                                w.set(x + nx, y + 1, z + nz, item);
                            }
                            true
                        } else {
                            false
                        }
                    }
                    // A clock face with torches either side.
                    _ => {
                        if (-1..=1).all(|du| free(w, du, 0)) {
                            let (x, z) = at(0);
                            w.set(x, y, z, b::CLOCK);
                            for du in [-1, 1] {
                                let (x, z) = at(du);
                                w.set(x + nx, y, z + nz, deco.sconce);
                            }
                            true
                        } else {
                            false
                        }
                    }
                };
                u += if placed { 4 + rng.int(0, 1) } else { 1 };
            }
        }
    }
}
