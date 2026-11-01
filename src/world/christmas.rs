//! Holly Hollow: a night ride from a station deep in a snowy pine forest, over a bridge
//! across a frozen creek, down the main street of Winterberry village, up a lift onto
//! its rooftops, down the giant chimney of the toy factory, through the factory and out
//! across the frozen lake, plunging through a crack in the ice into a crystal cavern and
//! on into the Snow King's throne room. (Its track is `XMAS_POINTS` in track.rs.)

use super::*;

const DECO: WallDeco = WallDeco {
    art: [b::XMAS_ART, b::XMAS_ART2, b::XMAS_ART3],
    crest: (b::WREATH, false),
    banner: (b::STOCKING, false),
    sconce: b::CANDLE,
    shelf: [b::PRESENT_RED, b::CANDLE, b::PRESENT_GREEN],
};

/// The toy factory: x and z extents of its outer walls (its floor is at y = 0).
const FACTORY: (i32, i32, i32, i32) = (-42, 10, 100, 150);
const FACTORY_TOP: i32 = 16;
/// The frozen lake (x and z extents).
const LAKE: (i32, i32, i32, i32) = (-44, 44, 152, 200);
/// The crystal cavern: x, y and z extents of its walls (floor block at y0).
const CAVERN: (i32, i32, i32, i32, i32, i32) = (-24, 20, -25, -12, 196, 230);
/// The throne room.
const THRONE: (i32, i32, i32, i32, i32, i32) = (-26, 26, -25, -6, 230, 268);

pub(super) fn build(w: &mut World, rng: &mut Rng, path: &[Vec3]) {
    ground(w, b::SNOW);
    lake(w, rng, path);
    creek(w, path);
    forest_station(w);
    village(w, rng, path);
    rooftops(w, rng);
    factory(w, rng);
    chimney(w);
    cavern(w, rng);
    throne_room(w, rng);
    throne_doors(w);
    let (x0, x1, z0, z1) = FACTORY;
    decorate_walls(w, rng, path, &DECO, x0 + 1, x1 - 1, 0, FACTORY_TOP - 1, z0 + 1, z1 - 1, b::WALL_RED, &[4, 9]);
    decorate_walls(w, rng, path, &DECO, THRONE.0, THRONE.1, THRONE.2, THRONE.3, THRONE.4, THRONE.5, b::ICE_BRICK, &[5, 10]);
    // Last, so they fill whatever open snow is left: the woods.
    woods(w, rng, path);
}

/// Open snow at least `r` blocks around (x, z), and the ride further than `r + gap` away.
fn open_snow(w: &World, path: &[Vec3], x: i32, z: i32, r: i32, gap: f32) -> bool {
    if near_track(path, x as f32 + 0.5, z as f32 + 0.5, r as f32 + gap) {
        return false;
    }
    (-r..=r).all(|dz| (-r..=r).all(|dx| w.get(x + dx, 0, z + dz) == b::SNOW && w.is_air(x + dx, 1, z + dz) && w.is_air(x + dx, 3, z + dz)))
}

/// A snowy pine tree `h` blocks tall standing on `y0`: tiers of needles with snow on
/// every ledge. `lights` is the chance of a string of fairy lights on each bit of its
/// outside; `star` puts a glowing star on top.
#[allow(clippy::too_many_arguments)]
fn pine_tree(w: &mut World, rng: &mut Rng, x: i32, y0: i32, z: i32, h: i32, lights: f32, star: bool) {
    let trunk = (h / 5).max(1);
    w.fill(x, x, y0, y0 + trunk - 1, z, z, b::LOG);
    let top = y0 + h - 1;
    let base_r = pine_radius(h);
    let span = (top - y0 - trunk).max(1) as f32;
    for y in y0 + trunk..=top {
        let u = (y - y0 - trunk) as f32 / span;
        // Three tiers, each wide at the bottom and narrower at the top.
        let v = (u * 3.0).fract();
        let r = (base_r - 0.35) * (1.0 - u * 0.85) * (1.0 - v * 0.4) + 0.35;
        let ri = r.ceil() as i32;
        for dz in -ri..=ri {
            for dx in -ri..=ri {
                if ((dx * dx + dz * dz) as f32).sqrt() <= r && w.is_air(x + dx, y, z + dz) {
                    w.set(x + dx, y, z + dz, b::PINE);
                }
            }
        }
    }
    w.set(x, top + 1, z, if star { b::XMAS_STAR } else { b::PINE });
    let ri = base_r.ceil() as i32 + 1;
    for y in y0 + trunk..=top + 1 {
        for dz in -ri..=ri {
            for dx in -ri..=ri {
                let (px, pz) = (x + dx, z + dz);
                if w.get(px, y, pz) != b::PINE {
                    continue;
                }
                if w.is_air(px, y + 1, pz) {
                    w.set(px, y, pz, b::PINE_SNOW);
                }
                if lights > 0.0 && rng.chance(lights) {
                    // Hang the lights on its outside, facing away from the trunk.
                    let (ox, oz) = if dx.abs() >= dz.abs() { (dx.signum(), 0) } else { (0, dz.signum()) };
                    if w.is_air(px + ox, y, pz + oz) && w.is_air(px + ox, y - 1, pz + oz) {
                        w.set(px + ox, y, pz + oz, b::LIGHTS);
                    }
                }
            }
        }
    }
}

