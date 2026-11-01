//! The roller-coaster track: a centripetal Catmull-Rom spline through hand-placed
//! control points with a speed profile, normalised so the ride from the station
//! to the boss stop takes exactly `RIDE_SECONDS`. Each level has its own track.

use crate::level::Level;
use crate::world::Zone;
use glam::Vec3;

pub const RIDE_SECONDS: f32 = 120.0;

/// (position of rail surface, relative speed)
const POINTS: &[([f32; 3], f32)] = &[
    ([0.0, 1.0, -22.0], 0.45), // 0 station
    ([0.0, 1.0, -12.0], 0.7),  // 1 cemetery gate
    ([-5.0, 1.0, 0.0], 0.95),  // 2 graveyard
    ([-9.0, 1.0, 10.0], 1.0),
    ([-2.0, 1.0, 19.0], 1.0),
    ([7.0, 1.0, 27.0], 1.0),
    ([5.0, 1.0, 34.0], 1.0),
    ([0.0, 1.0, 40.0], 1.0),   // 7 front door
    ([-5.0, 1.5, 47.0], 0.9),  // 8 foyer
    ([-7.0, 4.0, 55.0], 0.75), // 9 lift
    ([-2.0, 6.5, 62.0], 0.75),
    ([4.0, 7.0, 67.0], 0.85),
    ([8.0, 7.0, 74.0], 0.9),   // 12 library
    ([7.0, 6.5, 83.0], 0.9),
    ([0.0, 6.0, 89.0], 0.9),
    ([-8.0, 5.0, 95.0], 0.95),
    ([-8.0, 3.5, 102.0], 1.0), // 16 dining hall
    ([-2.0, 2.0, 108.0], 1.0),
    ([4.0, 1.0, 115.0], 1.0),
    ([6.0, 1.0, 121.0], 0.9),
    ([1.0, 1.0, 127.0], 1.1),
    ([0.0, -3.0, 132.0], 2.0), // 21 the big drop!
    ([0.0, -11.0, 137.0], 2.6),
    ([0.0, -17.0, 143.0], 2.0),
    ([-7.0, -17.0, 151.0], 1.2), // 24 crypt
    ([-2.0, -17.0, 159.0], 1.0),
    ([7.0, -17.0, 165.0], 1.0),
    ([3.0, -17.0, 172.0], 0.9),
    ([0.0, -17.0, 178.0], 0.6), // 28 lair
    ([0.0, -17.0, 184.0], 0.25), // 29 STOP: boss fight
    ([0.0, -17.0, 194.0], 0.8),
    ([0.0, -17.0, 204.0], 1.0),
    ([0.0, -17.0, 212.0], 1.0), // through the boss doors...
    ([0.0, -17.0, 222.0], 1.0),
    ([0.0, -17.0, 232.0], 1.0),
    ([0.0, -17.0, 296.0], 1.0), // ...and out down the endless exit hall
];
const STOP_INDEX: usize = 29;

/// Holly Hollow: (position of rail surface, relative speed).
const XMAS_POINTS: &[([f32; 3], f32)] = &[
    ([0.0, 1.0, -36.0], 0.45), // 0 station in the forest
    ([0.0, 1.0, -26.0], 0.7),
    ([-8.0, 1.0, -14.0], 0.95),
    ([-20.0, 1.0, -4.0], 1.0),
    ([-26.0, 1.6, 10.0], 1.0), // 4 over the bridge across the frozen creek
    ([-20.0, 1.0, 24.0], 1.0),
    ([-6.0, 1.0, 32.0], 1.0), // 6 through the village gate and down the street
    ([10.0, 1.0, 34.0], 1.0),
    ([26.0, 1.0, 38.0], 1.0),
    ([38.0, 1.0, 48.0], 0.9),
    ([42.0, 4.0, 60.0], 0.8), // 10 the lift up onto the rooftops
    ([38.0, 10.0, 70.0], 0.8),
    ([30.0, 15.0, 77.0], 0.85),
    ([18.0, 17.0, 81.0], 1.0),
    ([6.0, 17.0, 84.0], 1.0),
    ([-4.0, 17.0, 90.0], 0.95),
    ([-10.0, 15.0, 95.0], 1.0), // 16 down the factory chimney!
    ([-12.0, 8.0, 99.0], 2.0),
    ([-13.0, 1.0, 105.0], 2.2),
    ([-20.0, 1.5, 116.0], 1.0), // 19 the toy factory
    ([-30.0, 4.0, 126.0], 0.8),
    ([-26.0, 7.0, 137.0], 0.75),
    ([-12.0, 7.0, 141.0], 0.8),
    ([-2.0, 3.0, 146.0], 0.9),
    ([4.0, 1.5, 158.0], 1.0), // 24 out across the frozen lake
    ([18.0, 1.5, 163.0], 0.95),
    ([29.0, 1.5, 174.0], 0.95),
    ([24.0, 1.5, 187.0], 1.0),
    ([16.0, -3.0, 192.0], 1.6), // 28 down through a crack in the ice
    ([9.0, -12.0, 196.0], 2.4),
    ([3.0, -21.0, 200.0], 2.0),
    ([-6.0, -24.0, 208.0], 1.2), // 31 the crystal cavern
    ([-3.0, -24.0, 218.0], 1.0),
    ([0.0, -24.0, 226.0], 0.8),
    ([0.0, -24.0, 232.0], 0.6), // 34 the Snow King's throne room
    ([0.0, -24.0, 238.0], 0.25), // 35 STOP: boss fight
    ([0.0, -24.0, 248.0], 0.8),
    ([0.0, -24.0, 258.0], 1.0),
    ([0.0, -24.0, 266.0], 1.0), // through the doors...
    ([0.0, -24.0, 276.0], 1.0),
    ([0.0, -24.0, 286.0], 1.0),
    ([0.0, -24.0, 350.0], 1.0), // ...and out down the long hall
];
const XMAS_STOP_INDEX: usize = 35;
/// The control point where each of Holly Hollow's sections (`Zone`s) starts.
const XMAS_SECTIONS: &[usize] = &[0, 6, 10, 16, 19, 24, 28, 31, 34];

