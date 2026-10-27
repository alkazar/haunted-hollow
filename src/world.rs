//! The voxel world: a graveyard, a haunted mansion (foyer, library, dining hall),
//! a crypt beneath it and the Pumpkin King's lair. Includes Minecraft-style
//! flood-fill block lighting (coloured) and moonlight, plus mesh generation.

use crate::atlas::*;
use crate::mesh::{push_box, push_box_tiles, push_quad, shade, FACES};
use crate::rng::Rng;
use crate::track::Track;
use crate::vk::Vertex;
use glam::{Mat4, Vec3};
use std::collections::VecDeque;

pub const X0: i32 = -40;
pub const X1: i32 = 40;
pub const Y0: i32 = -32;
pub const Y1: i32 = 44;
pub const Z0: i32 = -32;
pub const Z1: i32 = 300;
const SX: i32 = X1 - X0;
const SY: i32 = Y1 - Y0;
const SZ: i32 = Z1 - Z0;

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
        _ => Props { opaque: false, shape: Shape::Cube, tiles: [0; 3], emit: [0; 3], glow: false, size: [1.0; 3] },
    }
}

pub struct World {
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
    fn new() -> World {
        let n = (SX * SY * SZ) as usize;
        World { blocks: vec![0; n], meta: vec![0; n], light: vec![[0; 4]; n] }
    }

    #[inline]
    fn idx(x: i32, y: i32, z: i32) -> Option<usize> {
        if x < X0 || x >= X1 || y < Y0 || y >= Y1 || z < Z0 || z >= Z1 {
            None
        } else {
            Some((((z - Z0) * SY + (y - Y0)) * SX + (x - X0)) as usize)
        }
    }