/// How far a pine `h` tall spreads at the bottom.
fn pine_radius(h: i32) -> f32 {
    (h as f32 * 0.3).max(1.2) + 0.35
}

/// A house on the snow (outer walls x0..x1, z0..z1): `wall` walls `h` high with log
/// corners and glowing windows, a door with a wreath on the side facing `face` (a cube
/// face index), a stepped snowy red roof along its longer side with fairy lights under
/// the eaves, and a chimney.
#[allow(clippy::too_many_arguments)]
fn house(w: &mut World, x0: i32, x1: i32, z0: i32, z1: i32, face: u8, wall: u8, h: i32) {
    w.room(x0, x1, 0, h, z0, z1, wall, b::PLANK, b::PLANK);
    for (x, z) in [(x0, z0), (x0, z1), (x1, z0), (x1, z1)] {
        w.fill(x, x, 1, h, z, z, b::LOG);
    }
    // Rows of windows, every three blocks along each wall.
    for wy in (2..h - 1).step_by(4) {
        for x in (x0 + 2..x1 - 1).step_by(3) {
            for z in [z0, z1] {
                w.fill(x, x, wy, wy + 1, z, z, b::WINDOW_LIT);
            }
        }
        for z in (z0 + 2..z1 - 1).step_by(3) {
            for x in [x0, x1] {
                w.fill(x, x, wy, wy + 1, z, z, b::WINDOW_LIT);
            }
        }
    }
    let (mx, mz) = ((x0 + x1) / 2, (z0 + z1) / 2);
    let ((dx, dz), (ox, oz)) = match face {
        0 => ((x1, mz), (1, 0)),
        1 => ((x0, mz), (-1, 0)),
        4 => ((mx, z1), (0, 1)),
        _ => ((mx, z0), (0, -1)),
    };
    w.fill(dx, dx, 1, 2, dz, dz, b::DARKPLANK);
    w.set(dx + ox, 3, dz + oz, b::WREATH);
    let along_z = z1 - z0 >= x1 - x0;
    for k in 0.. {
        let y = h + 1 + k;
        let (xa, xb, za, zb) = if along_z { (x0 - 1 + k, x1 + 1 - k, z0 - 1, z1 + 1) } else { (x0 - 1, x1 + 1, z0 - 1 + k, z1 + 1 - k) };
        if xa > xb || za > zb {
            break;
        }
        w.fill(xa, xb, y, y, za, zb, b::ROOF_RED);
    }
    for x in x0 - 1..=x1 + 1 {
        for z in [z0 - 1, z1 + 1] {
            if w.is_air(x, h, z) {
                w.set(x, h, z, b::LIGHTS);
            }
        }
    }
    for z in z0..=z1 {
        for x in [x0 - 1, x1 + 1] {
            if w.is_air(x, h, z) {
                w.set(x, h, z, b::LIGHTS);
            }
        }
    }
    let (cx, cz) = (x1 - 1, z1 - 1);
    w.fill(cx, cx, h + 1, h + 5, cz, cz, b::BRICK);
    w.set(cx, h + 6, cz, b::SNOW_BLOCK);
}

/// A snowman: two balls of snow and a head looking toward `face`.
fn snowman(w: &mut World, x: i32, y: i32, z: i32, face: u8) {
    w.set(x, y, z, b::SNOW_BLOCK);
    w.set_meta(x, y + 1, z, b::SNOWMAN_HEAD, face);
}

fn lamp_post(w: &mut World, x: i32, y: i32, z: i32, h: i32) {
    w.fill(x, x, y, y + h - 1, z, z, b::FENCE);
    w.set(x, y + h, z, b::LANTERN);
}

/// Unit vectors along and to the right of the ride at path point `i` (level, ignoring slope).
fn along(path: &[Vec3], i: usize) -> (Vec3, Vec3) {
    let i = i.min(path.len() - 2);
    let f = Vec3::new(path[i + 1].x - path[i].x, 0.0, path[i + 1].z - path[i].z).normalize_or_zero();
    (f, f.cross(Vec3::Y).normalize_or_zero())
}

