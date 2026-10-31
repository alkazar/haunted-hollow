//! `--export-ps1 DIR` and `--export-wii DIR`: write everything the console ports
//! (`ps1/`, `wii/`, sharing `shared/`) need as binary data, so they ride through the
//! very same world, with the same targets, models, textures, music and sound effects.
//!
//! The formats are the same for both consoles (see `Target` for what differs), except
//! for textures and sound. The rest of this describes the PS1, the tighter of the two.
//!
//! The world is far too big for the PS1's 2 MB of RAM, so it is streamed from the CD
//! as the cart moves. Only the faces a rider can see from the track are kept; they are
//! packed into "meshlets" of exactly one CD sector (2048 bytes), stored on the disc in
//! the order the ride first needs them, and each 2 m stretch of track lists the
//! meshlets in view from it. The PS1 reads ahead and drops meshlets once the cart has
//! passed the last stretch that sees them.
//!
//! Files are in the console's byte order (little-endian for the PS1, big-endian for
//! the Wii). World positions are in units of 1/`UNIT` block; directions are 4.12
//! fixed point. The C side reads them with the structs in `shared/data.h`, which must
//! match the layouts written here.

use crate::atlas;
use crate::game::{Game, Kind, SpawnRecord};
use crate::mesh::{Model, Models};
use crate::synth::{self, Drum, Ev, Inst};
use crate::track::{Track, RIDE_SECONDS};
use crate::vk::Vertex;
use crate::world::{zone_at, World};
use glam::{Vec2, Vec3};
use std::collections::HashMap;

/// PS1 world units per block.
const UNIT: f32 = 64.0;
/// Model units per block (models are small, so they get finer steps).
const MODEL_UNIT: f32 = 256.0;
/// Chunk size in blocks (faces are grouped by chunk before packing into meshlets).
const CHUNK: f32 = 8.0;
/// Track length covered by one entry of the visibility table, in metres.
const SEG_LEN: f32 = 2.0;
/// Segments of track the PS1 reads ahead.
const LOOKAHEAD: usize = 8;
/// What differs between the consoles' data.
pub struct Target {
    /// Big-endian (Wii) or little-endian (PS1).
    big_endian: bool,
    /// Merge neighbouring faces whose light blends smoothly into bigger ones (with
    /// repeating tiles). The Wii's GPU can't repeat a tile of the atlas, and doesn't
    /// need fewer faces anyway.
    merge: bool,
    /// Most riders (one schedule of targets is recorded for each number of riders).
    players: usize,
    /// Faces further than this from every camera on the ride are dropped (the
    /// console's fog ends sooner); a second, longer distance for the Pumpkin King's
    /// lair, so the doors behind him show.
    view: f32,
    view_lair: f32,
}

/// The PS1's fog closes in at 15 to 20 blocks (30 in the lair).
pub const PS1: Target = Target { big_endian: false, merge: true, players: 2, view: 24.0, view_lair: 36.0 };
/// The Wii draws much further (its fog ends at 46 blocks).
pub const WII: Target = Target { big_endian: true, merge: false, players: 4, view: 48.0, view_lair: 48.0 };

impl Target {
    fn max_view(&self, eye: Vec3) -> f32 {
        if eye.z > 176.0 { self.view_lair } else { self.view }
    }
}
/// Bytes per meshlet (one CD sector).
const SECTOR: usize = 2048;
const MESHLET_HEADER: usize = 28;
/// Camera samples per second of ride.
const CAM_HZ: f32 = 20.0;
/// Screen the PS1 shows (4:3), which decides where targets may appear.
const SCREEN: Vec2 = Vec2::new(640.0, 480.0);
/// Sample rate of all sounds on the PS1's sound chip.
const SPU_RATE: f32 = 22050.0;

/// A file being written, in the console's byte order.
struct Out(Vec<u8>, bool);

impl Out {
    fn new(t: &Target) -> Out {
        Out(vec![], t.big_endian)
    }
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&if self.1 { v.to_be_bytes() } else { v.to_le_bytes() });
    }
    fn i16(&mut self, v: i16) {
        self.u16(v as u16);
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&if self.1 { v.to_be_bytes() } else { v.to_le_bytes() });
    }
    fn i32(&mut self, v: i32) {
        self.u32(v as u32);
    }
    /// Overwrites a u32 written earlier at `at`.
    fn patch_u32(&mut self, at: usize, v: u32) {
        let b = if self.1 { v.to_be_bytes() } else { v.to_le_bytes() };
        self.0[at..at + 4].copy_from_slice(&b);
    }
    fn tag(&mut self, t: &[u8; 4]) {
        self.0.extend_from_slice(t);
    }
    fn align(&mut self, n: usize) {
        while self.0.len() % n != 0 {
            self.0.push(0);
        }
    }
    fn save(&self, dir: &str, name: &str) -> std::io::Result<()> {
        eprintln!("  {name}: {} KB", self.0.len().div_ceil(1024));
        std::fs::write(format!("{dir}/{name}"), &self.0)
    }
}

fn i16_of(v: f32) -> i16 {
    let r = v.round();
    assert!((-32768.0..=32767.0).contains(&r), "{v} doesn't fit in 16 bits");
    r as i16
}

fn fixed(v: f32) -> i16 {
    i16_of(v * 4096.0)
}