    pub fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        match Self::idx(x, y, z) {
            Some(i) => self.blocks[i],
            None => {
                if y < Y0 { b::STONE } else { b::AIR }
            }
        }
    }

    /// Positions (block bottom centres) of every block of kind `id`, for things animated per frame.
    pub fn cells_of(&self, id: u8) -> Vec<Vec3> {
        let mut out = vec![];
        for z in Z0..Z1 {
            for y in Y0..Y1 {
                for x in X0..X1 {
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
        props(id).opaque || matches!(id, b::FENCE | b::LANTERN | b::CAULDRON | b::POT | b::CAKE | b::BANNER)
    }

    fn set(&mut self, x: i32, y: i32, z: i32, id: u8) {
        if let Some(i) = Self::idx(x, y, z) {
            self.blocks[i] = id;
            self.meta[i] = 0;
        }
    }

    fn set_meta(&mut self, x: i32, y: i32, z: i32, id: u8, m: u8) {
        if let Some(i) = Self::idx(x, y, z) {
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
        match Self::idx(x, y, z) {
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
        for z in Z0..Z1 {
            for x in X0..X1 {
                for y in (Y0..Y1).rev() {
                    let i = Self::idx(x, y, z).unwrap();
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
        let strides: [isize; 6] = [1, -1, SX as isize, -(SX as isize), (SX * SY) as isize, -((SX * SY) as isize)];
        while let Some(i) = q.pop_front() {
            let l = self.light[i][ch];
            if l <= 1 {
                continue;
            }
            let x = (i as i32) % SX;
            let y = ((i as i32) / SX) % SY;
            let z = (i as i32) / (SX * SY);
            for (k, s) in strides.iter().enumerate() {
                let ok = match k {
                    0 => x + 1 < SX,
                    1 => x > 0,
                    2 => y + 1 < SY,
                    3 => y > 0,
                    4 => z + 1 < SZ,
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
        let mut out = Vec::with_capacity(1 << 20);
        for z in Z0..Z1 {
            for y in Y0..Y1 {
                for x in X0..X1 {
                    let i = Self::idx(x, y, z).unwrap();
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
        sky(&mut out);
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
                let flame = if id == b::SOUL_TORCH { [0.45, 1.0, 0.85] } else { [1.0, 0.78, 0.3] };
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
                push_box(out, &m(c + dir * 0.45 + Vec3::Y * 0.5, size), T_BANNER, [1.0; 3], light, 0.1);
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

fn sky(out: &mut Vec<Vertex>) {
    // Moon and stars: drawn unlit and unfogged (alpha 2 flag in the shader).
    // The moon hangs up and to the left of the house, clear of the cemetery gate as seen from the station.
    let moon_at = Vec3::new(-176.0, 168.0, 203.0);
    let moon = Mat4::from_translation(moon_at) * Mat4::from_scale(Vec3::splat(34.0));
    for (n, o, r, u) in FACES.iter() {
        let o = Vec3::from(*o) - 0.5;
        let (r, u) = (Vec3::from(*r), Vec3::from(*u));
        let pts = [o, o + r, o + r + u, o + u].map(|p| moon.transform_point3(p));
        let k = 0.85 + 0.15 * (n[1] as f32 * 0.5 + 0.5);
        push_quad(out, pts, tile_uv(T_MOON), [[k, k, k * 0.95, 2.0]; 4]);
    }
    // A blocky two-step halo just behind it, facing the station.
    let station = Vec3::new(0.0, 3.0, -22.0);
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

/// Which part of the ride a z coordinate belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Zone {
    Graveyard,
    Foyer,
    Library,
    Dining,
    Drop,
    Crypt,
    Lair,
}

pub fn zone_at(p: Vec3) -> Zone {
    if p.z < 39.5 {
        Zone::Graveyard
    } else if p.z < 70.5 {
        Zone::Foyer
    } else if p.z < 100.5 {
        Zone::Library
    } else if p.z < 127.5 && p.y > -0.5 {
        Zone::Dining
    } else if p.z < 140.0 {
        Zone::Drop
    } else if p.z < 176.5 {
        Zone::Crypt
    } else {
        Zone::Lair
    }
}

pub fn generate(track: &Track) -> World {
    let mut w = World::new();
    let mut rng = Rng::new(1031);

    // Ground.
    for z in Z0..Z1 {
        for x in X0..X1 {
            for y in Y0..=0 {
                let id = if y == 0 {
                    b::GRASS
                } else if y >= -3 {
                    b::DIRT
                } else {
                    b::STONE
                };
                w.set(x, y, z, id);
            }
        }
    }
    let path: Vec<Vec3> = track.samples(1.0).iter().map(|f| f.pos).collect();
    graveyard(&mut w, &mut rng, &path);
    house(&mut w, &mut rng);
    crypt(&mut w, &mut rng);
    lair(&mut w, &mut rng);
    decorate(&mut w, &mut rng, &path);
    boss_doors(&mut w);
    // Hang decorations on the bare walls of every indoor room.
    decorate_walls(&mut w, &mut rng, &path, -16, 16, 0, 18, 40, 70, b::WALL_PURPLE, &[4, 9, 14]);
    decorate_walls(&mut w, &mut rng, &path, -16, 16, 0, 18, 71, 100, b::WALL_GREEN, &[4, 9, 14]);
    decorate_walls(&mut w, &mut rng, &path, -16, 16, 0, 18, 101, 130, b::STONEBRICK, &[4, 9, 14]);
    decorate_walls(&mut w, &mut rng, &path, -14, 14, -18, -6, 140, 176, b::CRYPT, &[4, 8]);
    decorate_walls(&mut w, &mut rng, &path, -26, 26, -18, -1, 177, 214, b::PURPLE, &[5, 10]);
    carve(&mut w, track);
    after_carve(&mut w, track);
    clear_rails(&mut w, track);
    w.compute_light();
    w
}

/// A pumpkin can go here: open air, and no other pumpkin right next to it (pumpkins
/// side by side merge into one long-looking pumpkin).
fn pumpkin_spot_free(w: &World, x: i32, y: i32, z: i32) -> bool {
    w.is_air(x, y, z) && (-1..=1).all(|dx| (-1..=1).all(|dz| w.get(x + dx, y, z + dz) != b::PUMPKIN))
}

/// A fenced graveyard plot (fence at x0..x1 / z0..z1) with a two-wide gate at `gate`
/// on the fence line, and rows of single headstones facing direction `face` (a cube
/// face index: 0 +X, 1 -X, 4 +Z, 5 -Z). Each grave gets a dirt plot in front of it.
#[allow(clippy::too_many_arguments)]
fn grave_plot(w: &mut World, rng: &mut Rng, path: &[Vec3], x0: i32, x1: i32, z0: i32, z1: i32, face: u8, gate: (i32, i32)) {
    let (fx, fz) = match face {
        0 => (1, 0),
        1 => (-1, 0),
        4 => (0, 1),
        _ => (0, -1),
    };
    let along_z = fx != 0; // gate in a fence that runs along z?
    let is_gate = |x: i32, z: i32| {
        if along_z {
            x == gate.0 && (z == gate.1 || z == gate.1 + 1)
        } else {
            z == gate.1 && (x == gate.0 || x == gate.0 + 1)
        }
    };
    // Fence, leaving the gate open; tall gateposts topped with lanterns.
    for z in z0..=z1 {
        for x in x0..=x1 {
            let edge = x == x0 || x == x1 || z == z0 || z == z1;
            if !edge || is_gate(x, z) || near_track(path, x as f32 + 0.5, z as f32 + 0.5, 3.2) || !w.is_air(x, 1, z) {
                continue;
            }
            w.set(x, 1, z, b::FENCE);
        }
    }
    let posts = if along_z { [(gate.0, gate.1 - 1), (gate.0, gate.1 + 2)] } else { [(gate.0 - 1, gate.1), (gate.0 + 2, gate.1)] };
    for (px, pz) in posts {
        w.fill(px, px, 1, 2, pz, pz, b::FENCE);
        w.set(px, 3, pz, b::LANTERN);
    }
    // Rows of headstones (2 apart along a row, rows 3 apart).
    let (row_step, col_step) = (3, 2);
    let rows: Vec<i32> = if along_z { (x0 + 2..x1 - 1).step_by(row_step).collect() } else { (z0 + 2..z1 - 1).step_by(row_step).collect() };
    let cols: Vec<i32> = if along_z { (z0 + 2..z1 - 1).step_by(col_step).collect() } else { (x0 + 2..x1 - 1).step_by(col_step).collect() };
    for &r in &rows {
        for &c in &cols {
            let (x, z) = if along_z { (r, c) } else { (c, r) };
            if rng.chance(0.15) || near_track(path, x as f32 + 0.5, z as f32 + 0.5, 4.0) || !w.is_air(x, 1, z) {
                continue;
            }
            w.set_meta(x, 1, z, b::GRAVE, face);
            // Dirt plot in front, sometimes with a jack-o'-lantern or a mushroom.
            let (dx, dz) = (x + fx, z + fz);
            if w.get(dx, 0, dz) == b::GRASS && w.is_air(dx, 1, dz) {
                w.set(dx, 0, dz, b::DIRT);
                if rng.chance(0.12) && pumpkin_spot_free(w, dx, 1, dz) {
                    w.set_meta(dx, 1, dz, b::PUMPKIN, face);
                } else if rng.chance(0.15) {
                    w.set(dx, 1, dz, b::MUSHROOM);
                }
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

fn graveyard(w: &mut World, rng: &mut Rng, path: &[Vec3]) {
    // Cemetery gate with pumpkins on top.
    w.fill(-6, -5, 1, 7, -10, -9, b::STONEBRICK);
    w.fill(5, 6, 1, 7, -10, -9, b::STONEBRICK);
    w.fill(-6, 6, 8, 8, -10, -9, b::STONEBRICK);
    w.set_meta(-6, 9, -10, b::PUMPKIN, 5);
    w.set_meta(6, 9, -10, b::PUMPKIN, 5);
    w.set_meta(0, 9, -10, b::PUMPKIN, 5);
    // Iron fence around the cemetery.
    for z in -26..=37 {
        for x in [-24, 24] {
            w.fill(x, x, 1, 2, z, z, b::FENCE);
        }
    }
    for x in -24..=24i32 {
        if x.abs() > 6 {
            w.fill(x, x, 1, 2, -10, -10, b::FENCE);
        }
    }
    // Station platform with hay and pumpkins.
    w.fill(3, 6, 1, 1, -28, -15, b::DARKPLANK);
    w.fill(-6, -3, 1, 1, -28, -15, b::DARKPLANK);
    for z in [-26, -20] {
        for x in [5, -5] {
            w.fill(x, x, 2, 4, z, z, b::FENCE);
            w.set(x, 5, z, b::LANTERN);
        }
    }
    w.fill(5, 6, 2, 2, -17, -16, b::HAY);
    w.set(6, 3, -17, b::HAY);
    w.set_meta(5, 3, -16, b::PUMPKIN, 1);
    w.set_meta(-5, 2, -18, b::PUMPKIN, 0);
    w.fill(-6, -6, 2, 2, -24, -22, b::HAY);

    // Three small fenced graveyard plots with lantern-lit gates and neat rows of
    // headstones, their "RIP" faces turned toward the ride.
    grave_plot(w, rng, path, 8, 20, 7, 23, 1, (8, 14));
    grave_plot(w, rng, path, -23, -13, -7, 13, 0, (-13, 2));
    grave_plot(w, rng, path, -10, 0, 26, 32, 5, (-5, 26));
    // Extra jack-o'-lanterns lining the ride.
    let mut d = 6.0;
    while d < 60.0 {
        let i = (d as usize).min(path.len() - 2);
        let p = path[i];
        if p.z > 37.0 {
            break;
        }
        let fwd = (path[i + 1] - p).normalize_or_zero();
        let right = fwd.cross(Vec3::Y).normalize_or_zero();
        let side = if (d as i32 / 7) % 2 == 0 { 3.3 } else { -3.3 };
        let q = p + right * side;
        let (x, z) = (q.x.floor() as i32, q.z.floor() as i32);
        if pumpkin_spot_free(w, x, 1, z) {
            w.set_meta(x, 1, z, b::PUMPKIN, facing_toward(q.x, q.z, p.x, p.z));
        }
        // Lamp posts.
        let q2 = p - right * side * 1.1;
        let (x2, z2) = (q2.x.floor() as i32, q2.z.floor() as i32);
        if (d as i32) % 14 == 6 && w.is_air(x2, 1, z2) {
            w.fill(x2, x2, 1, 3, z2, z2, b::FENCE);
            w.set(x2, 4, z2, b::LANTERN);
        }
        d += 7.0;
    }
    // Dead trees.
    for &(tx, tz, h) in &[(-17, 4, 8), (15, 10, 7), (-12, 30, 9), (18, 31, 6), (-20, -4, 7), (12, -3, 6)] {
        w.fill(tx, tx, 1, h, tz, tz, b::LOG);
        for &(dx, dz, len) in &[(1, 0, 3), (-1, 1, 2), (0, -1, 3), (-1, 0, 2)] {
            let mut x = tx;
            let mut z = tz;
            let mut y = h - 2 + (dx + dz as i32).abs() % 2;
            for _ in 0..len {
                x += dx;
                z += dz;
                y += 1;
                w.set(x, y, z, b::LOG);
            }
            if rng.chance(0.4) {
                w.set(x, y - 1, z, b::WEB);
            }
        }
    }
    // A little mausoleum.
    w.room(-22, -15, 0, 7, 18, 26, b::STONEBRICK, b::MOSSY, b::STONEBRICK);
    w.fill(-23, -14, 8, 8, 17, 27, b::STONEBRICK);
    w.fill(-21, -16, 9, 9, 18, 26, b::STONEBRICK);
    w.fill(-15, -15, 1, 3, 21, 23, b::AIR);
    w.set(-14, 3, 20, b::SOUL_TORCH);
    w.set(-14, 3, 24, b::SOUL_TORCH);
    w.fill(-15, -15, 4, 4, 21, 23, b::COBBLE);
}

fn house(w: &mut World, rng: &mut Rng) {
    // Outer shell and rooms (walls are 2 thick: dark planks outside, wallpaper inside).
    w.room(-17, 17, 0, 19, 39, 131, b::DARKPLANK, b::DARKPLANK, b::DARKPLANK);
    for z in (39..=131).step_by(8) {
        w.fill(-17, -17, 1, 19, z, z, b::LOG);
        w.fill(17, 17, 1, 19, z, z, b::LOG);
    }
    for x in (-17..=17).step_by(6) {
        w.fill(x, x, 1, 19, 39, 39, b::LOG);
        w.fill(x, x, 1, 19, 131, 131, b::LOG);
    }
    w.room(-16, 16, 0, 18, 40, 70, b::WALL_PURPLE, b::DARKPLANK, b::DARKPLANK);
    w.room(-16, 16, 0, 18, 71, 100, b::WALL_GREEN, b::PLANK, b::DARKPLANK);
    w.room(-16, 16, 0, 18, 101, 130, b::STONEBRICK, b::PLANK, b::DARKPLANK);

    // Stepped roof.
    for k in 0..19 {
        let y = 20 + k;
        let (x0, x1) = (-18 + k, 18 - k);
        if x0 > x1 {
            break;
        }
        w.fill(x0, x1, y, y, 38, 132, b::ROOF);
    }
    // Chimney and corner towers.
    w.fill(9, 11, 20, 34, 110, 112, b::STONEBRICK);
    for &tx in &[-19, 19] {
        let tz = 37;
        for y in 0..=30 {
            for dz in -3..=3i32 {
                for dx in -3..=3i32 {
                    let r = ((dx * dx + dz * dz) as f32).sqrt();
                    if r <= 3.4 {
                        let id = if r > 2.4 {
                            if y % 7 == 4 && (dx == 0 || dz == 0) { b::WINDOW_LIT } else { b::STONEBRICK }
                        } else {
                            b::STONEBRICK
                        };
                        w.set(tx + dx, y, tz + dz, id);
                    }
                }
            }
        }
        for k in 0..9 {
            let r = 4.4 - k as f32 * 0.55;
            for dz in -5..=5i32 {
                for dx in -5..=5i32 {
                    if ((dx * dx + dz * dz) as f32).sqrt() <= r {
                        w.set(tx + dx, 31 + k, tz + dz, b::ROOF);
                    }
                }
            }
        }
        w.set_meta(tx, 40, tz, b::PUMPKIN, 5);
    }
    // Windows: some glowing, some dark (with matching inner window so moonlight leaks in).
    for &x in &[-13, -12, -8, -7, 7, 8, 12, 13] {
        for &y0 in &[4, 11] {
            let lit = (x + y0) % 3 != 0;
            w.fill(x, x, y0, y0 + 2, 39, 39, if lit { b::WINDOW_LIT } else { b::WINDOW });
            if !lit {
                w.fill(x, x, y0, y0 + 2, 40, 40, b::WINDOW);
            }
        }
    }
    for z in (44..127).step_by(7) {
        for &y0 in &[4, 11] {
            for &x in &[-17, 17] {
                let lit = rng.chance(0.55);
                w.fill(x, x, y0, y0 + 2, z, z + 1, if lit { b::WINDOW_LIT } else { b::WINDOW });
                if !lit {
                    let ix = if x < 0 { -16 } else { 16 };
                    w.fill(ix, ix, y0, y0 + 2, z, z + 1, b::WINDOW);
                }
            }
        }
    }
    // Porch (wide enough that the ride's tunnel, which swings in from x = 5, misses the
    // pillars and lanterns).
    w.fill(-8, 8, 8, 8, 34, 38, b::ROOF);
    for &x in &[-8, 8] {
        w.fill(x, x, 1, 7, 34, 34, b::LOG);
        let lx = x - x.signum();
        w.fill(lx, lx, 6, 7, 36, 36, b::FENCE);
        w.set(lx, 5, 36, b::LANTERN);
    }

    // ---------------- Foyer
    w.fill(-3, 3, 0, 0, 41, 69, b::CARPET);
    for i in 0..8 {
        let z = 44 + i * 2;
        w.fill(9, 15, 1, 1 + i, z, z + 1, b::STONEBRICK);
    }
    w.fill(9, 15, 9, 9, 60, 69, b::DARKPLANK);
    w.fill(9, 9, 10, 10, 60, 69, b::FENCE);
    for &z in &[60, 64, 69] {
        w.fill(9, 9, 1, 8, z, z, b::LOG);
    }
    for &z in &[46, 56, 66] {
        w.fill(-13, -13, 1, 17, z, z, b::LOG);
    }
    // Chandelier.
    w.fill(2, 2, 13, 17, 55, 55, b::FENCE);
    for (dx, dz) in [(0, 0), (2, 0), (-2, 0), (0, 2), (0, -2)] {
        w.set(2 + dx, 12, 55 + dz, b::LANTERN);
        if dx != 0 || dz != 0 {
            w.set(2 + dx, 13, 55 + dz, b::FENCE);
        }
    }
    w.fill(0, 4, 14, 14, 55, 55, b::FENCE);
    w.fill(2, 2, 14, 14, 53, 57, b::FENCE);
    // Portraits and torches on the walls.
    for z in (43..69).step_by(4) {
        w.set(-16, 6, z, if z % 8 == 3 { b::PORTRAIT } else { b::PORTRAIT2 });
        w.set(-16, 9, z + 2, b::PORTRAIT);
        if w.is_air(-15, 5, z + 2) {
            w.set(-15, 5, z + 2, b::TORCH);
        }
        if w.is_air(15, 11, z) {
            w.set(15, 11, z, b::TORCH);
        }
    }
    webs(w, rng, -15, 15, 17, 41, 69);

    // ---------------- Library
    for z in 72..100 {
        for &x in &[-15, 15] {
            if z % 7 == 0 {
                if w.is_air(x, 5, z) {
                    w.set(x, 5, z, b::TORCH);
                }
                continue;
            }
            w.fill(x, x, 1, 11, z, z, b::BOOKSHELF);
        }
    }
    w.fill(-14, 14, 1, 9, 99, 99, b::BOOKSHELF);
    w.fill(-14, 14, 1, 9, 72, 72, b::BOOKSHELF);
    for &z in &[77, 83, 89, 95] {
        w.fill(-12, -6, 1, 4, z, z, b::BOOKSHELF);
        w.fill(4, 10, 1, 4, z + 2, z + 2, b::BOOKSHELF);
    }
    for &(x, z) in &[(-2, 76), (1, 85), (-3, 93), (12, 80), (-10, 86)] {
        w.fill(x, x + 1, 1, 1, z, z + 1, b::PLANK);
        w.set(x, 2, z, b::CANDLE);
        w.set(x + 1, 2, z + 1, b::CANDLE);
    }
    for &(x, z) in &[(-6, 79), (6, 87), (-4, 96), (10, 92)] {
        w.fill(x, x, 14, 17, z, z, b::FENCE);
        w.set(x, 13, z, b::LANTERN);
    }
    webs(w, rng, -14, 14, 17, 72, 99);

    // ---------------- Dining hall
    w.fill(-13, -6, 0, 0, 103, 128, b::CARPET);
    w.fill(-11, -8, 1, 1, 105, 125, b::CLOTH);
    for z in (106..125).step_by(2) {
        w.set(-12, 1, z, b::DARKPLANK);
        w.set(-13, 2, z, b::DARKPLANK);
        w.set(-7, 1, z, b::DARKPLANK);
        w.set(-6, 2, z, b::DARKPLANK);
    }
    for z in (106..125).step_by(3) {
        w.set(-10, 2, z, b::CANDLE);
        if z % 2 == 0 {
            w.set(-9, 2, z + 1, b::CANDLE);
        }
    }
    w.set_meta(-9, 2, 112, b::PUMPKIN, 0);
    w.set_meta(-10, 2, 121, b::PUMPKIN, 1);
    // Fireplace.
    w.fill(13, 15, 1, 8, 110, 118, b::COBBLE);
    w.fill(13, 14, 1, 4, 112, 116, b::AIR);
    for z in 112..=116 {
        w.set(14, 1, z, b::FIRE);
    }
    w.fill(12, 12, 9, 9, 110, 118, b::DARKPLANK);
    w.set(12, 10, 111, b::CANDLE);
    w.set(12, 10, 117, b::CANDLE);
    w.set_meta(12, 10, 114, b::PUMPKIN, 1);
    for &z in &[110, 120] {
        w.fill(-9, -9, 14, 17, z, z, b::FENCE);
        for (dx, dz) in [(0, 0), (2, 0), (-2, 0), (0, 2), (0, -2)] {
            w.set(-9 + dx, 13, z + dz, b::LANTERN);
        }
        w.fill(-11, -7, 14, 14, z, z, b::FENCE);
        w.fill(-9, -9, 14, 14, z - 2, z + 2, b::FENCE);
    }
    for z in (104..129).step_by(5) {
        w.set(-16, 7, z, b::PORTRAIT2);
        if w.is_air(-15, 5, z + 2) {
            w.set(-15, 5, z + 2, b::TORCH);
        }
        if w.is_air(15, 6, z) && !(108..=120).contains(&z) {
            w.set(15, 6, z, b::TORCH);
        }
    }
    webs(w, rng, -15, 15, 17, 101, 130);
}

fn webs(w: &mut World, rng: &mut Rng, x0: i32, x1: i32, y: i32, z0: i32, z1: i32) {
    for &(x, z) in &[(x0, z0), (x1, z0), (x0, z1), (x1, z1)] {
        for _ in 0..6 {
            let (dx, dy, dz) = (rng.int(0, 2), rng.int(0, 2), rng.int(0, 2));
            let px = if x == x0 { x + dx } else { x - dx };
            let pz = if z == z0 { z + dz } else { z - dz };
            if w.is_air(px, y - dy, pz) {
                w.set(px, y - dy, pz, b::WEB);
            }
        }
    }
    for _ in 0..10 {
        let x = rng.int(x0, x1);
        let z = rng.int(z0, z1);
        if w.is_air(x, y, z) {
            w.set(x, y, z, b::WEB);
        }
    }
}

fn crypt(w: &mut World, rng: &mut Rng) {
    w.room(-14, 14, -18, -6, 140, 176, b::CRYPT, b::MOSSY, b::CRYPT);
    for z in (146..=170).step_by(8) {
        for &x in &[-9, 9] {
            w.fill(x, x, -17, -7, z, z, b::STONEBRICK);
            w.set(x, -13, z - 1, b::SOUL_TORCH);
            w.set(x, -13, z + 1, b::SOUL_TORCH);
        }
    }
    for z in (143..174).step_by(6) {
        for &x in &[-13, 12] {
            w.fill(x, x + 1, -17, -17, z, z + 2, b::DARKPLANK);
            w.fill(x, x + 1, -16, -16, z, z + 2, b::DARKPLANK);
            if rng.chance(0.4) {
                w.set(x, -15, z + 1, b::CANDLE);
            }
        }
        if w.is_air(-13, -12, z + 4) {
            w.set(-13, -12, z + 4, b::SOUL_TORCH);
        }
        if w.is_air(13, -12, z + 4) {
            w.set(13, -12, z + 4, b::SOUL_TORCH);
        }
    }
    for _ in 0..40 {
        let x = rng.int(-13, 13);
        let z = rng.int(141, 175);
        if w.is_air(x, -17, z) {
            w.set(x, -17, z, b::BONES);
        }
    }
    for _ in 0..60 {
        let x = rng.int(-13, 13);
        let z = rng.int(141, 175);
        let y = -7 - rng.int(0, 1);
        if w.is_air(x, y, z) {
            w.set(x, y, z, b::WEB);
        }
    }
}

fn lair(w: &mut World, rng: &mut Rng) {
    w.room(-26, 26, -18, -1, 177, 214, b::PURPLE, b::DIRT, b::PURPLE);
    w.fill(-25, 25, -18, -18, 178, 213, b::GRASS);
    // Pumpkin patch (always with gaps between the pumpkins).
    for _ in 0..140 {
        let x = rng.int(-24, 24);
        let z = rng.int(179, 212);
        if x.abs() < 4 && z < 196 {
            continue;
        }
        if pumpkin_spot_free(w, x, -17, z) {
            w.set_meta(x, -17, z, b::PUMPKIN, if z < 196 { 5 } else { rng.int(0, 5) as u8 });
            if rng.chance(0.3) && w.is_air(x + 1, -17, z) && pumpkin_spot_free(w, x + 2, -17, z) {
                w.set(x + 1, -17, z, b::HAY);
            }
        }
    }
    // Amethyst crystal clusters on walls and ceiling.
    for _ in 0..70 {
        let (x, y, z) = match rng.int(0, 3) {
            0 => (-26, rng.int(-16, -3), rng.int(178, 213)),
            1 => (26, rng.int(-16, -3), rng.int(178, 213)),
            2 => (rng.int(-25, 25), -1, rng.int(178, 213)),
            _ => (rng.int(-25, 25), rng.int(-16, -3), 214),
        };
        w.set(x, y, z, b::AMETHYST);
        let (dx, dy, dz) = if x.abs() == 26 {
            (-x.signum(), 0, 0)
        } else if y == -1 {
            (0, -1, 0)
        } else {
            (0, 0, -1)
        };
        if rng.chance(0.6) {
            w.set(x + dx, y + dy, z + dz, b::AMETHYST);
        }
    }
    // Pillars with lanterns.
    for &z in &[184, 197, 210] {
        for &x in &[-18, 18] {
            w.fill(x - 1, x + 1, -17, -2, z - 1, z + 1, b::PURPLE);
            w.set(x - 2, -9, z, b::LANTERN);
            w.set(x + 2, -9, z, b::LANTERN);
        }
    }
}

/// Door opening geometry (shared with the door models in mesh.rs): the doorway is
/// `DOOR_HALF` wide either side of x = 0, straight up to `DOOR_SPRING`, then a round arch.
pub const DOOR_HALF: f32 = 5.0;
pub const DOOR_FLOOR: f32 = -17.0;
pub const DOOR_SPRING: f32 = -9.0;
pub const DOOR_Z: f32 = 214.5;

pub fn in_doorway(x: f32, y: f32, grow: f32) -> bool {
    let r = DOOR_HALF + grow;
    y >= DOOR_FLOOR - grow && x.abs() <= r && (y <= DOOR_SPRING || x * x + (y - DOOR_SPRING).powi(2) <= r * r)
}

/// The grand red-and-gold doors behind the Pumpkin King, and the exit hall beyond.
/// (The door leaves themselves are animated models drawn by the game.)
fn boss_doors(w: &mut World) {
    // Clear the area in front of the doors of any random props.
    for z in 204..=213 {
        for y in -17..=-2 {
            for x in -11..=11 {
                w.set(x, y, z, b::AIR);
            }
        }
    }
    // Exit hall beyond the doors: long enough that its far end is lost in the dark.
    w.room(-7, 7, -18, -5, 214, 299, b::PURPLE, b::STONEBRICK, b::PURPLE);
    w.fill(-26, 26, -18, -1, 214, 214, b::PURPLE);
    for z in (218..296).step_by(5) {
        for x in [-6, 6] {
            w.set(x, -11, z, b::LANTERN);
            w.set(x, -14, z + 2, b::BANNER);
            w.set(x, -13, z + 2, b::BANNER);
        }
    }
    // Royal red carpet from the lair entrance up to the doors and down the hall.
    for z in 178..299 {
        for x in -3..=2 {
            let id = if x == -3 || x == 2 { b::GOLD } else { b::CARPET };
            w.set(x, -18, z, id);
        }
    }
    // Cut the arched doorway and wrap it in a thick gold frame.
    for y in -17..=-1 {
        for x in -8..=7 {
            let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
            if in_doorway(cx, cy, 0.0) {
                w.set(x, y, 214, b::AIR);
            } else if in_doorway(cx, cy, 1.6) {
                w.set(x, y, 214, b::GOLD_GLOW);
                w.set(x, y, 213, b::GOLD_GLOW);
            }
        }
    }
    // Banded pillars either side with braziers in front.
    for px in [-10, 9] {
        for y in -17..=-2 {
            let id = if (y + 17) % 4 == 3 { b::GOLD_GLOW } else { b::PURPLE };
            w.fill(px, px + 1, y, y, 212, 213, id);
        }
        w.set(px, -10, 211, b::LANTERN);
        w.set(px + 1, -10, 211, b::LANTERN);
        let bx = if px < 0 { -13 } else { 12 };
        w.fill(bx, bx, -17, -16, 211, 211, b::GOLD);
        w.set(bx, -15, 211, b::FIRE);
    }
    w.set_meta(-6, -17, 212, b::PUMPKIN, 5);
    w.set_meta(5, -17, 212, b::PUMPKIN, 5);
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

fn after_carve(w: &mut World, track: &Track) {
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
    // Spooky green torches along the drop tunnel.
    let mut side = 1.0;
    for f in track.samples(3.0) {
        if f.pos.z > 127.0 && f.pos.z < 141.0 {
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
                        w.set(x, y, z, b::SOUL_TORCH);
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

fn dead_tree(w: &mut World, rng: &mut Rng, tx: i32, base_y: i32, tz: i32, h: i32) {
    w.fill(tx, tx, base_y, base_y + h - 1, tz, tz, b::LOG);
    let dirs = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1)];
    let n = rng.int(2, 4);
    for k in 0..n {
        let (dx, dz) = dirs[((k + rng.int(0, 5)) % 6) as usize];
        let (mut x, mut z) = (tx, tz);
        let mut y = base_y + h - 1 - rng.int(0, 3);
        for _ in 0..rng.int(2, 4) {
            x += dx;
            z += dz;
            y += rng.int(0, 1);
            w.set(x, y, z, b::LOG);
        }
        if rng.chance(0.3) && w.is_air(x, y - 1, z) {
            w.set(x, y - 1, z, if rng.chance(0.5) { b::WEB } else { b::LANTERN });
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

fn armor(w: &mut World, x: i32, y: i32, z: i32) {
    w.set(x, y, z, b::FENCE);
    w.set(x, y + 1, z, b::IRON);
    w.set(x, y + 2, z, b::SKULLS);
}

fn clock(w: &mut World, x: i32, y: i32, z: i32) {
    w.set(x, y, z, b::DARKPLANK);
    w.set(x, y + 1, z, b::DARKPLANK);
    w.set(x, y + 2, z, b::CLOCK);
    w.set(x, y + 3, z, b::LOG);
}

fn banners(w: &mut World, x: i32, ys: (i32, i32), zr: (i32, i32), step: usize) {
    for z in (zr.0..=zr.1).step_by(step) {
        if w.is_air(x, ys.1, z) && w.is_air(x, ys.0, z) {
            w.set(x, ys.0, z, b::BANNER);
            w.set(x, ys.1, z, b::BANNER);
        }
    }
}

/// Fills every area with props so the ride feels lived-in (well... un-lived-in).
fn decorate(w: &mut World, rng: &mut Rng, path: &[Vec3]) {
    let ground = |r: &mut Rng| -> u8 {
        let k = r.f();
        if k < 0.68 {
            b::TALLGRASS
        } else if k < 0.9 {
            b::DEADBUSH
        } else {
            b::MUSHROOM
        }
    };
    // ---------------- Graveyard: dead forest beyond the fence, ground cover, props.
    for _ in 0..70 {
        let side = if rng.chance(0.5) { -1 } else { 1 };
        let x = side * rng.int(27, 38);
        let z = rng.int(-30, 37);
        if w.is_air(x, 1, z) && w.get(x, 0, z) == b::GRASS {
            let h = rng.int(6, 12);
            dead_tree(w, rng, x, 1, z, h);
        }
    }
    for _ in 0..30 {
        let x = rng.int(-36, 36);
        let z = rng.int(-31, -27);
        if w.is_air(x, 1, z) && x.abs() > 3 {
            let h = rng.int(6, 11);
            dead_tree(w, rng, x, 1, z, h);
        }
    }
    let mut placed = 0;
    for _ in 0..200 {
        if placed >= 10 {
            break;
        }
        let x = rng.int(-21, 21);
        let z = rng.int(-6, 34);
        if !near_track(path, x as f32 + 0.5, z as f32 + 0.5, 5.5) && w.is_air(x, 1, z) && w.is_air(x + 1, 2, z) {
            let h = rng.int(5, 8);
            dead_tree(w, rng, x, 1, z, h);
            placed += 1;
        }
    }
    for _ in 0..1600 {
        let x = rng.int(-39, 39);
        let z = rng.int(-31, 38);
        if w.get(x, 0, z) == b::GRASS && w.is_air(x, 1, z) && !near_track(path, x as f32 + 0.5, z as f32 + 0.5, 1.6) {
            let id = ground(rng);
            w.set(x, 1, z, id);
        }
    }
    // Candles on gravestones.
    for x in -24..24 {
        for z in -10..38 {
            for y in 2..5 {
                if w.get(x, y - 1, z) == b::GRAVE && w.is_air(x, y, z) && rng.chance(0.18) {
                    w.set(x, y, z, b::CANDLE);
                }
            }
        }
    }
    // Scarecrow by the station.
    for &(sx, sz) in &[(9, -14), (-10, 6)] {
        if near_track(path, sx as f32, sz as f32, 3.0) {
            continue;
        }
        w.fill(sx, sx, 1, 2, sz, sz, b::FENCE);
        w.set(sx, 3, sz, b::HAY);
        w.set(sx - 1, 3, sz, b::FENCE);
        w.set(sx + 1, 3, sz, b::FENCE);
        w.set_meta(sx, 4, sz, b::PUMPKIN, 1);
    }
    // Pumpkin patch.
    for _ in 0..40 {
        let x = rng.int(10, 21);
        let z = rng.int(-8, 4);
        if !near_track(path, x as f32 + 0.5, z as f32 + 0.5, 3.2) && pumpkin_spot_free(w, x, 1, z) {
            if rng.chance(0.75) {
                w.set_meta(x, 1, z, b::PUMPKIN, rng.int(0, 5) as u8);
            } else {
                w.set(x, 1, z, b::HAY);
            }
        }
    }
    // Witch's cauldron by the mausoleum.
    w.set(-13, 1, 29, b::CAULDRON);
    w.set(-12, 1, 31, b::CAULDRON);
    w.set(-11, 1, 29, b::POT);

    // ---------------- Foyer
    for &z in &[44, 51, 61] {
        armor(w, -15, 1, z);
    }
    clock(w, -15, 1, 68);
    clock(w, 15, 1, 42);
    banners(w, -15, (12, 13), (44, 68), 6);
    banners(w, 15, (13, 14), (43, 67), 6);
    for &z in &[48, 54, 65] {
        if w.is_air(-14, 1, z) {
            w.set(-14, 1, z, b::PLANK);
            w.set(-14, 2, z, b::CANDLE);
        }
    }
    float_candles(w, rng, 40, (-14, 14), (8, 16), (42, 68));
    scatter(w, rng, 10, (-14, 14), 10, (61, 68), &[b::DARKPLANK], &|_| b::POT);

    // ---------------- Library
    float_candles(w, rng, 45, (-13, 13), (6, 16), (73, 98));
    for &z in &[75, 82, 90, 96] {
        for &x in &[-14, 14] {
            if w.is_air(x, 1, z) {
                w.fill(x, x, 1, 10, z, z, b::FENCE);
            }
        }
    }
    banners(w, -14, (13, 14), (74, 98), 7);
    banners(w, 14, (13, 14), (77, 97), 7);
    clock(w, 13, 1, 97);
    w.set(-1, 1, 81, b::CAULDRON);
    for (dx, dz) in [(1, 0), (0, 1), (-1, 1)] {
        if w.is_air(-1 + dx, 1, 81 + dz) {
            w.set(-1 + dx, 1, 81 + dz, b::POT);
        }
    }
    scatter(w, rng, 20, (-13, 13), 5, (73, 98), &[b::BOOKSHELF], &|_| b::POT);
    scatter(w, rng, 20, (-13, 13), 5, (73, 98), &[b::BOOKSHELF], &|_| b::CANDLE);

    // ---------------- Dining hall
    for z in (106..126).step_by(4) {
        if w.is_air(-9, 2, z) {
            w.set(-9, 2, z, b::CAKE);
        }
        if w.is_air(-11, 2, z + 1) {
            w.set(-11, 2, z + 1, b::POT);
        }
        if w.is_air(-8, 2, z + 2) {
            w.set(-8, 2, z + 2, b::POT);
        }
    }
    for &z in &[104, 112, 120, 128] {
        armor(w, -15, 1, z);
    }
    banners(w, -15, (11, 12), (103, 129), 5);
    banners(w, 15, (12, 13), (103, 129), 5);
    clock(w, 15, 1, 104);
    w.fill(14, 14, 1, 1, 121, 127, b::PLANK);
    for z in (121..=127).step_by(2) {
        w.set(14, 2, z, if z % 4 == 1 { b::CAKE } else { b::CANDLE });
    }
    float_candles(w, rng, 30, (-14, 14), (8, 16), (102, 129));

    // ---------------- Crypt
    for &z in &[147, 159, 171] {
        for &x in &[-13, 13] {
            w.fill(x, x, -17, -15, z, z, b::DARKPLANK);
        }
    }
    scatter(w, rng, 24, (-13, 13), -17, (141, 175), &[b::MOSSY], &|r| if r.chance(0.5) { b::POT } else { b::SKULLS });
    scatter(w, rng, 40, (-13, 13), -17, (141, 175), &[b::MOSSY], &|_| b::MUSHROOM);
    for _ in 0..30 {
        let x = rng.int(-13, 13);
        let z = rng.int(141, 175);
        let len = rng.int(1, 4);
        for k in 0..len {
            if w.is_air(x, -7 - k, z) {
                w.set(x, -7 - k, z, b::CHAIN);
            }
        }
    }
    w.set(-11, -17, 152, b::CAULDRON);
    w.set(11, -17, 166, b::CAULDRON);
    banners(w, -13, (-10, -9), (143, 175), 6);
    banners(w, 13, (-10, -9), (146, 175), 6);

    // ---------------- Pumpkin King's lair
    scatter(w, rng, 160, (-25, 25), -17, (178, 213), &[b::GRASS], &ground);
    banners(w, -25, (-6, -5), (180, 212), 6);
    banners(w, 25, (-6, -5), (180, 212), 6);
}

/// True if a wall spot is close to the ride (the tunnel carve would cut a decoration in half).
fn near_ride(path: &[Vec3], x: i32, y: i32, z: i32) -> bool {
    let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
    path.iter().any(|q| (q.x - p.x).powi(2) + (q.z - p.z).powi(2) < 4.0f32.powi(2) && (p.y - q.y) > -3.0 && (p.y - q.y) < 7.0)
}

/// Hangs decorations on a room's bare walls: gold-framed portraits and paintings,
/// crest shields between banners, mirrors, shelves with pots and candles, torch
/// sconces and clocks. `x0..x1`/`z0..z1` are the wall coordinates, `y0` the floor.
#[allow(clippy::too_many_arguments)]
fn decorate_walls(w: &mut World, rng: &mut Rng, path: &[Vec3], x0: i32, x1: i32, y0: i32, y1: i32, z0: i32, z1: i32, wall: u8, bands: &[i32]) {
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
                            let art = [b::PORTRAIT, b::PORTRAIT2, b::PAINTING3][rng.int(0, 2) as usize];
                            w.set(x, y, z, art);
                            true
                        } else {
                            false
                        }
                    }
                    // Crest shield between two hanging banners.
                    2 => {
                        if (-1..=1).all(|du| free(w, du, 0) && free(w, du, -1)) {
                            let (x, z) = at(0);
                            w.set(x, y, z, b::SHIELD);
                            for du in [-1, 1] {
                                let (x, z) = at(du);
                                w.set(x + nx, y, z + nz, b::BANNER);
                                w.set(x + nx, y - 1, z + nz, b::BANNER);
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
                                w.set(x + nx, y, z + nz, b::TORCH);
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
                                let item = [b::POT, b::CANDLE, b::POT][(du + 1) as usize];
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
                                w.set(x + nx, y, z + nz, b::TORCH);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porch_pillars_and_lanterns_survive_the_ride_tunnel() {
        let track = Track::new();
        let w = generate(&track);
        for x in [-8, 8] {
            for y in 1..=7 {
                assert_eq!(w.get(x, y, 34), b::LOG, "porch pillar at x {x} is missing its block at y {y}");
            }
            assert_eq!(w.get(x - x.signum(), 5, 36), b::LANTERN, "porch lantern by x {x} was carved away");
        }
    }
}
