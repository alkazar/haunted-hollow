//! Haunted Hollow: a graveyard, a haunted mansion (foyer, library, dining hall), a crypt
//! beneath it and the Pumpkin King's lair.

use super::*;
use crate::level::Level;

const DECO: WallDeco = WallDeco {
    art: [b::PORTRAIT, b::PORTRAIT2, b::PAINTING3],
    crest: (b::SHIELD, true),
    banner: (b::BANNER, true),
    sconce: b::TORCH,
    shelf: [b::POT, b::CANDLE, b::POT],
};

pub(super) fn build(w: &mut World, rng: &mut Rng, path: &[Vec3]) {
    ground(w, b::GRASS);
    graveyard(w, rng, path);
    house(w, rng);
    crypt(w, rng);
    lair(w, rng);
    decorate(w, rng, path);
    boss_doors(w);
    // Hang decorations on the bare walls of every indoor room.
    decorate_walls(w, rng, path, &DECO, -16, 16, 0, 18, 40, 70, b::WALL_PURPLE, &[4, 9, 14]);
    decorate_walls(w, rng, path, &DECO, -16, 16, 0, 18, 71, 100, b::WALL_GREEN, &[4, 9, 14]);
    decorate_walls(w, rng, path, &DECO, -16, 16, 0, 18, 101, 130, b::STONEBRICK, &[4, 9, 14]);
    decorate_walls(w, rng, path, &DECO, -14, 14, -18, -6, 140, 176, b::CRYPT, &[4, 8]);
    decorate_walls(w, rng, path, &DECO, -26, 26, -18, -1, 177, 214, b::PURPLE, &[5, 10]);
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
/// Fenced graveyard plots: x and z extents (inclusive), which way the headstones face,
/// and where the gate is.
const GRAVE_PLOTS: [(i32, i32, i32, i32, u8, (i32, i32)); 3] = [
    (8, 20, 7, 23, 1, (8, 14)),
    (-23, -13, -7, 13, 0, (-13, 2)),
    (-10, 0, 26, 32, 5, (-5, 26)),
];

/// True on or within a block of a grave plot (where a tree would sprout from the graves).
fn in_grave_plot(x: i32, z: i32) -> bool {
    GRAVE_PLOTS.iter().any(|&(x0, x1, z0, z1, _, _)| x >= x0 - 1 && x <= x1 + 1 && z >= z0 - 1 && z <= z1 + 1)
}

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
    for &(x0, x1, z0, z1, face, gate) in &GRAVE_PLOTS {
        grave_plot(w, rng, path, x0, x1, z0, z1, face, gate);
    }
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
    // Dead trees (never growing up out of the graves).
    for &(tx, tz, h) in &[(-17, 4, 8), (22, 10, 7), (-12, 30, 9), (18, 31, 6), (-20, -4, 7), (12, -3, 6)] {
        if in_grave_plot(tx, tz) {
            continue;
        }
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

/// The grand red-and-gold doors behind the Pumpkin King, and the exit hall beyond.
/// (The door leaves themselves are animated models drawn by the game.)
fn boss_doors(w: &mut World) {
    let doors = Level::Halloween.doors();
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
            if doors.contains(cx, cy, 0.0) {
                w.set(x, y, 214, b::AIR);
            } else if doors.contains(cx, cy, 1.6) {
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
        if !near_track(path, x as f32 + 0.5, z as f32 + 0.5, 5.5) && !in_grave_plot(x, z) && w.is_air(x, 1, z) && w.is_air(x + 1, 2, z) {
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
    banners(w, -15, (12, 13), (44, 68), 6, b::BANNER);
    banners(w, 15, (13, 14), (43, 67), 6, b::BANNER);
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
    banners(w, -14, (13, 14), (74, 98), 7, b::BANNER);
    banners(w, 14, (13, 14), (77, 97), 7, b::BANNER);
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
    banners(w, -15, (11, 12), (103, 129), 5, b::BANNER);
    banners(w, 15, (12, 13), (103, 129), 5, b::BANNER);
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
    banners(w, -13, (-10, -9), (143, 175), 6, b::BANNER);
    banners(w, 13, (-10, -9), (146, 175), 6, b::BANNER);

    // ---------------- Pumpkin King's lair
    scatter(w, rng, 160, (-25, 25), -17, (178, 213), &[b::GRASS], &ground);
    banners(w, -25, (-6, -5), (180, 212), 6, b::BANNER);
    banners(w, 25, (-6, -5), (180, 212), 6, b::BANNER);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_trees_grow_out_of_the_grave_plots() {
        let track = Track::new(Level::Halloween);
        let w = generate(&track, Level::Halloween);
        for &(x0, x1, z0, z1, _, _) in &GRAVE_PLOTS {
            for z in z0..=z1 {
                for x in x0..=x1 {
                    assert!(!(w.get(x, 1, z) == b::LOG && w.get(x, 2, z) == b::LOG), "tree trunk in a grave plot at ({x}, {z})");
                }
            }
        }
    }

    #[test]
    fn porch_pillars_and_lanterns_survive_the_ride_tunnel() {
        let track = Track::new(Level::Halloween);
        let w = generate(&track, Level::Halloween);
        for x in [-8, 8] {
            for y in 1..=7 {
                assert_eq!(w.get(x, y, 34), b::LOG, "porch pillar at x {x} is missing its block at y {y}");
            }
            assert_eq!(w.get(x - x.signum(), 5, 36), b::LANTERN, "porch lantern by x {x} was carved away");
        }
    }
}