fn byte(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

fn ms(t: f32) -> u32 {
    (t * 1000.0).round().max(0.0) as u32
}

// ============================================================================ world

struct Quad {
    p: [Vec3; 4],
    uv: [[f32; 2]; 4],
    col: [[f32; 4]; 4],
}

fn quads(v: &[Vertex]) -> Vec<Quad> {
    v.chunks_exact(4)
        .map(|q| Quad {
            p: [0, 1, 2, 3].map(|i| Vec3::from(q[i].pos)),
            uv: [0, 1, 2, 3].map(|i| q[i].uv),
            col: [0, 1, 2, 3].map(|i| q[i].color),
        })
        .collect()
}

impl Quad {
    fn centre(&self) -> Vec3 {
        (self.p[0] + self.p[1] + self.p[2] + self.p[3]) * 0.25
    }
    fn normal(&self) -> Vec3 {
        (self.p[1] - self.p[0]).cross(self.p[3] - self.p[0]).normalize_or_zero()
    }
    /// The atlas tile, and for each corner whether it takes the tile's right edge
    /// (bit 0) and bottom edge (bit 1), packed two bits per corner.
    fn tile_uv(&self) -> (u8, u8) {
        let umin = self.uv.iter().map(|u| u[0]).fold(f32::MAX, f32::min);
        let vmin = self.uv.iter().map(|u| u[1]).fold(f32::MAX, f32::min);
        let size = atlas::SIZE as f32;
        let (tx, ty) = ((umin * size / 16.0).floor() as u32, (vmin * size / 16.0).floor() as u32);
        let mid = |t: u32| (t as f32 * 16.0 + 8.0) / size;
        let mut code = 0u8;
        for (k, uv) in self.uv.iter().enumerate() {
            let bits = (uv[0] > mid(tx)) as u8 | ((uv[1] > mid(ty)) as u8) << 1;
            code |= bits << (k * 2);
        }
        ((ty * 16 + tx) as u8, code)
    }
}

/// Vertex colour as (r, g, b, lantern) with 128 = 1.0 for the colour (the PS1's
/// texture blending scale) and 255 = full lantern response.
fn colour(c: [f32; 4]) -> [u8; 4] {
    [byte(c[0] * 128.0), byte(c[1] * 128.0), byte(c[2] * 128.0), byte(c[3].min(1.0) * 255.0)]
}

/// Walks the voxel grid from `a` to `b`; true if no opaque block is in the way.
/// The cells holding the two ends don't count.
fn clear(world: &World, a: Vec3, b: Vec3) -> bool {
    let d = b - a;
    let len = d.length();
    if len < 1e-4 {
        return true;
    }
    let dir = d / len;
    let mut cell = a.floor().as_ivec3();
    let end = b.floor().as_ivec3();
    let step = dir.signum().as_ivec3();
    let inv = |x: f32| if x.abs() < 1e-9 { f32::INFINITY } else { 1.0 / x.abs() };
    let first = |c: i32, s: i32, p: f32, d: f32| {
        if s == 0 || d == 0.0 {
            f32::INFINITY
        } else {
            let edge = if s > 0 { (c + 1) as f32 } else { c as f32 };
            (edge - p).abs() * inv(d)
        }
    };
    let mut t_max = Vec3::new(first(cell.x, step.x, a.x, dir.x), first(cell.y, step.y, a.y, dir.y), first(cell.z, step.z, a.z, dir.z));
    let t_delta = Vec3::new(inv(dir.x), inv(dir.y), inv(dir.z));
    for _ in 0..512 {
        if t_max.x < t_max.y && t_max.x < t_max.z {
            if t_max.x > len {
                return true;
            }
            cell.x += step.x;
            t_max.x += t_delta.x;
        } else if t_max.y < t_max.z {
            if t_max.y > len {
                return true;
            }
            cell.y += step.y;
            t_max.y += t_delta.y;
        } else {
            if t_max.z > len {
                return true;
            }
            cell.z += step.z;
            t_max.z += t_delta.z;
        }
        if cell == end {
            return true;
        }
        if world.solid_at(cell.as_vec3() + 0.5) {
            return false;
        }
    }
    true
}

/// How far along the track the whole show goes: the ride, then the outro.
fn show_end(track: &Track) -> f32 {
    (track.stop_dist + 2.6 * 4.8 * 4.8).min(track.total)
}

/// Eye positions for each segment of track (every half metre).
fn segment_cameras(track: &Track) -> Vec<Vec<Vec3>> {
    let end = show_end(track);
    let n = (end / SEG_LEN).ceil() as usize + 1;
    let mut segs = vec![vec![]; n];
    let mut d = 0.0;
    while d <= end + 0.01 {
        let s = ((d / SEG_LEN) as usize).min(n - 1);
        let base = track.frame_at(d).pos;
        segs[s].push(base + Vec3::Y * 1.65);
        if d == 0.0 {
            segs[s].push(base + Vec3::Y * 1.9); // the title screen's camera
        }
        d += 0.5;
    }
    segs
}

/// For each segment of track, which quads can be seen from any camera in it.
fn visibility(t: &Target, world: &World, segs: &[Vec<Vec3>], qs: &[Quad]) -> Vec<Vec<u32>> {
    let probes: Vec<(Vec3, Vec3, [Vec3; 5])> = qs
        .iter()
        .map(|q| {
            let c = q.centre();
            let n = q.normal();
            // Points just in front of the face: its centre and near each corner.
            let lift = n * 0.02;
            let pts = [c + lift, q.p[0].lerp(c, 0.1) + lift, q.p[1].lerp(c, 0.1) + lift, q.p[2].lerp(c, 0.1) + lift, q.p[3].lerp(c, 0.1) + lift];
            (c, n, pts)
        })
        .collect();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let per = segs.len().div_ceil(threads);
    std::thread::scope(|s| {
        let handles: Vec<_> = segs
            .chunks(per)
            .map(|group| {
                let probes = &probes;
                s.spawn(move || {
                    group
                        .iter()
                        .map(|cams| {
                            let mut vis = vec![];
                            for (qi, (c, n, pts)) in probes.iter().enumerate() {
                                let seen = cams.iter().any(|&eye| {
                                    let to = eye - *c;
                                    if to.length() > t.max_view(eye) || to.dot(*n) <= 0.0 {
                                        return false;
                                    }
                                    pts.iter().any(|&p| clear(world, eye, p))
                                });
                                if seen {
                                    vis.push(qi as u32);
                                }
                            }
                            vis
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
    })
}

struct Meshlet {
    origin: [i16; 3],
    verts: Vec<([i16; 3], u16)>,
    /// (corners, tile, uv code, tile repeats, facing group)
    quads: Vec<([u8; 4], u8, u8, u8, u8)>,
    segs: Vec<usize>,
}

/// A face as it goes into a meshlet: possibly several equal neighbouring faces
/// merged into one, whose tile repeats `rep` times across (low 4 bits) and down.
struct PQuad {
    p: [Vec3; 4],
    col: [[u8; 4]; 4],
    tile: u8,
    uv: u8,
    rep: u8,
    group: u8,
    segs: Vec<usize>,
}

/// How far (on the 128 = 1.0 scale) a corner's colour may stray from the smooth
/// blend across a merged face.
const MERGE_TOLERANCE: i32 = 4;
/// Most tiles a merged face may span each way.
const MERGE_MAX: i32 = 8;

/// Merges neighbouring faces of a chunk that lie in the same plane, with the same
/// tile, into rectangles, as long as the light across them is smooth enough for the
/// GPU's colour blend to reproduce it. Fewer, bigger faces are much cheaper for the
/// PS1; their tiles repeat with the GPU's texture window.
fn merge_faces(t: &Target, qs: &[Quad], list: &[usize], quad_segs: &[Vec<usize>], first_track: usize) -> Vec<PQuad> {
    let mut out = vec![];
    type Key = (glam::IVec3, glam::IVec3, glam::IVec3, i32, u8, u8);
    let mut groups: HashMap<Key, HashMap<(i32, i32), usize>> = HashMap::new();
    for &qi in list {
        let q = &qs[qi];
        let col = q.col.map(colour);
        let (tile, uv) = q.tile_uv();
        let r = q.p[1] - q.p[0];
        let u = q.p[3] - q.p[0];
        let unit = |v: Vec3| (v.length() - 1.0).abs() < 1e-4 && v.abs().max_element() > 0.999;
        let int = q.p.iter().all(|p| (*p - p.round()).abs().max_element() < 1e-4);
        if t.merge && qi < first_track && int && unit(r) && unit(u) {
            let (ri, ui) = (r.round().as_ivec3(), u.round().as_ivec3());
            let n = ri.cross(ui);
            let p0 = q.p[0].round().as_ivec3();
            groups.entry((n, ri, ui, p0.dot(n), tile, uv)).or_default().insert((p0.dot(ri), p0.dot(ui)), qi);
            continue;
        }
        out.push(PQuad { p: q.p, col, tile, uv, rep: 0x11, group: face_group(q, qi >= first_track), segs: quad_segs[qi].clone() });
    }
    let mut keys: Vec<_> = groups.keys().copied().collect();
    keys.sort_by_key(|k| (k.0.to_array(), k.1.to_array(), k.2.to_array(), k.3, k.4, k.5));
    for key in keys {
        let cells = &groups[&key];
        let (_, r, u, ..) = key;
        // Corner colours of a cell: (0,0), (1,0), (1,1), (0,1) in (r, u) steps.
        let corner = |c: (i32, i32), k: usize| colour(qs[cells[&c]].col[k]);
        let mut order: Vec<(i32, i32)> = cells.keys().copied().collect();
        order.sort_by_key(|&(i, j)| (j, i));
        let mut used = std::collections::HashSet::new();
        for (i0, j0) in order {
            if used.contains(&(i0, j0)) {
                continue;
            }
            let free = |c: (i32, i32), used: &std::collections::HashSet<(i32, i32)>| cells.contains_key(&c) && !used.contains(&c);
            // Every cell corner within tolerance of the blend of the rectangle's corners.
            let smooth = |a: i32, b: i32| {
                let c = [corner((i0, j0), 0), corner((i0 + a - 1, j0), 1), corner((i0 + a - 1, j0 + b - 1), 2), corner((i0, j0 + b - 1), 3)];
                for j in 0..b {
                    for i in 0..a {
                        for (k, (di, dj)) in [(0, 0), (1, 0), (1, 1), (0, 1)].into_iter().enumerate() {
                            let (s, t) = ((i + di) as f32 / a as f32, (j + dj) as f32 / b as f32);
                            let got = corner((i0 + i, j0 + j), k);
                            for ch in 0..4 {
                                let blend = c[0][ch] as f32 * (1.0 - s) * (1.0 - t)
                                    + c[1][ch] as f32 * s * (1.0 - t)
                                    + c[2][ch] as f32 * s * t
                                    + c[3][ch] as f32 * (1.0 - s) * t;
                                let tol = if ch == 3 { MERGE_TOLERANCE * 4 } else { MERGE_TOLERANCE };
                                if (blend - got[ch] as f32).abs() > tol as f32 {
                                    return false;
                                }
                            }
                        }
                    }
                }
                true
            };
            let mut a = 1;
            while a < MERGE_MAX && free((i0 + a, j0), &used) && smooth(a + 1, 1) {
                a += 1;
            }
            let mut b = 1;
            while b < MERGE_MAX && (0..a).all(|i| free((i0 + i, j0 + b), &used)) && smooth(a, b + 1) {
                b += 1;
            }
            let mut segs = vec![];
            for j in 0..b {
                for i in 0..a {
                    used.insert((i0 + i, j0 + j));
                    segs.extend(&quad_segs[cells[&(i0 + i, j0 + j)]]);
                }
            }
            segs.sort();
            segs.dedup();
            let q = &qs[cells[&(i0, j0)]];
            let (ra, ub) = ((r * a).as_vec3(), (u * b).as_vec3());
            let p = [q.p[0], q.p[0] + ra, q.p[0] + ra + ub, q.p[0] + ub];
            let col = [corner((i0, j0), 0), corner((i0 + a - 1, j0), 1), corner((i0 + a - 1, j0 + b - 1), 2), corner((i0, j0 + b - 1), 3)];
            out.push(PQuad { p, col, tile: key.4, uv: key.5, rep: (a | b << 4) as u8, group: face_group(q, false), segs });
        }
    }
    out
}

impl Meshlet {
    fn bytes(&self) -> usize {
        MESHLET_HEADER + self.verts.len() * 8 + self.quads.len() * 8
    }
}

/// Faces smaller than this (in square blocks) are details the PS1 skips from afar.
const DETAIL_AREA: f32 = 0.2;

/// The group a face goes in: 0 +x, 1 -x, 2 +y, 3 -y, 4 +z, 5 -z (which way it
/// points), 6 the track's slanted faces, 7 small details and crossed plants. The PS1 skips whole groups that face
/// away from the eye, and the details when they're far off.
fn face_group(q: &Quad, track: bool) -> u8 {
    let area = (q.p[1] - q.p[0]).cross(q.p[3] - q.p[0]).length();
    if area < DETAIL_AREA && !track {
        return 7;
    }
    let n = q.normal();
    let axes = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z];
    match axes.iter().position(|a| a.dot(n) > 0.999) {
        Some(g) => g as u8,
        // Crossed plants and cobwebs are details too; the track's slanted faces aren't.
        None if !track => 7,
        None => 6,
    }
}

/// Packs the visible quads into meshlets and writes WORLD.BIN (the meshlets, for the
/// CD) and pvs.bin (colour palette, each meshlet's span of segments, and the meshlets
/// to draw from each segment).
fn export_world(t: &Target, dir: &str, world: &World, track: &Track) -> std::io::Result<()> {
    let qs = quads(&world.build_world_mesh(track));
    let first_track = qs.len() - world.track_mesh(track).len() / 4;
    let segs = segment_cameras(track);
    let vis = visibility(t, world, &segs, &qs);
    let n_segs = vis.len();
    // Each quad's segments (a quad seen from one segment is drawn from its neighbours
    // too, so a camera between two segments never misses anything).
    let mut quad_segs: Vec<Vec<usize>> = vec![vec![]; qs.len()];
    for (si, v) in vis.iter().enumerate() {
        for &q in v {
            quad_segs[q as usize].extend(si.saturating_sub(1)..=(si + 1).min(n_segs - 1));
        }
    }
    let mut palette: Vec<[u8; 4]> = vec![];
    let mut pal_index: HashMap<[u8; 4], u16> = HashMap::new();
    let mut chunks: std::collections::BTreeMap<(i32, i32, i32), Vec<usize>> = Default::default();
    for (qi, q) in qs.iter().enumerate() {
        if quad_segs[qi].is_empty() {
            continue;
        }
        let c = (q.centre() / CHUNK).floor().as_ivec3();
        chunks.entry((c.z, c.y, c.x)).or_default().push(qi);
    }
    let mut meshlets: Vec<Meshlet> = vec![];
    for ((cz, cy, cx), list) in &chunks {
        let origin = [(*cx as f32 * CHUNK * UNIT) as i16, (*cy as f32 * CHUNK * UNIT) as i16, (*cz as f32 * CHUNK * UNIT) as i16];
        let new = || Meshlet { origin, verts: vec![], quads: vec![], segs: vec![] };
        let mut m = new();
        let mut lookup: HashMap<([i16; 3], u16), u8> = HashMap::new();
        for q in merge_faces(t, &qs, list, &quad_segs, first_track) {
            let keys: [([i16; 3], u16); 4] = [0, 1, 2, 3].map(|k| {
                let p = q.p[k] * UNIT;
                let pos = [i16_of(p.x - origin[0] as f32), i16_of(p.y - origin[1] as f32), i16_of(p.z - origin[2] as f32)];
                let c = q.col[k];
                let ci = *pal_index.entry(c).or_insert_with(|| {
                    palette.push(c);
                    (palette.len() - 1) as u16
                });
                (pos, ci)
            });
            let fresh = keys.iter().filter(|k| !lookup.contains_key(*k)).count();
            if m.verts.len() + fresh > 255 || m.bytes() + fresh * 8 + 8 > SECTOR || m.quads.len() == 255 {
                meshlets.push(std::mem::replace(&mut m, new()));
                lookup.clear();
            }
            let idx = keys.map(|k| {
                *lookup.entry(k).or_insert_with(|| {
                    m.verts.push(k);
                    (m.verts.len() - 1) as u8
                })
            });
            m.quads.push((idx, q.tile, q.uv, q.rep, q.group));
            m.segs.extend(&q.segs);
        }
        if !m.quads.is_empty() {
            meshlets.push(m);
        }
    }
    for m in meshlets.iter_mut() {
        m.segs.sort();
        m.segs.dedup();
        m.quads.sort_by_key(|q| q.4);
    }
    // Disc order = the order the ride first needs them.
    meshlets.sort_by_key(|m| (m.segs[0], *m.segs.last().unwrap()));
    let mut seg_lists: Vec<Vec<u16>> = vec![vec![]; n_segs];
    let mut img = Out::new(t);
    for (mi, m) in meshlets.iter().enumerate() {
        for &s in &m.segs {
            seg_lists[s].push(mi as u16);
        }
        let start = img.0.len();
        for v in m.origin {
            img.i16(v);
        }
        img.u8(m.verts.len() as u8);
        img.u8(m.quads.len() as u8);
        for axis in 0..3 {
            img.i16(m.verts.iter().map(|v| v.0[axis]).min().unwrap());
        }
        for axis in 0..3 {
            img.i16(m.verts.iter().map(|v| v.0[axis]).max().unwrap());
        }
        for g in 0..8 {
            img.u8(m.quads.iter().filter(|q| q.4 == g).count() as u8);
        }
        for (p, c) in &m.verts {
            p.iter().for_each(|&v| img.i16(v));
            img.u16(*c);
        }
        for (idx, tile, uv, rep, _) in &m.quads {
            idx.iter().for_each(|&i| img.u8(i));
            img.u8(*tile);
            img.u8(*uv);
            img.u8(*rep);
            img.u8(0);
        }
        assert!(img.0.len() - start <= SECTOR);
        img.0.resize(start + SECTOR, 0);
    }
    // Most meshlets resident at once: from LOOKAHEAD segments before first needed to
    // one segment past the last that needs them (as the PS1 keeps them).
    let peak = (0..n_segs)
        .map(|s| meshlets.iter().filter(|m| m.segs[0].saturating_sub(LOOKAHEAD) <= s && s <= *m.segs.last().unwrap() + 1).count())
        .max()
        .unwrap_or(0);
    let total_quads: usize = meshlets.iter().map(|m| m.quads.len()).sum();
    let most_drawn = seg_lists.iter().map(|l| l.iter().map(|&m| meshlets[m as usize].quads.len()).sum::<usize>()).max().unwrap_or(0);
    eprintln!(
        "  world: {} faces in {} meshlets, {} colours, {} segments; at most {} meshlets resident, {} faces listed",
        total_quads,
        meshlets.len(),
        palette.len(),
        n_segs,
        peak,
        most_drawn
    );
    img.save(dir, "WORLD.BIN")?;

    let mut o = Out::new(t);
    o.tag(b"PVS1");
    o.u16(meshlets.len() as u16);
    o.u16(n_segs as u16);
    o.u32(palette.len() as u32);
    o.u16(peak as u16);
    o.u16(LOOKAHEAD as u16);
    o.i32((SEG_LEN * UNIT) as i32);
    for c in &palette {
        c.iter().for_each(|&b| o.u8(b));
    }
    for m in &meshlets {
        o.u16(m.segs[0] as u16);
        o.u16(*m.segs.last().unwrap() as u16);
    }
    let mut off = 0u32;
    for l in &seg_lists {
        o.u32(off);
        off += l.len() as u32;
    }
    o.u32(off);
    for l in &seg_lists {
        l.iter().for_each(|&m| o.u16(m));
    }
    o.align(4);
    o.save(dir, "pvs.bin")
}

// ============================================================================ camera

/// track.bin: the camera for every 1/20 s of the ride and the outro, plus fixed poses.
fn export_track(t: &Target, out_dir: &str, game: &Game, track: &Track) -> std::io::Result<()> {
    let mut o = Out::new(t);
    let n_ride = (RIDE_SECONDS * CAM_HZ) as usize + 1;
    let outro_time = 4.8f32;
    let n_outro = (outro_time * CAM_HZ) as usize + 1;
    o.tag(b"TRK1");
    o.u16(n_ride as u16);
    o.u16(n_outro as u16);
    o.u16(CAM_HZ as u16);
    o.u16(0);
    let pose = |o: &mut Out, eye: Vec3, dir: Vec3, up: Vec3, d: f32, speed: f32| {
        for v in eye.to_array() {
            o.i32((v * UNIT * 4.0).round() as i32);
        }
        dir.to_array().iter().for_each(|&v| o.i16(fixed(v)));
        up.to_array().iter().for_each(|&v| o.i16(fixed(v)));
        o.u16((d / SEG_LEN) as u16);
        o.u8(zone_at(track.frame_at(d).pos) as u8);
        o.u8(byte(speed * 16.0));
    };
    // Fixed poses: the title camera's eye (it looks around in code), the boss stop,
    // and the end of the outro (results screens).
    let start = track.frame_at(0.0).pos + Vec3::Y * 1.9;
    pose(&mut o, start, Vec3::Z, Vec3::Y, 0.0, 0.0);
    let (eye, dir, up) = game.pose(track.stop_dist, 0.0);
    pose(&mut o, eye, dir, up, track.stop_dist, 0.0);
    let end = track.stop_dist + 2.6 * outro_time * outro_time;
    let (eye, dir, up) = game.pose(end, 0.0);
    pose(&mut o, eye, dir, up, end, 0.0);
    for i in 0..n_ride {
        let t = i as f32 / CAM_HZ;
        let d = track.dist_at_time(t);
        let speed = (track.dist_at_time(t + 0.1) - d) * 10.0;
        let (eye, dir, up) = game.ride_pose(t);
        pose(&mut o, eye, dir, up, d, speed);
    }
    // The outro: the cart rolls out through the doors, first turning to face them.
    let door = Vec3::new(0.0, crate::world::DOOR_FLOOR + 6.5, crate::world::DOOR_Z);
    for i in 0..n_outro {
        let t = i as f32 / CAM_HZ;
        let d = track.stop_dist + 2.6 * t * t;
        let speed = 2.0 * 2.6 * t;
        let (eye, mut dir, up) = game.pose(d, speed);
        let look = (door - eye).normalize_or_zero();
        if look.dot(dir) > 0.0 && eye.z < door.z - 2.0 {
            let k = (t / 1.5).clamp(0.0, 1.0);
            dir = look.lerp(dir, k * k * (3.0 - 2.0 * k)).normalize();
        }
        pose(&mut o, eye, dir, up, d, speed);
    }
    o.save(out_dir, "track.bin")
}

// ============================================================================ models

fn write_model(o: &mut Out, m: &Model) {
    let mut verts: Vec<[i16; 3]> = vec![];
    let mut lookup: HashMap<[i16; 3], u16> = HashMap::new();
    let mut faces = vec![];
    for (p, n, c, glow) in m.merged_faces() {
        let idx = p.map(|v| {
            let k = [i16_of(v.x * MODEL_UNIT), i16_of(v.y * MODEL_UNIT), i16_of(v.z * MODEL_UNIT)];
            *lookup.entry(k).or_insert_with(|| {
                verts.push(k);
                (verts.len() - 1) as u16
            })
        });
        let face = crate::mesh::FACES.iter().position(|f| Vec3::new(f.0[0] as f32, f.0[1] as f32, f.0[2] as f32) == n).unwrap();
        faces.push((idx, face as u8, byte(glow * 255.0), [byte(c[0] * 255.0), byte(c[1] * 255.0), byte(c[2] * 255.0)]));
    }
    o.u16(verts.len() as u16);
    o.u16(faces.len() as u16);
    m.size.to_array().iter().for_each(|&v| o.i16(i16_of(v * MODEL_UNIT)));
    o.u16(0);
    for v in &verts {
        v.iter().for_each(|&x| o.i16(x));
        o.i16(0);
    }
    for (idx, face, glow, c) in &faces {
        idx.iter().for_each(|&i| o.u16(i));
        o.u8(*face);
        o.u8(*glow);
        c.iter().for_each(|&b| o.u8(b));
        o.u8(0);
    }
}

/// models.bin: every voxel model, in the order of `enum model` in the C code.
fn export_models(t: &Target, dir: &str) -> std::io::Result<()> {
    let m = Models::new();
    let list: [&Model; 18] = [
        &m.ghost,
        &m.bat_body,
        &m.bat_wing,
        &m.pumpkin,
        &m.skull,
        &m.torso,
        &m.arm,
        &m.spider,
        &m.book_half[0],
        &m.book_half[1],
        &m.book_half[2],
        &m.candy,
        &m.boss_head,
        &m.boss_head_angry,
        &m.boss_cape,
        &m.boss_hand,
        &m.door[0],
        &m.door[1],
    ];
    let mut o = Out::new(t);
    o.tag(b"MDL1");
    o.u32(list.len() as u32);
    let table = o.0.len();
    for _ in &list {
        o.u32(0);
    }
    for (i, model) in list.iter().enumerate() {
        o.align(4);
        let off = o.0.len() as u32;
        o.patch_u32(table + i * 4, off);
        write_model(&mut o, model);
    }
    o.save(dir, "models.bin")
}

// ============================================================================ spawns

fn write_spawn(o: &mut Out, s: &SpawnRecord, rel: Vec3) {
    o.u32(ms(s.t));
    o.u8(s.kind as u8);
    o.u8(s.variant as u8);
    o.u16((s.phase * 4096.0).round() as u16);
    o.u32(ms(s.life));
    for p in [s.home, s.a, s.b] {
        let p = (p - rel) * UNIT;
        p.to_array().iter().for_each(|&v| o.i16(i16_of(v)));
    }
    o.u16(0);
}

/// spawns.bin: the targets of a ride for each number of riders up to four (more
/// riders get more targets; 0 for counts the console doesn't have), and the helpers
/// the Pumpkin King summons for the most riders (relative to him).
fn export_spawns(t: &Target, dir: &str) -> std::io::Result<()> {
    let mut o = Out::new(t);
    o.tag(b"SPN2");
    let mut sets = vec![];
    for players in 1..=t.players {
        let track = Track::new();
        let world = crate::world::generate(&track);
        sets.push(Game::record_spawns(track, world, players, SCREEN));
    }
    let summons = &sets[t.players - 1].1;
    for n in 0..4 {
        o.u16(sets.get(n).map_or(0, |s| s.0.len()) as u16);
    }
    o.u16(summons.len() as u16);
    o.u16(0);
    for set in &sets {
        for s in &set.0 {
            write_spawn(&mut o, s, Vec3::ZERO);
        }
    }
    for s in summons {
        write_spawn(&mut o, s, s.boss.unwrap_or(Vec3::ZERO));
    }
    let counts: Vec<String> = sets.iter().map(|s| s.0.len().to_string()).collect();
    eprintln!(
        "  targets for 1, 2... riders: {}; {} summoned (of which {} bats)",
        counts.join(", "),
        summons.len(),
        summons.iter().filter(|s| s.kind == Kind::Bat).count()
    );
    o.save(dir, "spawns.bin")
}

// ============================================================================ light, sky, scenery

/// Light grid spacing in blocks, and its extent.
const LIGHT_STEP: i32 = 4;
const LIGHT_X: (i32, i32) = (-28, 28);
const LIGHT_Y: (i32, i32) = (-28, 16);
const LIGHT_Z: (i32, i32) = (-32, 300);

/// light.bin: the world's light (for lighting targets) on a coarse grid, as 5:6:5
/// with 1.0 = 16 (red, blue) or 32 (green) so it can go a little over 1.
fn export_light(t: &Target, dir: &str, world: &World) -> std::io::Result<()> {
    let mut o = Out::new(t);
    o.tag(b"LGT1");
    let n = |r: (i32, i32)| ((r.1 - r.0) / LIGHT_STEP + 1) as u16;
    for r in [LIGHT_X, LIGHT_Y, LIGHT_Z] {
        o.i16(r.0 as i16);
        o.u16(n(r));
    }
    o.u16(LIGHT_STEP as u16);
    o.u16(0);
    for z in 0..n(LIGHT_Z) as i32 {
        for y in 0..n(LIGHT_Y) as i32 {
            for x in 0..n(LIGHT_X) as i32 {
                let p = Vec3::new((LIGHT_X.0 + x * LIGHT_STEP) as f32, (LIGHT_Y.0 + y * LIGHT_STEP) as f32, (LIGHT_Z.0 + z * LIGHT_STEP) as f32) + 0.5;
                let l = world.light_at(p);
                let r = (l.x * 16.0).round().clamp(0.0, 31.0) as u16;
                let g = (l.y * 32.0).round().clamp(0.0, 63.0) as u16;
                let b = (l.z * 16.0).round().clamp(0.0, 31.0) as u16;
                o.u16(r | g << 5 | b << 11);
            }
        }
    }
    o.save(dir, "light.bin")
}

/// sky.bin: moon, halo and stars as quads in world units (never fogged).
fn export_sky(t: &Target, dir: &str) -> std::io::Result<()> {
    let qs = quads(&crate::world::sky_mesh());
    let mut o = Out::new(t);
    o.tag(b"SKY1");
    o.u32(qs.len() as u32);
    for q in &qs {
        for p in q.p {
            (p * UNIT).to_array().iter().for_each(|&v| o.i16(i16_of(v)));
            o.i16(0);
        }
        let (tile, uv) = q.tile_uv();
        o.u8(tile);
        o.u8(uv);
        let c = colour(q.col[0]);
        o.u8(c[0]);
        o.u8(c[1]);
        o.u8(c[2]);
        o.u8(0);
        o.u16(0);
    }
    o.save(dir, "sky.bin")
}

/// scenery.bin: flames, cauldrons, clouds and mist puffs, animated by the PS1.
fn export_scenery(t: &Target, dir: &str, game: &Game) -> std::io::Result<()> {
    let (fires, cauldrons, clouds, mist) = game.scenery();
    let mut o = Out::new(t);
    o.tag(b"SCN1");
    o.u16(fires.len() as u16);
    o.u16(cauldrons.len() as u16);
    o.u16(clouds.len() as u16);
    o.u16(mist.len() as u16);
    for p in fires.iter().chain(cauldrons.iter()) {
        (*p * UNIT).to_array().iter().for_each(|&v| o.i16(i16_of(v)));
        o.i16(0);
    }
    for &(x, z, len) in &clouds {
        o.i16(i16_of(x));
        o.i16(i16_of(z));
        o.i16(i16_of(len));
        o.i16(0);
    }
    for (pos, half, ph, alpha, tint, wander, rise, zones) in &mist {
        (*pos * UNIT).to_array().iter().for_each(|&v| o.i16(i16_of(v)));
        o.i16(i16_of(half.x * UNIT));
        o.i16(i16_of(half.y * UNIT));
        o.u16((ph * 4096.0) as u16);
        o.u8(byte(alpha * 255.0));
        o.u8(*zones);
        tint.iter().for_each(|&c| o.u8(byte(c * 255.0)));
        o.u8(byte(wander * 64.0));
        o.u16((rise * UNIT) as u16);
        o.u16(0);
    }
    o.save(dir, "scenery.bin")
}

/// ATLAS.BIN: the 256x256 texture atlas (block tiles and the font) as PS1 15-bit
/// colour. Clear pixels are 0 (the PS1 skips them); black is 0x8000.
fn export_atlas(t: &Target, dir: &str) -> std::io::Result<()> {
    let px = atlas::build();
    if t.big_endian {
        // The Wii takes the atlas as it is (RGBA, 8 bits each) and converts it itself.
        eprintln!("  ATLAS.BIN: {} KB", px.len() / 1024);
        return std::fs::write(format!("{dir}/ATLAS.BIN"), &px);
    }
    let mut o = Out::new(t);
    for p in px.chunks_exact(4) {
        let v = if p[3] < 128 {
            0
        } else {
            let c = (p[0] as u16 >> 3) | (p[1] as u16 >> 3) << 5 | (p[2] as u16 >> 3) << 10;
            if c == 0 { 0x8000 } else { c }
        };
        o.u16(v);
    }
    o.save(dir, "ATLAS.BIN")
}

// ============================================================================ sound

/// PS1 ADPCM: 28 samples per 16-byte block, after one silent block. `loop_start`
/// (a sample index, rounded down to a block) makes it loop back there at the end.
fn adpcm(samples: &[f32], loop_start: Option<usize>) -> Vec<u8> {
    const F: [(i32, i32); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];
    let pcm: Vec<i32> = samples.iter().map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i32).collect();
    let blocks = pcm.len().div_ceil(28).max(1);
    let mut out = vec![0u8; 16];
    let (mut s1, mut s2) = (0i32, 0i32);
    for b in 0..blocks {
        let block: Vec<i32> = (0..28).map(|i| *pcm.get(b * 28 + i).unwrap_or(&0)).collect();
        let mut best: Option<(i64, u8, u8, [u8; 14], i32, i32)> = None;
        for (fi, &(f0, f1)) in F.iter().enumerate() {
            for shift in 0..13u8 {
                let (mut p1, mut p2) = (s1, s2);
                let mut err = 0i64;
                let mut nib = [0u8; 14];
                for (i, &x) in block.iter().enumerate() {
                    let pred = (p1 * f0 + p2 * f1 + 32) >> 6;
                    let q = (((x - pred) << shift) as f32 / 4096.0).round().clamp(-8.0, 7.0) as i32;
                    let dec = (((q << 12) >> shift) + pred).clamp(-32768, 32767);
                    err += ((dec - x) as i64).pow(2);
                    p2 = p1;
                    p1 = dec;
                    nib[i / 2] |= ((q & 15) as u8) << ((i % 2) * 4);
                }
                if best.as_ref().map_or(true, |b| err < b.0) {
                    best = Some((err, fi as u8, shift, nib, p1, p2));
                }
            }
        }
        let (_, fi, shift, nib, p1, p2) = best.unwrap();
        s1 = p1;
        s2 = p2;
        let mut flags = 0u8;
        if loop_start.map(|l| l / 28) == Some(b) {
            flags |= 4;
        }
        if b == blocks - 1 {
            flags |= if loop_start.is_some() { 3 } else { 1 };
        }
        out.push(shift | fi << 4);
        out.push(flags);
        out.extend_from_slice(&nib);
    }
    out
}

/// Mixes a stereo buffer to mono and resamples it to the SPU rate.
fn to_mono(b: &synth::Buf) -> Vec<f32> {
    let step = synth::RATE / SPU_RATE;
    let n = (b.frames() as f32 / step) as usize;
    (0..n)
        .map(|i| {
            let j = (i as f32 * step) as usize;
            let (mut s, mut k) = (0.0, 0);
            for f in j..(j + step as usize).min(b.frames()) {
                s += (b.data[f * 2] + b.data[f * 2 + 1]) * 0.5;
                k += 1;
            }
            s / k.max(1) as f32
        })
        .collect()
}

fn peak(s: &[f32]) -> f32 {
    s.iter().fold(1e-6f32, |m, &v| m.max(v.abs()))
}

struct Sample {
    data: Vec<u8>,
    /// Multiply volumes by this (the sample was scaled up to full level).
    gain: f32,
    /// Frequency the sample plays at the SPU's 1.0 pitch (0x1000 = 44.1 kHz).
    base: f32,
}

fn sample(mono: Vec<f32>, loop_start: Option<usize>, base: f32) -> Sample {
    let k = 0.95 / peak(&mono);
    let scaled: Vec<f32> = mono.iter().map(|&v| v * k).collect();
    Sample { data: adpcm(&scaled, loop_start), gain: 1.0 / k, base }
}

const INSTS: [Inst; 8] = [Inst::Organ, Inst::Theremin, Inst::Xylo, Inst::Bass, Inst::Bell, Inst::Brass, Inst::Chip, Inst::Pad];
const DRUMS: [Drum; 5] = [Drum::Kick, Drum::Snare, Drum::Hat, Drum::Tom, Drum::Clap];

/// The reference pitch an instrument is sampled at (an exact number of SPU samples
/// per cycle, so loops line up).
fn inst_base(i: Inst) -> f32 {
    match i {
        Inst::Bass => SPU_RATE / 200.0,
        Inst::Bell | Inst::Xylo => SPU_RATE / 25.0,
        _ => SPU_RATE / 50.0,
    }
}

/// One note of an instrument at its reference pitch. Sustained instruments get a
/// crossfaded loop; the SPU's envelope does the release when the note ends.
fn inst_sample(i: Inst) -> Sample {
    let f0 = inst_base(i);
    let sustained = !matches!(i, Inst::Xylo | Inst::Bell);
    let len = if sustained { 1.4 } else { 1.0 };
    let mut b = synth::Buf::new(len + 0.1);
    synth::note(&mut b, 0.0, f0, if sustained { len + 1.0 } else { 0.95 }, i, 1.0, 0.0, false);
    let mut mono = to_mono(&b);
    mono.truncate((len * SPU_RATE) as usize);
    if !sustained {
        return sample(mono, None, f0);
    }
    // Loop the last stretch: a whole number of cycles and of ADPCM blocks, with the
    // end crossfaded into what comes before the loop start so there's no click.
    let period = (SPU_RATE / f0).round() as usize;
    let unit = period * 28 / gcd(period, 28);
    let loop_len = unit * (0.3 * SPU_RATE / unit as f32).ceil() as usize;
    let end = mono.len() / 28 * 28;
    let start = end - loop_len;
    let fade = loop_len / 2;
    let src = mono.clone();
    for k in 0..fade {
        let w = k as f32 / fade as f32;
        mono[end - fade + k] = src[end - fade + k] * (1.0 - w) + src[start - fade + k] * w;
    }
    mono.truncate(end);
    sample(mono, Some(start), f0)
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn drum_sample(d: Drum) -> Sample {
    let mut b = synth::Buf::new(0.35);
    synth::drum(&mut b, 0.0, d, 1.0, 0.0, false, 12345);
    sample(to_mono(&b), None, SPU_RATE)
}

/// SPU pitch register value to play a sample with reference frequency `base` at `freq`.
fn pitch(base: f32, freq: f32) -> u16 {
    (4096.0 * (SPU_RATE / 44100.0) * freq / base).round().clamp(1.0, 16383.0) as u16
}

/// Constant-power pan as in `synth::note`, as SPU volumes (0x3fff = 1.0).
fn pan_vol(vol: f32, pan: f32) -> (u16, u16) {
    let p = (pan.clamp(-1.0, 1.0) + 1.0) * 0.25 * std::f32::consts::PI;
    let v = |x: f32| (x * 16383.0).round().clamp(0.0, 16383.0) as u16;
    (v(vol * p.cos()), v(vol * p.sin()))
}

/// SOUND.BIN (the sound chip's samples, for the CD) and sound.bin (where each sample
/// is, and the songs as note lists).
fn export_sound(t: &Target, dir: &str) -> std::io::Result<()> {
    let mut samples: Vec<Sample> = INSTS.iter().map(|&i| inst_sample(i)).collect();
    samples.extend(DRUMS.iter().map(|&d| drum_sample(d)));
    let sfx = [
        synth::sfx_pop(),
        synth::sfx_shot(),
        synth::sfx_bonus(),
        synth::sfx_poof(),
        synth::sfx_ghost(),
        synth::sfx_squeak(),
        synth::sfx_rattle(),
        synth::sfx_zip(),
        synth::sfx_boing(),
        synth::sfx_flutter(),
        synth::sfx_beep(false),
        synth::sfx_beep(true),
        synth::sfx_join(),
        synth::sfx_whoosh(),
        synth::sfx_bonk(),
        synth::sfx_laugh(),
        synth::sfx_rumble(),
        synth::sfx_boom(),
        synth::sfx_ding(),
        synth::sfx_boing_deep(),
    ];
    let first_sfx = samples.len();
    for b in &sfx {
        samples.push(sample(to_mono(b), None, SPU_RATE));
    }
    let cart = to_mono(&synth::cart_loop());
    let cart_len = cart.len() / 28 * 28;
    samples.push(sample(cart[..cart_len].to_vec(), Some(0), SPU_RATE));

    let mut blob = Out::new(t);
    let mut table = vec![];
    for s in &samples {
        table.push((blob.0.len() as u32, s.data.len() as u32));
        blob.0.extend_from_slice(&s.data);
        blob.align(16);
    }
    eprintln!("  sound: {} samples, {} KB of sound memory", samples.len(), blob.0.len() / 1024);
    blob.save(dir, "SOUND.BIN")?;

    let mut o = Out::new(t);
    o.tag(b"SND1");
    o.u16(samples.len() as u16);
    o.u16(first_sfx as u16);
    o.u32(blob.0.len() as u32);
    for (s, (off, len)) in samples.iter().zip(&table) {
        o.u32(*off);
        o.u32(*len);
        o.u16(pitch(s.base, SPU_RATE));
        o.u16(0);
        // Volume scale (16.16): the sample was scaled up to full level.
        o.u32((s.gain * 65536.0) as u32);
    }
    // Songs: title, ride, boss, victory, results; each normalised as on the PC.
    let song = |s: &synth::Song| {
        let (ev, len) = synth::song_events(s);
        let rendered = synth::render_song(s, true);
        let mut raw = synth::Buf::new(len);
        synth::render_events(&mut raw, &ev, true);
        (ev, len, true, peak(&rendered.data) / peak(&raw.data))
    };
    let victory = {
        let v = synth::victory_events();
        let mut b = synth::Buf::new(3.5);
        synth::render_events(&mut b, &v, false);
        (v, 3.5, false, 0.85 / peak(&b.data))
    };
    let songs = [song(&synth::TITLE), song(&synth::RIDE), song(&synth::BOSS), victory, song(&synth::RESULTS)];
    // The PC plays music at 0.42.
    let music_vol = 0.42;
    let mut song_out = Out::new(t);
    let mut heads = vec![];
    for (events, len, looping, norm) in &songs {
        let mut evs: Vec<(u32, u16, u16, u8, u16, u16)> = events
            .iter()
            .map(|e| match *e {
                Ev::Note { t, freq, dur, inst, vol, pan } => {
                    let si = INSTS.iter().position(|&i| i == inst).unwrap();
                    let s = &samples[si];
                    let (l, r) = pan_vol(vol * norm * music_vol * s.gain, pan);
                    (ms(t), pitch(s.base, freq), ms(dur).min(65535) as u16, si as u8, l, r)
                }
                Ev::Drum { t, drum, vol, pan, .. } => {
                    let si = INSTS.len() + DRUMS.iter().position(|&d| d == drum).unwrap();
                    let s = &samples[si];
                    let (l, r) = pan_vol(vol * norm * music_vol * s.gain, pan);
                    (ms(t), pitch(s.base, SPU_RATE), 400, si as u8, l, r)
                }
            })
            .collect();
        evs.sort_by_key(|e| e.0);
        heads.push((song_out.0.len() as u32, evs.len() as u32, ms(*len), *looping));
        for (t, p, d, si, l, r) in evs {
            song_out.u32(t);
            song_out.u16(p);
            song_out.u16(d);
            song_out.u16(l);
            song_out.u16(r);
            song_out.u8(si);
            song_out.u8(0);
            song_out.u16(0);
        }
    }
    for (off, n, len, looping) in heads {
        o.u32(off);
        o.u32(n);
        o.u32(len);
        o.u32(looping as u32);
    }
    o.0.extend_from_slice(&song_out.0);
    o.save(dir, "sound.bin")
}

/// Wii sound: 16-bit big-endian PCM, mono. Effects keep more of their sparkle at a
/// higher rate; the songs are long, so they get a lower one.
const WII_SFX_RATE: f32 = 32000.0;
const WII_SONG_RATE: f32 = 24000.0;

/// Mixes a stereo buffer to mono at `rate` as 16-bit big-endian samples.
fn pcm_be(b: &synth::Buf, rate: f32, gain: f32) -> Vec<u8> {
    let step = synth::RATE / rate;
    let n = (b.frames() as f32 / step) as usize;
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let x = i as f32 * step;
        let (j, f) = (x as usize, x.fract());
        let at = |k: usize| {
            let k = k.min(b.frames() - 1);
            (b.data[k * 2] + b.data[k * 2 + 1]) * 0.5
        };
        let v = (at(j) * (1.0 - f) + at(j + 1) * f) * gain;
        out.extend_from_slice(&((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_be_bytes());
    }
    out
}

/// SOUND.BIN for the Wii: every effect, the five songs exactly as the PC renders them
/// (title, ride, boss, victory, results) and the cart's rattle, as PCM. Each entry is
/// (offset, bytes, sample rate, loops), offsets from the start of the file; the data
/// is 32-byte aligned for the Wii's sound library.
fn export_wii_sound(t: &Target, dir: &str) -> std::io::Result<()> {
    let bank = crate::audio::Bank::build();
    let mut clips: Vec<(Vec<u8>, f32, bool)> = vec![];
    for b in bank.sfx() {
        clips.push((pcm_be(b, WII_SFX_RATE, 1.0), WII_SFX_RATE, false));
    }
    for (b, looping) in bank.songs() {
        clips.push((pcm_be(b, WII_SONG_RATE, 1.0), WII_SONG_RATE, looping));
    }
    clips.push((pcm_be(bank.cart(), WII_SFX_RATE, 1.0), WII_SFX_RATE, true));
    let mut o = Out::new(t);
    o.tag(b"WSN1");
    o.u32(clips.len() as u32);
    let table = o.0.len();
    for _ in &clips {
        for _ in 0..4 {
            o.u32(0);
        }
    }
    for (i, (pcm, rate, looping)) in clips.iter().enumerate() {
        o.align(32);
        let at = table + i * 16;
        let off = o.0.len() as u32;
        o.patch_u32(at, off);
        o.patch_u32(at + 4, pcm.len() as u32);
        o.patch_u32(at + 8, *rate as u32);
        o.patch_u32(at + 12, *looping as u32);
        o.0.extend_from_slice(pcm);
    }
    o.align(32);
    o.save(dir, "SOUND.BIN")
}

/// Writes all the data for `t` into `dir`.
pub fn export(t: &Target, dir: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let t0 = std::time::Instant::now();
    eprintln!("exporting {} data to {dir}", if t.big_endian { "Wii" } else { "PS1" });
    let track = Track::new();
    let world = crate::world::generate(&track);
    export_world(t, dir, &world, &track)?;
    export_light(t, dir, &world)?;
    let game = Game::new(Track::new(), world, false, 0);
    export_track(t, dir, &game, &track)?;
    export_scenery(t, dir, &game)?;
    export_models(t, dir)?;
    export_sky(t, dir)?;
    export_atlas(t, dir)?;
    if t.big_endian {
        export_wii_sound(t, dir)?;
    } else {
        export_sound(t, dir)?;
    }
    export_spawns(t, dir)?;
    eprintln!("done in {:.1}s", t0.elapsed().as_secs_f32());
    Ok(())
}