fn cell(p: Vec3) -> (i32, i32, i32) {
    (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
}

/// The middle of the frozen creek at x.
fn creek_z(x: i32) -> i32 {
    (8.0 + 3.0 * (x as f32 * 0.13).sin()).round() as i32
}

// ====================================================================== the forest

/// A frozen creek winding across the forest, and the little bridge the ride takes over it.
fn creek(w: &mut World, path: &[Vec3]) {
    for x in -64..40 {
        let cz = creek_z(x);
        for z in cz - 2..=cz + 2 {
            w.set(x, 0, z, b::ICE);
            w.set(x, -1, z, b::ICE);
        }
    }
    // The bridge: plank deck under the rails and a railing either side with lanterns.
    for (i, p) in path.iter().enumerate() {
        if p.y > 2.5 || p.x > -15.0 || !(0.0..18.0).contains(&p.z) {
            continue;
        }
        let (_, r) = along(path, i);
        let over = |q: Vec3| (q.z.floor() as i32 - creek_z(q.x.floor() as i32)).abs() <= 4;
        for k in -8..=8 {
            let q = *p + r * (k as f32 * 0.3);
            if over(q) {
                let (x, _, z) = cell(q);
                w.set(x, 0, z, b::DARKPLANK);
            }
        }
        for side in [-3.0, 3.0] {
            let q = *p + r * side;
            if over(q) {
                let (x, _, z) = cell(q);
                w.set(x, 0, z, b::DARKPLANK);
                w.set(x, 1, z, b::FENCE);
                if i % 6 == 0 {
                    w.set(x, 2, z, b::FENCE);
                    w.set(x, 3, z, b::LANTERN);
                }
            }
        }
    }
}

/// The station in the woods: two platforms, lamps, a ticket hut, presents waiting to go
/// and a sled.
fn forest_station(w: &mut World) {
    w.fill(3, 6, 1, 1, -44, -30, b::DARKPLANK);
    w.fill(-6, -3, 1, 1, -44, -30, b::DARKPLANK);
    for z in [-42, -36] {
        for x in [5, -5] {
            lamp_post(w, x, 2, z, 3);
        }
    }
    house(w, 9, 15, -42, -35, 1, b::PLANK, 5);
    for (x, z) in [(5, -32), (6, -33), (6, -32), (-6, -40), (-6, -39), (-5, -41)] {
        w.set(x, 2, z, if (x + z) % 2 == 0 { b::PRESENT_RED } else { b::PRESENT_GREEN });
    }
    snowman(w, -5, 2, -33, 0);
    w.fill(7, 8, 1, 1, -46, -46, b::GOLD);
    w.fill(7, 8, 2, 2, -46, -45, b::WALL_RED);
}

// ====================================================================== the village

fn village(w: &mut World, rng: &mut Rng, path: &[Vec3]) {
    // Candy-cane arch across the ride at the edge of the village, with a garland, lights
    // and a star.
    w.fill(-8, -8, 1, 7, 25, 26, b::CANDY_CANE);
    w.fill(-8, -8, 1, 7, 36, 37, b::CANDY_CANE);
    w.fill(-8, -8, 8, 8, 25, 37, b::PINE);
    w.fill(-8, -8, 9, 9, 25, 37, b::PINE_SNOW);
    w.fill(-9, -9, 7, 7, 27, 35, b::LIGHTS);
    w.set(-8, 10, 31, b::XMAS_STAR);
    w.set(-9, 5, 26, b::WREATH);
    w.set(-9, 5, 36, b::WREATH);
    // Houses down both sides of the street, doors facing it, and more behind them.
    house(w, -2, 5, 16, 23, 4, b::PLANK, 5);
    house(w, 9, 16, 17, 24, 4, b::BRICK, 6);
    house(w, 19, 25, 16, 23, 4, b::GINGERBREAD, 5);
    for x in 18..=26 {
        if w.is_air(x, 6, 15) {
            w.set(x, 6, 15, b::FROSTING);
        }
    }
    house(w, -11, -4, 45, 52, 5, b::BRICK, 6);
    house(w, 0, 7, 46, 53, 5, b::PLANK, 5);
    house(w, 11, 18, 46, 53, 5, b::GINGERBREAD, 5);
    house(w, -9, -2, 57, 64, 5, b::PLANK, 9);
    house(w, 4, 11, 58, 65, 5, b::BRICK, 9);
    house(w, 15, 22, 57, 64, 5, b::PLANK, 6);
    // Fairy lights strung across the street.
    for x in [2, 14] {
        for z in 24..=45 {
            if w.is_air(x, 6, z) {
                w.set(x, 6, z, b::LIGHTS);
            }
        }
    }
    // Lamps along the street, and snowmen in the front gardens.
    for (i, p) in path.iter().enumerate() {
        if !(-4.0..37.0).contains(&p.x) || !(28.0..50.0).contains(&p.z) || p.y > 2.0 || i % 9 != 0 {
            continue;
        }
        let (_, r) = along(path, i);
        for side in [-3.6, 3.6] {
            let (x, _, z) = cell(*p + r * side);
            if w.is_air(x, 1, z) && w.is_air(x, 4, z) {
                lamp_post(w, x, 1, z, 3);
            }
        }
    }
    for &(x, z, face) in &[(2, 26, 4), (12, 27, 4), (-6, 41, 5), (9, 42, 5), (22, 28, 4)] {
        if w.is_air(x, 1, z) && !near_track(path, x as f32 + 0.5, z as f32 + 0.5, 3.0) {
            snowman(w, x, 1, z, face);
        }
    }
    // The town square: the village's own great tree, ringed with lamps and presents.
    pine_tree(w, rng, 34, 1, 30, 16, 0.4, true);
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::TAU / 8.0;
        let (x, z) = ((34.0 + a.cos() * 7.5).round() as i32, (30.0 + a.sin() * 7.5).round() as i32);
        if w.is_air(x, 1, z) && !near_track(path, x as f32 + 0.5, z as f32 + 0.5, 3.0) {
            lamp_post(w, x, 1, z, 3);
        }
        let (x, z) = ((34.0 + a.cos() * 5.0).round() as i32, (30.0 + a.sin() * 5.0).round() as i32);
        if w.is_air(x, 1, z) {
            w.set(x, 1, z, if k % 2 == 0 { b::PRESENT_RED } else { b::PRESENT_GREEN });
        }
    }
    // A sleigh full of presents, parked in the square.
    w.fill(40, 43, 1, 1, 22, 23, b::GOLD);
    w.fill(40, 43, 2, 3, 22, 23, b::WALL_RED);
    w.fill(43, 43, 4, 4, 22, 23, b::WALL_RED);
    w.set(41, 4, 22, b::PRESENT_GREEN);
    w.set(42, 4, 23, b::PRESENT_RED);
}

