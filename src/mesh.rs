//! Geometry helpers: cube faces, oriented boxes and small voxel models
//! (ghosts, bats, the Pumpkin King...) built from code.

use crate::atlas::{tile_uv, T_WHITE};
use crate::level::Level;
use crate::vk::Vertex;
use glam::{Mat4, Vec3};
use std::collections::HashMap;

/// Cube face definitions: (normal, origin corner, right, up) on the unit cube [0,1]^3.
/// Vertices are o, o+r, o+r+u, o+u (counter-clockwise seen from outside).
pub const FACES: [([i32; 3], [f32; 3], [f32; 3], [f32; 3]); 6] = [
    ([1, 0, 0], [1.0, 0.0, 1.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
    ([-1, 0, 0], [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
    ([0, 1, 0], [0.0, 1.0, 1.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
    ([0, -1, 0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
    ([0, 0, 1], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ([0, 0, -1], [1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
];

/// Minecraft-like directional shading for a face normal.
pub fn shade(n: Vec3) -> f32 {
    (0.8 + 0.2 * n.y - 0.14 * n.x.abs()).clamp(0.45, 1.0)
}

pub fn push_quad(out: &mut Vec<Vertex>, p: [Vec3; 4], uv: [f32; 4], color: [[f32; 4]; 4]) {
    let uvs = [[uv[0], uv[3]], [uv[2], uv[3]], [uv[2], uv[1]], [uv[0], uv[1]]];
    for i in 0..4 {
        out.push(Vertex { pos: p[i].to_array(), uv: uvs[i], color: color[i] });
    }
}

/// An oriented box (unit cube centred at origin transformed by `m`), uniformly lit.
/// `light` is the incoming light colour; `glow` is a minimum brightness (1 = emissive).
pub fn push_box(out: &mut Vec<Vertex>, m: &Mat4, tile: u8, tint: [f32; 3], light: Vec3, glow: f32) {
    push_box_tiles(out, m, [tile; 6], tint, light, glow);
}

pub fn push_box_tiles(out: &mut Vec<Vertex>, m: &Mat4, tiles: [u8; 6], tint: [f32; 3], light: Vec3, glow: f32) {
    for (fi, (n, o, r, u)) in FACES.iter().enumerate() {
        let o = Vec3::from(*o) - Vec3::splat(0.5);
        let r = Vec3::from(*r);
        let u = Vec3::from(*u);
        let p = [
            m.transform_point3(o),
            m.transform_point3(o + r),
            m.transform_point3(o + r + u),
            m.transform_point3(o + u),
        ];
        let wn = m.transform_vector3(Vec3::new(n[0] as f32, n[1] as f32, n[2] as f32)).normalize_or_zero();
        let s = shade(wn);
        let c = face_color(tint, light, s, glow);
        push_quad(out, p, tile_uv(tiles[fi]), [c; 4]);
    }
}

pub fn face_color(tint: [f32; 3], light: Vec3, shade: f32, glow: f32) -> [f32; 4] {
    let lit = (light * shade).max(Vec3::splat(glow * (0.75 + 0.25 * shade)));
    [tint[0] * lit.x, tint[1] * lit.y, tint[2] * lit.z, (1.0 - glow).clamp(0.0, 1.0) * shade]
}

/// One voxel of a model.
#[derive(Clone, Copy)]
pub struct Vox {
    pub color: [f32; 3],
    pub glow: f32,
}

pub fn vox(hexc: u32, glow: f32) -> Option<Vox> {
    Some(Vox { color: crate::atlas::hex(hexc), glow })
}

#[derive(Clone, Copy)]
struct MQuad {
    p: [Vec3; 4],
    n: Vec3,
    color: [f32; 3],
    glow: f32,
}

/// A small voxel model, pre-culled, in model space.
pub struct Model {
    quads: Vec<MQuad>,
    pub size: Vec3,
    /// The voxels it was built from, (w, h, d) and the voxel size (for exporting).
    grid: Vec<Option<Vox>>,
    dims: (i32, i32, i32),
    scale: f32,
}

impl Model {
    /// Builds a model from a voxel function over a `w x h x d` grid. The model is
    /// centred on x/z and has its bottom at y = 0. One voxel = `scale` world units.
    pub fn build(w: i32, h: i32, d: i32, scale: f32, f: impl Fn(i32, i32, i32) -> Option<Vox>) -> Model {
        let mut grid = vec![None; (w * h * d) as usize];
        for z in 0..d {
            for y in 0..h {
                for x in 0..w {
                    grid[((z * h + y) * w + x) as usize] = f(x, y, z);
                }
            }
        }
        let get = |x: i32, y: i32, z: i32| -> Option<Vox> {
            if x < 0 || y < 0 || z < 0 || x >= w || y >= h || z >= d {
                None
            } else {
                grid[((z * h + y) * w + x) as usize]
            }
        };
        let origin = Vec3::new(w as f32 * 0.5, 0.0, d as f32 * 0.5);
        let mut quads = vec![];
        for z in 0..d {
            for y in 0..h {
                for x in 0..w {
                    let Some(v) = get(x, y, z) else { continue };
                    // Small per-voxel colour variation gives a hand-painted pixel look.
                    let j = 0.93 + hash3(x, y, z) * 0.12;
                    let color = [v.color[0] * j, v.color[1] * j, v.color[2] * j];
                    for (n, o, r, u) in FACES.iter() {
                        if get(x + n[0], y + n[1], z + n[2]).is_some() {
                            continue;
                        }
                        let base = Vec3::new(x as f32, y as f32, z as f32) + Vec3::from(*o);
                        let r = Vec3::from(*r);
                        let u = Vec3::from(*u);
                        let p = [base, base + r, base + r + u, base + u].map(|p| (p - origin) * scale);
                        quads.push(MQuad {
                            p,
                            n: Vec3::new(n[0] as f32, n[1] as f32, n[2] as f32),
                            color,
                            glow: v.glow,
                        });
                    }
                }
            }
        }
        Model { quads, size: Vec3::new(w as f32, h as f32, d as f32) * scale, grid, dims: (w, h, d), scale }
    }

    /// The model's faces with neighbouring faces of the same colour merged into
    /// rectangles (fewer polygons, for the PS1). Each rectangle gets the per-voxel
    /// colour variation of its first voxel. Returns (corners, normal, colour, glow).
    pub fn merged_faces(&self) -> Vec<([Vec3; 4], Vec3, [f32; 3], f32)> {
        let (w, h, d) = self.dims;
        let get = |p: glam::IVec3| -> Option<Vox> {
            if p.x < 0 || p.y < 0 || p.z < 0 || p.x >= w || p.y >= h || p.z >= d {
                None
            } else {
                self.grid[((p.z * h + p.y) * w + p.x) as usize]
            }
        };
        let key = |v: Vox| ([0, 1, 2].map(|i| (v.color[i] * 255.0).round() as i32), (v.glow * 255.0).round() as i32);
        let origin = Vec3::new(w as f32 * 0.5, 0.0, d as f32 * 0.5);
        let mut out = vec![];
        for (n, o, r, u) in FACES.iter() {
            let (n, r, u) = (glam::IVec3::from(*n), Vec3::from(*r).as_ivec3(), Vec3::from(*u).as_ivec3());
            // Exposed faces of this direction, keyed by (slice, i along r, j along u).
            let mut faces: HashMap<(i32, i32, i32), (glam::IVec3, Vox)> = HashMap::new();
            for z in 0..d {
                for y in 0..h {
                    for x in 0..w {
                        let p = glam::IVec3::new(x, y, z);
                        let Some(v) = get(p) else { continue };
                        if get(p + n).is_some() {
                            continue;
                        }
                        faces.insert((p.dot(n), p.dot(r), p.dot(u)), (p, v));
                    }
                }
            }
            let mut keys: Vec<_> = faces.keys().copied().collect();
            keys.sort_by_key(|k| (k.0, k.2, k.1));
            let mut used = std::collections::HashSet::new();
            for k0 in keys {
                if used.contains(&k0) {
                    continue;
                }
                let (p0, v0) = faces[&k0];
                let same = |k: (i32, i32, i32)| !used.contains(&k) && faces.get(&k).is_some_and(|f| key(f.1) == key(v0));
                let mut a = 1;
                while same((k0.0, k0.1 + a, k0.2)) {
                    a += 1;
                }
                let mut b = 1;
                while (0..a).all(|i| same((k0.0, k0.1 + i, k0.2 + b))) {
                    b += 1;
                }
                for j in 0..b {
                    for i in 0..a {
                        used.insert((k0.0, k0.1 + i, k0.2 + j));
                    }
                }
                let base = p0.as_vec3() + Vec3::from(*o);
                let (ra, ub) = ((r * a).as_vec3(), (u * b).as_vec3());
                let p = [base, base + ra, base + ra + ub, base + ub].map(|p| (p - origin) * self.scale);
                let jit = 0.93 + hash3(p0.x, p0.y, p0.z) * 0.12;
                out.push((p, n.as_vec3(), v0.color.map(|c| c * jit), v0.glow));
            }
        }
        out
    }

    /// Emits the model transformed by `m`. `flash` (0..1) tints toward white (hit feedback).
    pub fn draw(&self, out: &mut Vec<Vertex>, m: &Mat4, light: Vec3, flash: f32, tint: Option<[f32; 3]>) {
        let uv = tile_uv(T_WHITE);
        for q in &self.quads {
            let p = q.p.map(|p| m.transform_point3(p));
            let wn = m.transform_vector3(q.n).normalize_or_zero();
            let s = shade(wn);
            let mut col = q.color;
            if let Some(t) = tint {
                col = [col[0] * t[0], col[1] * t[1], col[2] * t[2]];
            }
            if flash > 0.0 {
                col = [
                    col[0] + (1.0 - col[0]) * flash,
                    col[1] + (1.0 - col[1]) * flash,
                    col[2] + (1.0 - col[2]) * flash,
                ];
            }
            let glow = if flash > 0.0 { q.glow.max(flash) } else { q.glow };
            let c = face_color(col, light, s, glow);
            push_quad(out, p, uv, [c; 4]);
        }
    }
}

pub fn hash3(x: i32, y: i32, z: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(73856093) ^ (y as u32).wrapping_mul(19349663) ^ (z as u32).wrapping_mul(83492791);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h ^= h >> 16;
    (h & 0xffff) as f32 / 65535.0
}

/// All the voxel models used by targets and the boss, for one level. Models face +Z.
/// The Christmas level fills the shared slots with its own models: `pumpkin` is a
/// snowman, `ghost` a gingerbread man, `skull`/`torso`/`arm` a nutcracker, `candy` a
/// golden star, the boss the Snow King, and the doors are wrapped like a present.
pub struct Models {
    pub ghost: Model,
    pub bat_body: Model,
    pub bat_wing: Model,
    pub pumpkin: Model,
    pub skull: Model,
    pub torso: Model,
    pub arm: Model,
    pub spider: Model,
    pub book_half: [Model; 3],
    pub candy: Model,
    pub boss_head: Model,
    pub boss_head_angry: Model,
    pub boss_cape: Model,
    pub boss_hand: Model,
    /// The two leaves of the boss doors: [left, right].
    pub door: [Model; 2],
    // Christmas only.
    pub plane: Model,
    pub present_box: [Model; 3],
    pub present_lid: [Model; 3],
    pub bauble: [Model; 3],
    pub wreath: Model,
}

impl Models {
    pub fn new(level: Level) -> Models {
        let mut m = Models::halloween();
        if level == Level::Christmas {
            m.pumpkin = snowman();
            m.ghost = gingerbread();
            m.skull = nutcracker_head();
            m.torso = nutcracker_body();
            m.arm = nutcracker_arm();
            m.candy = gold_star();
            m.boss_head = snow_king_head(0x202028, 0.0);
            m.boss_head_angry = snow_king_head(0xff3020, 1.0);
            m.boss_cape = snow_king_body();
            m.boss_hand = mitten();
            m.door = [present_door(false), present_door(true)];
        }
        m
    }

    fn halloween() -> Models {
        Models {
            plane: plane(),
            present_box: [present_box(0xc8202c, 0xf0c840), present_box(0x1f7a3a, 0xe83040), present_box(0x2850c8, 0xe8e8f0)],
            present_lid: [present_lid(0xc8202c, 0xf0c840), present_lid(0x1f7a3a, 0xe83040), present_lid(0x2850c8, 0xe8e8f0)],
            bauble: [bauble(0xd02030, 0xf0d060), bauble(0xe8b830, 0xffffff), bauble(0x3060e0, 0xe8e8f0)],
            wreath: wreath(),
            ghost: ghost(),
            bat_body: bat_body(),
            bat_wing: Model::build(6, 1, 4, 0.12, |x, _, z| {
                if z <= 3 - x / 2 || (x == 5 && z == 0) {
                    vox(0x4a3358, 0.15)
                } else {
                    None
                }
            }),
            pumpkin: pumpkin(),
            skull: skull(),
            torso: Model::build(6, 9, 3, 0.09, |x, y, z| {
                let spine = x == 2 || x == 3;
                let rib = y % 2 == 1 && y > 2;
                if spine || rib || y == 0 || y >= 8 {
                    if z == 1 || spine {
                        vox(0xe8e2cc, 0.35)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }),
            arm: Model::build(2, 9, 2, 0.09, |_, _, _| vox(0xe0dac4, 0.35)),
            spider: spider(),
            book_half: [book_half(0x8a1e22), book_half(0x1e3a8a), book_half(0x2a6a2a)],
            candy: candy(),
            boss_head: boss_head(0xfff6a8),
            boss_head_angry: boss_head(0xff4a2a),
            boss_cape: boss_cape(),
            door: [door_leaf(false), door_leaf(true)],
            boss_hand: Model::build(5, 5, 4, 0.42, |x, y, z| {
                if y == 4 && (x == 0 || x == 4) {
                    return None;
                }
                if x == 4 && y < 2 {
                    return None;
                }
                if y == 0 && z > 0 && z < 3 && x < 4 {
                    return vox(0xff9a1e, 0.6); // orange cuff
                }
                vox(0xf4f4f8, 0.55)
            }),
        }
    }
}

fn ghost() -> Model {
    Model::build(9, 8, 5, 0.19, |x, y, z| {
        let arm = (x == 0 || x == 8) && (y == 3 || y == 4) && z == 2;
        if arm {
            return vox(0xf0f0ff, 0.8);
        }
        if x < 1 || x > 7 {
            return None;
        }
        let bx = x - 1; // 0..6
        if y == 7 && (bx == 0 || bx == 6 || z == 0 || z == 4) {
            return None;
        }
        if y == 6 && (bx == 0 || bx == 6) && (z == 0 || z == 4) {
            return None;
        }
        if y == 0 && bx % 2 == 1 {
            return None;
        }
        if z == 4 {
            if (bx == 2 || bx == 4) && (y == 4 || y == 5) {
                return vox(0x14141e, 0.0);
            }
            if bx == 3 && y == 2 {
                return vox(0x3a1430, 0.0);
            }
            if (bx == 1 || bx == 5) && y == 3 {
                return vox(0xffa0c0, 0.8);
            }
        }
        vox(0xf2f2ff, 0.8)
    })
}

fn bat_body() -> Model {
    Model::build(5, 5, 4, 0.12, |x, y, z| {
        if y == 4 {
            return if (x == 0 || x == 4) && (z == 1 || z == 2) { vox(0x2e2238, 0.15) } else { None };
        }
        if z == 3 && y == 2 && (x == 1 || x == 3) {
            return vox(0xffe040, 1.0);
        }
        if z == 3 && y == 0 && (x == 1 || x == 3) {
            return vox(0xffffff, 0.6);
        }
        if y == 0 && (x == 0 || x == 4) {
            return None;
        }
        vox(0x3a2c48, 0.15)
    })
}

fn pumpkin() -> Model {
    // Jack-o'-lantern with a glowing carved face.
    // Face rows from the top of the pumpkin down: eyes, nose, then a big grin.
    const FACE: [&str; 7] = ["........", ".##..##.", ".##..##.", "...##...", ".#....#.", "..####..", "........"];
    Model::build(8, 9, 8, 0.16, |x, y, z| {
        if y >= 7 {
            return if (x == 3 || x == 4) && (z == 3 || z == 4) && (y == 7 || x == 4) {
                vox(0x4a6a1e, 0.1)
            } else {
                None
            };
        }
        let cx = x == 0 || x == 7;
        let cz = z == 0 || z == 7;
        if cx && cz {
            return None;
        }
        if (y == 0 || y == 6) && (cx || cz) {
            return None;
        }
        if z == 7 && FACE[6 - y as usize].as_bytes()[x as usize] == b'#' {
            return vox(0xffd84a, 1.0);
        }
        if x % 2 == 0 {
            vox(0xd06810, 0.12)
        } else {
            vox(0xf08a18, 0.12)
        }
    })
}

fn skull() -> Model {
    Model::build(8, 8, 8, 0.1, |x, y, z| {
        if z == 7 {
            if (y == 4 || y == 5) && (x == 1 || x == 2 || x == 5 || x == 6) {
                return vox(0x1a1a20, 0.0);
            }
            if y == 3 && (x == 3 || x == 4) {
                return vox(0x2a2a30, 0.0);
            }
            if y == 1 && x % 2 == 0 && x > 0 && x < 7 {
                return vox(0x3a3a3a, 0.0);
            }
        }
        if y == 0 && (z < 2) {
            return None;
        }
        vox(0xece6d2, 0.35)
    })
}

fn spider() -> Model {
    Model::build(7, 4, 9, 0.11, |x, y, z| {
        let head = z >= 6;
        if head && (x == 0 || x == 6) {
            return None;
        }
        if head && y == 3 {
            return None;
        }
        if z == 8 && (y == 1 || y == 2) && (x == 1 || x == 2 || x == 4 || x == 5) {
            return vox(0xff3030, 1.0);
        }
        if !head && y == 3 && (x == 0 || x == 6 || z == 0 || z == 5) {
            return None;
        }
        if !head && y == 3 && x == 3 && z % 2 == 0 {
            return vox(0x8a2a8a, 0.3); // purple back spots
        }
        vox(0x2a2030, 0.2)
    })
}

fn book_half(cover: u32) -> Model {
    // Spine along local z axis at x = 0, page half extends to +x.
    Model::build(5, 2, 7, 0.1, move |x, y, z| {
        if y == 0 {
            vox(cover, 0.25)
        } else if x < 4 && z > 0 && z < 6 {
            vox(0xf0ead6, 0.3)
        } else {
            None
        }
    })
}

fn candy() -> Model {
    Model::build(9, 4, 4, 0.13, |x, y, z| {
        let center = (3..=5).contains(&x);
        let neck = x == 2 || x == 6;
        let inner = (1..=2).contains(&y) && (1..=2).contains(&z);
        if center {
            return if (x + y + z) % 2 == 0 { vox(0xffd030, 0.9) } else { vox(0xff7a1e, 0.9) };
        }
        if neck {
            return if inner { vox(0xffe680, 0.8) } else { None };
        }
        let corner = (y == 0 || y == 3) && (z == 0 || z == 3);
        if corner && (x == 1 || x == 7) {
            return None;
        }
        vox(0xfff0a0, 0.85)
    })
}

fn boss_head(glow: u32) -> Model {
    // Classic jack-o'-lantern face (top row first): pointed eyes, a nose and a big
    // grin with two teeth, on plenty of orange so it reads clearly.
    const FACE: [&str; 13] = [
        "...............",
        "...............",
        "...#.......#...",
        "..###.....###..",
        ".#####...#####.",
        "...............",
        "......###......",
        "...............",
        ".#...........#.",
        ".###.#####.###.",
        "..###########..",
        "...............",
        "...............",
    ];
    // A rounded cube (like a Minecraft pumpkin) so the carved face sits on a flat
    // front panel and stays crisp, even seen from below.
    let inside = |x: i32, y: i32, z: i32| -> bool {
        let dx = ((x as f32 - 7.0) / 7.5).abs();
        let dy = ((y as f32 - 6.0) / 6.5).abs();
        let dz = ((z as f32 - 7.0) / 7.5).abs();
        dx.powi(4) + dy.powi(4) + dz.powi(4) <= 1.0
    };
    let front_z = move |x: i32, y: i32| -> i32 { (0..15).rev().find(|&z| inside(x, y, z)).unwrap_or(-1) };
    Model::build(15, 17, 15, 0.5, move |x, y, z| {
        // Stem, leaf and crown above the head.
        if y >= 12 {
            let stem = (6..=8).contains(&x) && (6..=8).contains(&z) && y <= 15;
            if stem {
                return vox(0x4a6a1e, 0.2);
            }
            if y == 14 && (9..=11).contains(&x) && z == 7 {
                return vox(0x3a8a2a, 0.2);
            }
            let r = (((x as f32 - 7.0).powi(2) + (z as f32 - 7.0).powi(2)) as f32).sqrt();
            let ang = (z as f32 - 7.0).atan2(x as f32 - 7.0);
            let spike = ((ang / std::f32::consts::TAU * 8.0).rem_euclid(1.0)) < 0.4;
            if (3.2..4.6).contains(&r) && (y == 12 || y == 13 || (y == 14 && spike)) {
                if y == 13 && spike && !inside(x, y, z) {
                    return vox(0xc040ff, 1.0);
                }
                return vox(0xffc828, 0.65);
            }
        }
        if !inside(x, y, z) {
            return None;
        }
        let row = 12 - y;
        let carved = row >= 0 && row < 13 && FACE[row as usize].as_bytes()[x as usize] == b'#';
        let fz = front_z(x, y);
        // The face glows flush on the flat front (like a Minecraft carved pumpkin),
        // so its shapes stay crisp from any angle, even looking up at him.
        if carved && (z == fz || z == fz - 1) {
            return vox(glow, 1.0);
        }
        let ang = (z as f32 - 7.0).atan2(x as f32 - 7.0);
        let ridge = ((ang / std::f32::consts::TAU * 10.0).rem_euclid(1.0)) < 0.22;
        if ridge {
            vox(0x9a420a, 0.15)
        } else {
            vox(0xd0661a, 0.15)
        }
    })
}

fn boss_cape() -> Model {
    Model::build(13, 9, 13, 0.5, |x, y, z| {
        let r = (((x as f32 - 6.0).powi(2) + (z as f32 - 6.0).powi(2)) as f32).sqrt();
        let max_r = 6.2 - y as f32 * 0.45;
        if r > max_r {
            return None;
        }
        if y == 0 && (x + z) % 2 == 0 {
            return None;
        }
        if y == 8 {
            return vox(0xffc828, 0.6);
        }
        if r < max_r - 1.2 && y < 7 {
            return None; // hollow inside
        }
        if (x + y) % 5 == 0 {
            vox(0x3a1050, 0.3)
        } else {
            vox(0x5a1a7a, 0.3)
        }
    })
}

/// One leaf of the Pumpkin King's grand arched doors: red planks, gold trim and
/// straps with studs, a ring handle, and half of a gold jack-o'-lantern crest so the
/// two leaves form a whole pumpkin when closed. Built in 0.5 m voxels; voxel x 0 is
/// the hinge side of the left leaf and the centre side of the right leaf.
fn door_leaf(right: bool) -> Model {
    use crate::world::{in_doorway, DOOR_HALF};
    let vw = (DOOR_HALF * 2.0) as i32; // voxels across one leaf
    let vh = 26;
    Model::build(vw, vh, 1, 0.5, move |ix, iy, _| {
        // Distance of this voxel's centre from the middle of the doorway.
        let k = if right { ix } else { vw - 1 - ix };
        let cx = k as f32 * 0.5 + 0.25;
        let cy = iy as f32 * 0.5 + 0.25;
        if !in_doorway(cx, cy, 0.0) {
            return None;
        }
        let edge = !in_doorway(cx, cy + 0.5, 0.0) || !in_doorway(cx + 0.5, cy, 0.0) || k == 0 || iy == 0;
        let strap = iy == 4 || iy == 12;
        let crest = cx * cx + (cy - 9.0).powi(2) <= 2.6f32.powi(2);
        let crest_face = crest && ((iy == 18 || iy == 19) && (k == 2 || k == 3) || iy == 15 && k < 4 || iy == 16 && k == 3);
        if edge {
            return vox(0xf0c040, 0.45);
        }
        if crest_face {
            return vox(0xffe680, 0.9);
        }
        if crest {
            return vox(0xe0a020, 0.5);
        }
        if strap {
            return if k % 3 == 1 { vox(0xfff0a0, 0.6) } else { vox(0xc89020, 0.4) };
        }
        if k == 1 && iy == 9 {
            return vox(0xf0c040, 0.6); // ring handle
        }
        // Red planks with darker grooves.
        if k % 2 == 0 { vox(0x9a1424, 0.2) } else { vox(0x7c0e1c, 0.2) }
    })
}

// ============================================================== Christmas

/// Draws a model from a picture: `rows` top row first, `#` = `body`, other characters
/// looked up in `colors`; every voxel `depth` deep (the details only on the front).
fn picture(rows: &'static [&'static str], depth: i32, scale: f32, body: u32, colors: &'static [(u8, u32, f32)], glow: f32) -> Model {
    let (w, h) = (rows[0].len() as i32, rows.len() as i32);
    Model::build(w, h, depth, scale, move |x, y, z| {
        let c = rows[(h - 1 - y) as usize].as_bytes()[x as usize];
        if c == b'.' {
            return None;
        }
        if c != b'#' && z == depth - 1 {
            if let Some(&(_, col, g)) = colors.iter().find(|k| k.0 == c) {
                return vox(col, g);
            }
        }
        vox(body, glow)
    })
}

fn snowman() -> Model {
    // Three balls of snow, coal eyes and buttons, a carrot nose, a red scarf and twig arms.
    let balls = [(3.0, 3.2), (7.0, 2.4), (10.4, 1.9)];
    let inside = move |x: i32, y: i32, z: i32| {
        let (px, py, pz) = (x as f32 + 0.5 - 4.5, y as f32 + 0.5, z as f32 + 0.5 - 4.5);
        balls.iter().any(|&(cy, r)| px * px + (py - cy).powi(2) + pz * pz <= r * r)
    };
    let front = move |x: i32, y: i32| (0..9).rev().find(|&z| inside(x, y, z));
    Model::build(9, 13, 9, 0.11, move |x, y, z| {
        if !inside(x, y, z) {
            // Carrot nose, sticking out of the face.
            if x == 4 && y == 10 && front(x, y).is_some_and(|f| z > f && z <= f + 2) {
                return vox(0xf07818, 0.5);
            }
            // Twig arms.
            if (y == 7 || (y == 8 && (x == 0 || x == 8))) && z == 4 && (x <= 1 || x >= 7) {
                return vox(0x5a3a1e, 0.2);
            }
            return None;
        }
        let is_front = front(x, y) == Some(z);
        if is_front && y == 11 && (x == 3 || x == 5) {
            return vox(0x18181e, 0.0);
        }
        if is_front && y == 9 && (x == 3 || x == 5) {
            return vox(0x18181e, 0.0);
        }
        if is_front && x == 4 && (y == 4 || y == 6) {
            return vox(0x18181e, 0.0);
        }
        if y == 8 {
            return if (x + z) % 3 == 0 { vox(0x2a8a40, 0.35) } else { vox(0xd02028, 0.35) };
        }
        vox(0xf2f6ff, 0.4)
    })
}

fn gingerbread() -> Model {
    const ROWS: [&str; 13] = [
        "....###....",
        "...#####...",
        "..##e#e##..",
        "..#######..",
        "..##sss##..",
        "...#####...",
        "ww#######ww",
        "###########",
        "...##r##...",
        "...#####...",
        "...##g##...",
        "...##.##...",
        "..ww#.#ww..",
    ];
    picture(&ROWS, 2, 0.13, 0xb06a30, &[(b'e', 0xffffff, 0.8), (b's', 0xffffff, 0.8), (b'w', 0xffffff, 0.8), (b'r', 0xe02030, 0.8), (b'g', 0x30c050, 0.8)], 0.4)
}

fn plane() -> Model {
    // A toy biplane-ish plane: red body, yellow wings, a blue cockpit, facing +Z.
    Model::build(11, 5, 8, 0.11, |x, y, z| {
        let body = (4..=6).contains(&x) && (1..=3).contains(&y) && z <= 7 && !(z == 7 && (x != 5 || y != 2));
        let wing = y == 2 && z >= 3 && z <= 4;
        let tail_fin = x == 5 && (y == 3 || y == 4) && z == 0;
        let tail_plane = y == 2 && z == 0 && (2..=8).contains(&x);
        let cockpit = x == 5 && y == 4 && (z == 4 || z == 5);
        let wheel = y == 0 && (x == 4 || x == 6) && z == 5;
        if cockpit {
            return vox(0x8ad0ff, 0.6);
        }
        if body {
            if z == 7 {
                return vox(0xe0e0e8, 0.5);
            }
            return if z == 5 { vox(0xffffff, 0.4) } else { vox(0xd02828, 0.35) };
        }
        if wing {
            return if x == 0 || x == 10 { vox(0xd02828, 0.35) } else { vox(0xf0d030, 0.4) };
        }
        if tail_fin || tail_plane {
            return vox(0xd02828, 0.35);
        }
        if wheel {
            return vox(0x202024, 0.1);
        }
        None
    })
}

fn present_box(paper: u32, ribbon: u32) -> Model {
    Model::build(6, 5, 6, 0.1, move |x, _, z| if x == 2 || x == 3 || z == 2 || z == 3 { vox(ribbon, 0.5) } else { vox(paper, 0.4) })
}

/// A present's lid (hinged along its back edge, z = 0) with a bow on top.
fn present_lid(paper: u32, ribbon: u32) -> Model {
    Model::build(6, 3, 6, 0.1, move |x, y, z| {
        let band = x == 2 || x == 3 || z == 2 || z == 3;
        match y {
            0 => vox(if band { ribbon } else { paper }, 0.45),
            1 => ((1..=4).contains(&x) && (z == 2 || z == 3) || (x == 2 || x == 3) && (1..=4).contains(&z)).then(|| Vox { color: crate::atlas::hex(ribbon), glow: 0.6 }),
            _ => ((x == 1 || x == 4) && (z == 2 || z == 3)).then(|| Vox { color: crate::atlas::hex(ribbon), glow: 0.6 }),
        }
    })
}

/// A shiny Christmas ornament with a band round its middle and a gold cap.
fn bauble(color: u32, band: u32) -> Model {
    Model::build(7, 8, 7, 0.11, move |x, y, z| {
        if y == 7 {
            return ((2..=4).contains(&x) && (2..=4).contains(&z)).then(|| Vox { color: crate::atlas::hex(0xd8b040), glow: 0.5 });
        }
        let (px, py, pz) = (x as f32 - 3.0, y as f32 - 3.2, z as f32 - 3.0);
        if px * px + py * py + pz * pz > 3.5 * 3.5 {
            return None;
        }
        if y == 3 {
            return vox(band, 0.7);
        }
        // A glint up on its front.
        if x == 2 && y == 5 && z >= 5 {
            return vox(0xffffff, 1.0);
        }
        vox(color, 0.45)
    })
}

fn wreath() -> Model {
    Model::build(13, 13, 2, 0.1, |x, y, z| {
        let (dx, dy) = (x as f32 - 6.0, y as f32 - 6.5);
        let d = (dx * dx + dy * dy).sqrt();
        // A red bow at the bottom.
        let bow = y <= 3 && (x as i32 - 6).abs() <= 3 - (y as i32 - 1).abs().min(1) && z == 1;
        if bow && d >= 3.0 {
            return vox(0xd8202a, 0.5);
        }
        if !(3.2..=6.4).contains(&d) {
            return None;
        }
        if z == 1 && hash3(x, y, 9) > 0.82 {
            return vox(0xff3040, 0.8);
        }
        if (x + y) % 3 == 0 {
            vox(0x1c5a2a, 0.3)
        } else {
            vox(0x2b7a3a, 0.3)
        }
    })
}

fn gold_star() -> Model {
    Model::build(11, 11, 3, 0.11, |x, y, z| {
        let (dx, dy) = (x as f32 - 5.0, y as f32 - 4.8);
        let r = (dx * dx + dy * dy).sqrt();
        let a = dy.atan2(dx) - std::f32::consts::FRAC_PI_2;
        let k = (a / std::f32::consts::TAU * 5.0).rem_euclid(1.0);
        let reach = 2.3 + 3.4 * (1.0 - (k - 0.5).abs() * 2.0).powi(3);
        // Thicker in the middle than at the tips.
        let reach = if z == 1 { reach + 0.4 } else { reach - 0.6 };
        if r > reach {
            return None;
        }
        if r > reach - 0.9 {
            vox(0xffa818, 0.9)
        } else {
            vox(0xffe050, 0.95)
        }
    })
}

fn nutcracker_head() -> Model {
    // Face with a white beard and a tall black hat with a gold band and a red plume.
    Model::build(6, 10, 6, 0.1, |x, y, z| {
        if y >= 5 {
            let hat = (1..=4).contains(&x) && (1..=4).contains(&z);
            if y == 9 {
                return (x == 2 && z == 2 || x == 3 && z == 3 || x == 2 && z == 3 || x == 3 && z == 2).then(|| Vox { color: crate::atlas::hex(0xe02030), glow: 0.5 });
            }
            if !hat {
                return None;
            }
            return if y == 5 { vox(0xe8c040, 0.5) } else { vox(0x1a1a24, 0.2) };
        }
        let front = z == 5;
        if front && y == 3 && (x == 1 || x == 4) {
            return vox(0x101018, 0.0);
        }
        if front && y == 2 && (x == 2 || x == 3) {
            return vox(0xd88870, 0.4);
        }
        if y <= 1 && (z >= 3 || x == 0 || x == 5) {
            return if front && y == 1 && (x == 2 || x == 3) { vox(0x501018, 0.2) } else { vox(0xf4f4f4, 0.5) };
        }
        vox(0xf0c8a0, 0.45)
    })
}

fn nutcracker_body() -> Model {
    // Black boots, white trousers, a black belt with a gold buckle and a red jacket with
    // gold buttons and epaulettes.
    Model::build(6, 9, 4, 0.09, |x, y, z| {
        let legs = (1..=4).contains(&x);
        match y {
            0 | 1 => legs.then(|| Vox { color: crate::atlas::hex(0x14141a), glow: 0.2 }),
            2 | 3 => legs.then(|| Vox { color: crate::atlas::hex(0xf0f0f4), glow: 0.4 }),
            4 => vox(if (x == 2 || x == 3) && z == 3 { 0xe8c040 } else { 0x14141a }, 0.3),
            8 if x == 0 || x == 5 => vox(0xe8c040, 0.5),
            _ if z == 3 && (x == 2 || x == 3) && (y == 5 || y == 7) => vox(0xe8c040, 0.5),
            _ => vox(0xc81e2c, 0.35),
        }
    })
}

fn nutcracker_arm() -> Model {
    Model::build(2, 9, 2, 0.09, |_, y, _| if y <= 1 { vox(0xf4f4f4, 0.45) } else { vox(0xc81e2c, 0.35) })
}

fn snow_king_head(eyes: u32, eye_glow: f32) -> Model {
    // A huge snowman's head: coal eyes and smile, a carrot nose and an icy crown.
    const FACE: [&str; 13] = [
        "...............",
        "...............",
        "..###.....###..",
        "..###.....###..",
        "..###.....###..",
        "...............",
        "...............",
        "...............",
        ".#...........#.",
        "..#.........#..",
        "...##.#.#.##...",
        "...............",
        "...............",
    ];
    let inside = |x: i32, y: i32, z: i32| -> bool {
        let dx = ((x as f32 - 7.0) / 7.5).abs();
        let dy = ((y as f32 - 6.0) / 6.5).abs();
        let dz = ((z as f32 - 9.0) / 7.5).abs();
        dx.powf(2.6) + dy.powf(2.6) + dz.powf(2.6) <= 1.0
    };
    let front_z = move |x: i32, y: i32| -> i32 { (0..19).rev().find(|&z| inside(x, y, z)).unwrap_or(-1) };
    Model::build(15, 17, 19, 0.5, move |x, y, z| {
        if y >= 12 {
            let r = (((x as f32 - 7.0).powi(2) + (z as f32 - 9.0).powi(2)) as f32).sqrt();
            let ang = (z as f32 - 9.0).atan2(x as f32 - 7.0);
            let spike = ((ang / std::f32::consts::TAU * 8.0).rem_euclid(1.0)) < 0.4;
            if (3.2..4.6).contains(&r) && (y == 12 || y == 13 || (y == 14 && spike) || (y == 15 && spike && r < 4.0)) {
                if y == 13 && spike && !inside(x, y, z) {
                    return vox(0xff4090, 1.0);
                }
                return vox(0xa8e0ff, 0.7);
            }
        }
        let fz = front_z(x, y);
        if !inside(x, y, z) {
            // The carrot nose.
            let nose = (x == 7 && y == 6 && z > fz && z <= fz + 3) || ((6..=8).contains(&x) && (5..=7).contains(&y) && z == fz + 1);
            return (nose && fz >= 0).then(|| Vox { color: crate::atlas::hex(0xf07818), glow: 0.55 });
        }
        let row = 12 - y;
        let coal = row >= 0 && row < 13 && FACE[row as usize].as_bytes()[x as usize] == b'#';
        if coal && (z == fz || z == fz - 1) {
            return vox(eyes, eye_glow);
        }
        vox(0xf2f6ff, 0.42)
    })
}

fn snow_king_body() -> Model {
    // A great ball of snow with a striped scarf round its neck and coal buttons.
    Model::build(13, 9, 13, 0.5, |x, y, z| {
        let r = (((x as f32 - 6.0).powi(2) + (z as f32 - 6.0).powi(2)) as f32).sqrt();
        let max_r = (6.4f32.powi(2) - (y as f32 - 2.5).powi(2)).max(0.0).sqrt();
        if r > max_r {
            return None;
        }
        if y >= 7 {
            return if (x + z + y) % 4 < 2 { vox(0xd02028, 0.45) } else { vox(0x2a8a40, 0.45) };
        }
        if r < max_r - 1.2 {
            return None; // hollow inside
        }
        if z >= 11 && x == 6 && (y == 2 || y == 4) {
            return vox(0x202028, 0.0);
        }
        vox(0xeef4ff, 0.42)
    })
}

fn mitten() -> Model {
    Model::build(5, 5, 4, 0.42, |x, y, _| {
        if y == 0 {
            return (x < 4).then(|| Vox { color: crate::atlas::hex(0xf4f4f8), glow: 0.6 });
        }
        if x == 4 {
            return (y == 2 || y == 3).then(|| Vox { color: crate::atlas::hex(0xd02028), glow: 0.5 });
        }
        if y == 4 && (x == 0 || x == 3) {
            return None;
        }
        vox(0xd02028, 0.5)
    })
}

/// One leaf of the Snow King's doors, wrapped like a giant present: red paper with
/// white dots, a gold ribbon down the middle and across, and half of a big bow.
fn present_door(right: bool) -> Model {
    use crate::world::{in_doorway, DOOR_HALF};
    let vw = (DOOR_HALF * 2.0) as i32;
    let vh = 26;
    Model::build(vw, vh, 1, 0.5, move |ix, iy, _| {
        let k = if right { ix } else { vw - 1 - ix };
        let cx = k as f32 * 0.5 + 0.25;
        let cy = iy as f32 * 0.5 + 0.25;
        if !in_doorway(cx, cy, 0.0) {
            return None;
        }
        let edge = !in_doorway(cx, cy + 0.5, 0.0) || !in_doorway(cx + 0.5, cy, 0.0) || iy == 0;
        let by = 9.0;
        let lobe = (cx - 1.6).powi(2) / 1.9f32.powi(2) + (cy - by).powi(2) / 1.2f32.powi(2) <= 1.0;
        let knot = cx < 0.8 && (cy - by).abs() < 0.8;
        if edge {
            return vox(0xf0c040, 0.45);
        }
        if knot {
            return vox(0xc89020, 0.6);
        }
        if lobe {
            return vox(0xffe680, 0.8);
        }
        if k <= 1 || iy == 6 || iy == 7 {
            return vox(0xe8b830, 0.5);
        }
        if k % 4 == 2 && iy % 4 == 2 {
            return vox(0xf4f0f0, 0.3);
        }
        if k % 2 == 0 { vox(0xb01828, 0.2) } else { vox(0x9a1424, 0.2) }
    })
}