const SUBDIV: usize = 48;

pub struct Track {
    pts: Vec<Vec3>,
    dist: Vec<f32>,
    times: Vec<f32>,
    pub stop_dist: f32,
    pub total: f32,
    /// Distance along the track where each section starts (empty when the level's
    /// sections aren't laid out along its track, see `zone_at_dist`).
    sections: Vec<f32>,
}

#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub pos: Vec3,
    pub fwd: Vec3,
    pub right: Vec3,
    pub up: Vec3,
}

fn cr_point(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    // Centripetal Catmull-Rom (Barry-Goldman), avoids loops/cusps.
    let a = 0.5;
    let t0 = 0.0f32;
    let t1 = t0 + p0.distance(p1).max(1e-3).powf(a);
    let t2 = t1 + p1.distance(p2).max(1e-3).powf(a);
    let t3 = t2 + p2.distance(p3).max(1e-3).powf(a);
    let t = t1 + (t2 - t1) * t;
    let a1 = p0 * ((t1 - t) / (t1 - t0)) + p1 * ((t - t0) / (t1 - t0));
    let a2 = p1 * ((t2 - t) / (t2 - t1)) + p2 * ((t - t1) / (t2 - t1));
    let a3 = p2 * ((t3 - t) / (t3 - t2)) + p3 * ((t - t2) / (t3 - t2));
    let b1 = a1 * ((t2 - t) / (t2 - t0)) + a2 * ((t - t0) / (t2 - t0));
    let b2 = a2 * ((t3 - t) / (t3 - t1)) + a3 * ((t - t1) / (t3 - t1));
    b1 * ((t2 - t) / (t2 - t1)) + b2 * ((t - t1) / (t2 - t1))
}

impl Track {
    pub fn new(level: Level) -> Track {
        let (points, stop, sections) = match level {
            Level::Halloween => (POINTS, STOP_INDEX, &[][..]),
            Level::Christmas => (XMAS_POINTS, XMAS_STOP_INDEX, XMAS_SECTIONS),
        };
        Self::build(points, stop, sections)
    }

    fn build(points: &[([f32; 3], f32)], stop: usize, sections: &[usize]) -> Track {
        let cp: Vec<Vec3> = points.iter().map(|(p, _)| Vec3::from(*p)).collect();
        let n = cp.len();
        let get = |i: isize| -> Vec3 {
            if i < 0 {
                cp[0] * 2.0 - cp[1]
            } else if i as usize >= n {
                cp[n - 1] * 2.0 - cp[n - 2]
            } else {
                cp[i as usize]
            }
        };
        let mut pts = vec![];
        let mut speeds = vec![];
        for i in 0..n - 1 {
            for s in 0..SUBDIV {
                let t = s as f32 / SUBDIV as f32;
                pts.push(cr_point(get(i as isize - 1), get(i as isize), get(i as isize + 1), get(i as isize + 2), t));
                // Smoothstep between control-point speeds.
                let e = t * t * (3.0 - 2.0 * t);
                speeds.push(points[i].1 + (points[i + 1].1 - points[i].1) * e);
            }
        }
        pts.push(cp[n - 1]);
        speeds.push(points[n - 1].1);

        let mut dist = vec![0.0];
        let mut raw_t = vec![0.0];
        for i in 1..pts.len() {
            let ds = pts[i].distance(pts[i - 1]);
            dist.push(dist[i - 1] + ds);
            let v = 0.5 * (speeds[i] + speeds[i - 1]);
            raw_t.push(raw_t[i - 1] + ds / v.max(0.05));
        }
        let stop_i = stop * SUBDIV;
        let k = raw_t[stop_i] / RIDE_SECONDS;
        let times = raw_t.iter().map(|t| t / k).collect();
        let sections = sections.iter().map(|&i| dist[i * SUBDIV]).collect();
        Track { stop_dist: dist[stop_i], total: *dist.last().unwrap(), pts, dist, times, sections }
    }