// ====================================================================== the rooftops

/// Two rows of tall townhouses whose roofs the ride runs over, with chimneys, lights and
/// Santa's sleigh parked on one of the roofs.
fn rooftops(w: &mut World, rng: &mut Rng) {
    let walls = [b::PLANK, b::BRICK, b::GINGERBREAD, b::BRICK, b::PLANK, b::BRICK];
    for (k, x0) in (-14..=26).step_by(8).enumerate() {
        let h = 6 + (k as i32 % 3);
        house(w, x0, x0 + 6, 70, 80, 4, walls[k % walls.len()], h);
        if x0 < 18 {
            house(w, x0, x0 + 6, 83, 89, 5, walls[(k + 2) % walls.len()], 5 + (k as i32 % 2));
        }
    }
    // The sleigh on a roof: red sides on gold runners, piled high with presents.
    let (sx, sy, sz) = (27, 13, 74);
    w.fill(sx, sx + 4, sy, sy, sz, sz + 1, b::GOLD);
    w.fill(sx, sx + 3, sy + 1, sy + 1, sz, sz + 1, b::WALL_RED);
    w.fill(sx + 3, sx + 3, sy + 2, sy + 2, sz, sz + 1, b::WALL_RED);
    for (dx, dz) in [(0, 0), (1, 1), (2, 0)] {
        let id = if rng.chance(0.5) { b::PRESENT_RED } else { b::PRESENT_GREEN };
        w.set(sx + dx, sy + 2, sz + dz, id);
    }
}

// ====================================================================== the factory

