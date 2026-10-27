//! Geometry helpers: cube faces, oriented boxes and small voxel models
//! (ghosts, bats, the Pumpkin King...) built from code.

use crate::atlas::{tile_uv, T_WHITE};
use crate::vk::Vertex;
use glam::{Mat4, Vec3};

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
        Model { quads, size: Vec3::new(w as f32, h as f32, d as f32) * scale }
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

/// All the voxel models used by targets and the boss. Models face +Z.
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
    pub cart: Model,
}

impl Models {
    pub fn new() -> Models {
        Models {
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
            boss_head: boss_head(0xffe84a),
            boss_head_angry: boss_head(0xff2a1a),
            boss_cape: boss_cape(),
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
            cart: cart(),
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
    const FACE: [&str; 7] = ["........", ".#....#.", ".######.", "........", "...##...", ".##..##.", ".###.##."];
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
    const FACE: [&str; 13] = [
        "...............",
        "...............",
        "...............",
        "...##.....##...",
        "..####...####..",
        "..#####.#####..",
        "...............",
        "......###......",
        ".##.........##.",
        ".#############.",
        "..###.###.###..",
        "....#######....",
        "...............",
    ];
    let inside = |x: i32, y: i32, z: i32| -> bool {
        let dx = (x as f32 - 7.0) / 7.6;
        let dy = (y as f32 - 6.0) / 6.6;
        let dz = (z as f32 - 7.0) / 7.6;
        dx * dx + dy * dy + dz * dz <= 1.0
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
        if carved && z == fz {
            return None;
        }
        if carved && z == fz - 1 {
            return vox(glow, 1.0);
        }
        let ang = (z as f32 - 7.0).atan2(x as f32 - 7.0);
        let ridge = ((ang / std::f32::consts::TAU * 10.0).rem_euclid(1.0)) < 0.22;
        if ridge {
            vox(0xc0580e, 0.3)
        } else {
            vox(0xf0841a, 0.3)
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

fn cart() -> Model {
    // The front of the ride cart, seen at the bottom of the screen.
    Model::build(16, 6, 6, 0.11, |x, y, z| {
        if y == 5 && (x == 0 || x == 15 || z == 5) {
            return vox(0xd8a020, 0.6); // gold trim
        }
        if z == 5 && y >= 1 && y <= 3 && (x == 6 || x == 9) && y == 3 {
            return vox(0xffd84a, 1.0);
        }
        if z == 5 && y == 2 && (6..=9).contains(&x) {
            return vox(0xffd84a, 1.0);
        }
        if y < 5 && z < 4 && x > 0 && x < 15 && y > 0 {
            return None; // seat well
        }
        if z == 5 && (5..=10).contains(&x) && y >= 1 {
            return vox(0xe07a12, 0.6); // pumpkin badge
        }
        vox(0x7a1424, 0.6)
    })
}