    /// Ride time when the cart is `d` along the track.
    pub fn time_at_dist(&self, d: f32) -> f32 {
        let i = self.dist.partition_point(|&x| x < d).min(self.dist.len() - 1);
        self.times[i]
    }

    /// Ride time when the cart enters section `z`.
    pub fn section_time(&self, z: Zone) -> f32 {
        self.sections.get(z.0 as usize).map_or(RIDE_SECONDS, |&d| self.time_at_dist(d))
    }

    /// The section the cart is in, `d` along the track.
    pub fn zone_at_dist(&self, d: f32) -> Zone {
        Zone(self.sections.iter().take_while(|&&s| s <= d).count().saturating_sub(1) as u8)
    }

    /// The section of the nearest bit of track to a point.
    pub fn zone_near(&self, p: Vec3) -> Zone {
        let mut best = (f32::MAX, 0.0);
        for i in (0..self.pts.len()).step_by(4) {
            let d2 = self.pts[i].distance_squared(p);
            if d2 < best.0 {
                best = (d2, self.dist[i]);
            }
        }
        self.zone_at_dist(best.1)
    }

    pub fn pos_at(&self, d: f32) -> Vec3 {
        let d = d.clamp(0.0, self.total);
        let i = match self.dist.binary_search_by(|x| x.partial_cmp(&d).unwrap()) {
            Ok(i) => i,
            Err(i) => i,
        }
        .clamp(1, self.pts.len() - 1);
        let (d0, d1) = (self.dist[i - 1], self.dist[i]);
        let t = if d1 > d0 { (d - d0) / (d1 - d0) } else { 0.0 };
        self.pts[i - 1].lerp(self.pts[i], t)
    }

    /// Distance along the track at a given ride time.
    pub fn dist_at_time(&self, t: f32) -> f32 {
        if t <= 0.0 {
            return 0.0;
        }
        let i = match self.times.binary_search_by(|x| x.partial_cmp(&t).unwrap()) {
            Ok(i) => i,
            Err(i) => i,
        };
        if i >= self.times.len() {
            return self.total;
        }
        let i = i.max(1);
        let (t0, t1) = (self.times[i - 1], self.times[i]);
        let f = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
        self.dist[i - 1] + (self.dist[i] - self.dist[i - 1]) * f
    }

    /// Ride time when the track first crosses the given z.
    pub fn time_at_z(&self, z: f32) -> f32 {
        for i in 0..self.pts.len() {
            if self.pts[i].z >= z {
                return self.times[i];
            }
        }
        *self.times.last().unwrap()
    }

    pub fn frame_at(&self, d: f32) -> Frame {
        let pos = self.pos_at(d);
        let ahead = self.pos_at(d + 0.6);
        let behind = self.pos_at(d - 0.6);
        let mut fwd = (ahead - behind).normalize_or_zero();
        if fwd == Vec3::ZERO {
            fwd = Vec3::Z;
        }
        let right = fwd.cross(Vec3::Y).normalize_or_zero();
        let right = if right == Vec3::ZERO { Vec3::X } else { right };
        let up = right.cross(fwd).normalize();
        Frame { pos, fwd, right, up }
    }

    /// Signed horizontal curvature (rad per metre), used for banking.
    pub fn turn_rate(&self, d: f32) -> f32 {
        let a = self.frame_at(d - 2.0).fwd;
        let b = self.frame_at(d + 2.0).fwd;
        let ha = a.z.atan2(a.x);
        let hb = b.z.atan2(b.x);
        let mut dh = hb - ha;
        while dh > std::f32::consts::PI {
            dh -= std::f32::consts::TAU;
        }
        while dh < -std::f32::consts::PI {
            dh += std::f32::consts::TAU;
        }
        dh / 4.0
    }

    /// Dense samples for world building (position every ~step metres).
    pub fn samples(&self, step: f32) -> Vec<Frame> {
        let mut out = vec![];
        let mut d = 0.0;
        while d <= self.total {
            out.push(self.frame_at(d));
            d += step;
        }
        out
    }
}