/// The toy factory: a big brick hall full of conveyor belts of presents, shelves of toys,
/// a wrapping machine, workbenches and hanging lanterns, under a flat snowy roof.
fn factory(w: &mut World, rng: &mut Rng) {
    let (x0, x1, z0, z1) = FACTORY;
    let top = FACTORY_TOP;
    w.room(x0, x1, 0, top, z0, z1, b::BRICK, b::PLANK, b::DARKPLANK);
    w.room(x0 + 1, x1 - 1, 0, top - 1, z0 + 1, z1 - 1, b::WALL_RED, b::PLANK, b::DARKPLANK);
    w.fill(x0, x1, top + 1, top + 1, z0, z1, b::SNOW_BLOCK);
    // Fairy lights round the roof and glowing windows down the sides.
    for z in z0..=z1 {
        for x in [x0 - 1, x1 + 1] {
            w.set(x, top, z, b::LIGHTS);
        }
        if z % 6 == 3 {
            for x in [x0, x1] {
                w.fill(x, x, 4, 6, z, z + 1, b::WINDOW_LIT);
                w.fill(x, x, 10, 12, z, z + 1, b::WINDOW_LIT);
            }
        }
    }
    for x in x0..=x1 {
        for z in [z0 - 1, z1 + 1] {
            w.set(x, top, z, b::LIGHTS);
        }
    }
    // A wreath over the way out to the lake.
    w.set(0, 11, z1 + 1, b::WREATH);

    // Conveyor belts of presents down the west wall and across the east end.
    w.fill(-40, -36, 1, 1, 104, 146, b::DARKPLANK);
    for z in (105..146).step_by(2) {
        w.set(-38, 2, z, if z % 4 == 1 { b::PRESENT_RED } else { b::PRESENT_GREEN });
    }
    w.fill(-6, 7, 1, 1, 104, 106, b::DARKPLANK);
    for x in (-5..7).step_by(3) {
        w.set(x, 2, 105, if x % 2 == 0 { b::PRESENT_RED } else { b::PRESENT_GREEN });
    }
    w.fill(3, 7, 1, 1, 112, 134, b::DARKPLANK);
    for z in (113..134).step_by(2) {
        w.set(5, 2, z, if z % 4 == 1 { b::PRESENT_GREEN } else { b::PRESENT_RED });
    }
    // Shelves of toys along the east wall and the south-west corner.
    for z in z0 + 2..z1 - 1 {
        if z % 8 != 0 {
            w.fill(x1 - 2, x1 - 2, 1, 8, z, z, b::TOYSHELF);
        }
    }
    w.fill(-40, -26, 1, 6, z0 + 2, z0 + 2, b::TOYSHELF);
    // The wrapping machine: an iron box with glowing windows, gold trim, lights and
    // candy-cane pipes up to the roof.
    w.fill(-8, -2, 1, 6, 121, 127, b::IRON);
    w.fill(-8, -2, 7, 7, 121, 127, b::GOLD);
    for z in [122, 124, 126] {
        w.fill(-1, -1, 3, 4, z, z, b::WINDOW_LIT);
    }
    for x in [-7, -3] {
        w.fill(x, x, 8, top - 1, 124, 124, b::CANDY_CANE);
    }
    for z in 121..=127 {
        w.set(-1, 7, z, b::LIGHTS);
    }
    // Workbenches with presents being wrapped.
    for &(x, z) in &[(-16, 130), (-6, 112), (-38, 132), (-30, 104), (-8, 136)] {
        w.fill(x, x + 2, 1, 1, z, z + 1, b::PLANK);
        w.set(x, 2, z, b::CANDLE);
        w.set(x + 1, 2, z + 1, b::PRESENT_RED);
        w.set(x + 2, 2, z, b::PRESENT_GREEN);
    }
    // Piles of presents.
    for _ in 0..40 {
        let (x, z) = (rng.int(x0 + 3, x1 - 3), rng.int(z0 + 3, z1 - 3));
        if w.is_air(x, 1, z) && w.get(x, 0, z) == b::PLANK {
            w.set(x, 1, z, if rng.chance(0.5) { b::PRESENT_RED } else { b::PRESENT_GREEN });
        }
    }
    // Lanterns hanging from the roof, and strings of lights across it.
    for z in (106..148).step_by(10) {
        for x in (-36..8).step_by(11) {
            w.fill(x, x, 12, top - 1, z, z, b::FENCE);
            w.set(x, 11, z, b::LANTERN);
        }
    }
    for z in (110..148).step_by(10) {
        for x in x0 + 2..x1 - 1 {
            if w.is_air(x, 13, z) {
                w.set(x, 13, z, b::LIGHTS);
            }
        }
    }
}

/// The factory's giant chimney, which the ride dives down into from the rooftops: a
/// brick stack open at the top, against the factory's south wall.
fn chimney(w: &mut World) {
    let (x0, x1, z0, z1, top) = (-18, -6, 91, 104, 15);
    for y in 1..=top {
        for z in z0..=z1 {
            for x in x0..=x1 {
                let wall = x == x0 || x == x1 || z == z0 || z == z1;
                w.set(x, y, z, if wall { b::BRICK } else { b::AIR });
            }
        }
    }
    for z in z0..=z1 {
        for x in x0..=x1 {
            if x == x0 || x == x1 || z == z0 || z == z1 {
                w.set(x, top + 1, z, b::SNOW_BLOCK);
            }
        }
    }
    w.fill(x0 + 1, x1 - 1, 0, 0, z0 + 1, z1 - 1, b::BRICK);
}

// ====================================================================== the lake

/// The frozen lake (solid ice deep down, for the crack the ride plunges through), with
/// an island and its great tree, skating snowmen, an ice-fishing hut and lanterns.
fn lake(w: &mut World, rng: &mut Rng, path: &[Vec3]) {
    let (x0, x1, z0, z1) = LAKE;
    let (cx, cz) = ((x0 + x1) as f32 * 0.5, (z0 + z1) as f32 * 0.5);
    for z in z0..=z1 {
        for x in x0..=x1 {
            // Rounded corners.
            let (dx, dz) = ((x as f32 - cx) / ((x1 - x0) as f32 * 0.5), (z as f32 - cz) / ((z1 - z0) as f32 * 0.5));
            if dx.powi(4) + dz.powi(4) > 1.0 {
                continue;
            }
            for y in -30..=0 {
                w.set(x, y, z, b::ICE);
            }
        }
    }
    // The island the ride curves round.
    let (ix, iz): (i32, i32) = (6, 176);
    for z in iz - 7..=iz + 7 {
        for x in ix - 7..=ix + 7 {
            if (x - ix).pow(2) + (z - iz).pow(2) <= 49 {
                w.set(x, 0, z, b::SNOW);
            }
        }
    }
    pine_tree(w, rng, ix, 1, iz, 16, 0.45, true);
    for k in 0..6 {
        let a = k as f32 * std::f32::consts::TAU / 6.0 + 0.4;
        let (x, z) = ((ix as f32 + a.cos() * 5.5).round() as i32, (iz as f32 + a.sin() * 5.5).round() as i32);
        if w.is_air(x, 1, z) {
            w.set(x, 1, z, if k % 2 == 0 { b::PRESENT_RED } else { b::PRESENT_GREEN });
        }
    }
    // Snowmen out skating, all over the ice, turned toward the ride.
    let mut n = 0;
    for _ in 0..200 {
        if n >= 14 {
            break;
        }
        let (x, z) = (rng.int(x0 + 4, x1 - 4), rng.int(z0 + 3, z1 - 3));
        let p = Vec3::new(x as f32 + 0.5, 1.0, z as f32 + 0.5);
        if w.get(x, 0, z) == b::ICE && w.is_air(x, 1, z) && !near_track(path, p.x, p.z, 4.0) {
            let near = path.iter().copied().min_by(|a, b| a.distance_squared(p).total_cmp(&b.distance_squared(p))).unwrap_or(Vec3::ZERO);
            snowman(w, x, 1, z, facing_toward(p.x, p.z, near.x, near.z));
            n += 1;
        }
    }
    // An ice-fishing hut, and lanterns on poles here and there.
    house(w, 33, 38, 156, 161, 1, b::PLANK, 4);
    for _ in 0..16 {
        let (x, z) = (rng.int(x0 + 4, x1 - 4), rng.int(z0 + 2, z1 - 2));
        if w.get(x, 0, z) == b::ICE && w.is_air(x, 1, z) && !near_track(path, x as f32 + 0.5, z as f32 + 0.5, 3.5) {
            lamp_post(w, x, 1, z, 3);
        }
    }
}

// ====================================================================== underground

/// The crystal cavern under the lake: walls of ice glittering with crystals, icicles,
/// columns of ice, frosty torches and snowdrifts.
fn cavern(w: &mut World, rng: &mut Rng) {
    let (x0, x1, y0, y1, z0, z1) = CAVERN;
    w.room(x0, x1, y0, y1, z0, z1, b::ICE, b::SNOW_BLOCK, b::ICE);
    for _ in 0..70 {
        let (x, y, z) = match rng.int(0, 3) {
            0 => (x0, rng.int(y0 + 1, y1 - 1), rng.int(z0 + 1, z1 - 1)),
            1 => (x1, rng.int(y0 + 1, y1 - 1), rng.int(z0 + 1, z1 - 1)),
            2 => (rng.int(x0 + 1, x1 - 1), y1, rng.int(z0 + 1, z1 - 1)),
            _ => (rng.int(x0 + 1, x1 - 1), rng.int(y0 + 1, y1 - 1), z1),
        };
        w.set(x, y, z, b::ICE_CRYSTAL);
        let (dx, dy, dz) = if x == x0 || x == x1 {
            (if x == x0 { 1 } else { -1 }, 0, 0)
        } else if y == y1 {
            (0, -1, 0)
        } else {
            (0, 0, -1)
        };
        if rng.chance(0.5) && w.is_air(x + dx, y + dy, z + dz) {
            w.set(x + dx, y + dy, z + dz, b::ICE_CRYSTAL);
        }
    }
    for _ in 0..110 {
        let (x, z) = (rng.int(x0 + 1, x1 - 1), rng.int(z0 + 1, z1 - 1));
        if w.is_air(x, y1 - 1, z) {
            w.set(x, y1 - 1, z, b::ICICLE);
        }
    }
    // Columns of ice from floor to ceiling, with frosty torches.
    for &(x, z) in &[(-19, 203), (13, 204), (-18, 216), (11, 213), (-15, 226), (14, 225)] {
        w.fill(x, x + 1, y0 + 1, y1 - 1, z, z + 1, b::ICE_BRICK);
        w.set(x + 2, y0 + 5, z, b::FROST_TORCH);
        w.set(x - 1, y0 + 5, z + 1, b::FROST_TORCH);
    }
    for _ in 0..60 {
        let (x, z) = (rng.int(x0 + 1, x1 - 1), rng.int(z0 + 1, z1 - 1));
        if w.is_air(x, y0 + 1, z) {
            let k = rng.f();
            w.set(x, y0 + 1, z, if k < 0.7 { b::SNOW_BLOCK } else if k < 0.85 { b::PRESENT_RED } else { b::PRESENT_GREEN });
        }
    }
}

/// The Snow King's throne room: a hall of ice bricks with a checked floor, a red carpet
/// up to his doors with snowman guards standing to attention, crystal chandeliers,
/// lit-up trees in the corners, and garlands and lights on the walls.
fn throne_room(w: &mut World, rng: &mut Rng) {
    let (x0, x1, y0, y1, z0, z1) = THRONE;
    w.room(x0, x1, y0, y1, z0, z1, b::ICE_BRICK, b::SNOW_BLOCK, b::ICE_BRICK);
    for z in z0 + 1..z1 {
        for x in x0 + 1..x1 {
            if (x + z).rem_euclid(2) == 0 {
                w.set(x, y0, z, b::ICE);
            }
        }
    }
    for z in z0..z1 {
        for x in -3..=2 {
            w.set(x, y0, z, if x == -3 || x == 2 { b::GOLD } else { b::CARPET });
        }
    }
    // Guards along the carpet (not where the king rises up), facing it.
    for z in (z0 + 3..z1 - 2).step_by(4) {
        if (244..=262).contains(&z) {
            continue;
        }
        snowman(w, -6, y0 + 1, z, 0);
        snowman(w, 5, y0 + 1, z, 1);
    }
    for z in (z0 + 5..z1 - 4).step_by(5) {
        for x in [-21, -14, 13, 20] {
            snowman(w, x, y0 + 1, z, if x < 0 { 0 } else { 1 });
        }
    }
    // Crystal chandeliers hanging on chains.
    for &(x, z) in &[(-12, 238), (12, 238), (-12, 256), (12, 256)] {
        w.fill(x, x, y1 - 4, y1 - 1, z, z, b::FENCE);
        for (dx, dz) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
            w.set(x + dx, y1 - 5, z + dz, b::ICE_CRYSTAL);
        }
        w.set(x, y1 - 6, z, b::ICE_CRYSTAL);
    }
    for &(x, z) in &[(-22, 234), (22, 234), (-22, 263), (22, 263)] {
        pine_tree(w, rng, x, y0 + 1, z, 11, 0.3, true);
    }
    banners(w, x0 + 1, (y0 + 12, y0 + 13), (z0 + 2, z1 - 2), 6, b::GARLAND);
    banners(w, x1 - 1, (y0 + 12, y0 + 13), (z0 + 2, z1 - 2), 6, b::GARLAND);
    for z in z0 + 1..z1 {
        for x in [x0 + 1, x1 - 1] {
            if w.is_air(x, y0 + 15, z) {
                w.set(x, y0 + 15, z, b::LIGHTS);
            }
        }
    }
}

/// The grand doors behind the Snow King (the leaves themselves are animated models), with
/// candy-cane pillars and braziers, and the long hall beyond them.
fn throne_doors(w: &mut World) {
    let doors = Level::Christmas.doors();
    let (fl, dz) = (doors.floor as i32, doors.z.floor() as i32);
    w.room(-7, 7, fl - 1, fl + 12, dz, dz + 72, b::ICE_BRICK, b::SNOW_BLOCK, b::ICE_BRICK);
    for z in (dz + 4..dz + 70).step_by(5) {
        for x in [-6, 6] {
            w.set(x, fl + 6, z, b::LANTERN);
            w.set(x, fl + 3, z + 2, b::STOCKING);
            w.set(x, fl + 8, z + 2, b::WREATH);
        }
    }
    for z in dz..dz + 72 {
        for x in -3..=2 {
            w.set(x, fl - 1, z, if x == -3 || x == 2 { b::GOLD } else { b::CARPET });
        }
    }
    for y in fl..fl + 16 {
        for x in -8..=7 {
            let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
            if doors.contains(cx, cy, 0.0) {
                w.set(x, y, dz, b::AIR);
            } else if doors.contains(cx, cy, 1.6) {
                w.set(x, y, dz, b::GOLD_GLOW);
                w.set(x, y, dz - 1, b::GOLD_GLOW);
            }
        }
    }
    for px in [-10, 9] {
        w.fill(px, px + 1, fl, fl + 15, dz - 2, dz - 1, b::CANDY_CANE);
        w.set(px, fl + 7, dz - 3, b::LANTERN);
        w.set(px + 1, fl + 7, dz - 3, b::LANTERN);
        let bx = if px < 0 { -13 } else { 12 };
        w.fill(bx, bx, fl, fl + 1, dz - 3, dz - 3, b::GOLD);
        w.set(bx, fl + 2, dz - 3, b::FIRE);
    }
}

// ====================================================================== the woods

/// Snowy pines everywhere there's open snow (a few lit up), snowdrifts, and candy canes
/// and lanterns along the ride through the forest.
fn woods(w: &mut World, rng: &mut Rng, path: &[Vec3]) {
    let ([x0, _, z0], [x1, _, z1]) = (w.lo, w.hi);
    for _ in 0..2600 {
        let (x, z) = (rng.int(x0 + 3, x1 - 4), rng.int(z0 + 3, z1 - 4));
        let h = rng.int(6, 13);
        let r = pine_radius(h).ceil() as i32 - 1;
        if open_snow(w, path, x, z, r, 8.0) {
            let lights = if rng.chance(0.15) { 0.15 } else { 0.0 };
            pine_tree(w, rng, x, 1, z, h, lights, false);
        }
    }
    for _ in 0..500 {
        let (x, z) = (rng.int(x0 + 1, x1 - 2), rng.int(z0 + 1, z1 - 2));
        if open_snow(w, path, x, z, 0, 14.0) {
            w.set(x, 1, z, b::SNOW_BLOCK);
            if rng.chance(0.3) && w.is_air(x + 1, 1, z) {
                w.set(x + 1, 1, z, b::SNOW_BLOCK);
            }
        }
    }
    // Candy canes and lanterns lining the ride through the forest.
    for (i, p) in path.iter().enumerate() {
        if p.z > 24.0 || i % 7 != 3 {
            continue;
        }
        let (_, r) = along(path, i);
        let side = if (i / 7) % 2 == 0 { 3.4 } else { -3.4 };
        let (x, _, z) = cell(*p + r * side);
        if w.get(x, 0, z) == b::SNOW && w.is_air(x, 1, z) && w.is_air(x, 2, z) {
            if (i / 7) % 3 == 0 {
                lamp_post(w, x, 1, z, 3);
            } else {
                w.fill(x, x, 1, 2, z, z, b::CANDY_CANE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ride_through_the_christmas_world_is_clear() {
        let track = Track::new(Level::Christmas);
        let w = generate(&track, Level::Christmas);
        for f in track.samples(0.5) {
            for h in [0.3f32, 1.0, 1.65, 2.2] {
                let p = f.pos + Vec3::Y * h;
                assert!(!w.solid_at(p), "something solid in the ride's way at {p}");
            }
        }
    }

    #[test]
    fn the_ride_dives_down_the_factory_chimney() {
        let track = Track::new(Level::Christmas);
        let w = generate(&track, Level::Christmas);
        // Its walls stand either side of the rails on the way down.
        let mid = track.samples(0.5).into_iter().find(|f| f.pos.z > 97.0 && f.pos.y < 11.0).unwrap();
        let (x, y, z) = cell(mid.pos);
        assert_eq!(w.get(-18, y, z), b::BRICK, "no west wall by ({x}, {y}, {z})");
        assert_eq!(w.get(-6, y, z), b::BRICK, "no east wall by ({x}, {y}, {z})");
    }

    #[test]
    fn the_christmas_track_is_its_own() {
        let a = Track::new(Level::Halloween).samples(2.0);
        let b = Track::new(Level::Christmas).samples(2.0);
        // Hardly any of the Christmas ride runs where the Halloween one does.
        let shared = b.iter().filter(|f| a.iter().any(|g| g.pos.distance(f.pos) < 3.0)).count();
        assert!(shared * 10 < b.len(), "{shared} of {} Christmas track samples overlap Halloween's", b.len());
    }
}
