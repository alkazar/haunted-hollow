//! Procedurally painted 16x16 "Minecraft style" block textures plus an 8x8 pixel font,
//! packed into one 256x256 RGBA atlas. No image files needed.

pub const SIZE: u32 = 256;

// Tile ids (16x16 px each, 16 per row, in the top half of the atlas).
pub const T_WHITE: u8 = 0;
pub const T_GRASS_TOP: u8 = 1;
pub const T_GRASS_SIDE: u8 = 2;
pub const T_DIRT: u8 = 3;
pub const T_STONE: u8 = 4;
pub const T_COBBLE: u8 = 5;
pub const T_STONEBRICK: u8 = 6;
pub const T_PLANK: u8 = 7;
pub const T_DARKPLANK: u8 = 8;
pub const T_LOG: u8 = 9;
pub const T_LOG_TOP: u8 = 10;
pub const T_WALL_PURPLE: u8 = 11;
pub const T_WALL_GREEN: u8 = 12;
pub const T_CARPET: u8 = 13;
pub const T_BOOKSHELF: u8 = 14;
pub const T_PUMPKIN_SIDE: u8 = 15;
pub const T_PUMPKIN_FACE: u8 = 16;
pub const T_PUMPKIN_TOP: u8 = 17;
pub const T_COBWEB: u8 = 18;
pub const T_WINDOW: u8 = 19;
pub const T_GRAVE: u8 = 20;
pub const T_METAL: u8 = 21;
pub const T_BONE: u8 = 22;
pub const T_RING: u8 = 23;
pub const T_LANTERN: u8 = 24;
pub const T_MOSSY: u8 = 25;
pub const T_CRYPT: u8 = 26;
pub const T_PURPLE: u8 = 27;
pub const T_ROOF: u8 = 28;
pub const T_CLOTH: u8 = 29;
pub const T_PORTRAIT: u8 = 30;
pub const T_WINDOW_LIT: u8 = 31;
pub const T_AMETHYST: u8 = 32;
pub const T_FIRE: u8 = 33;
pub const T_STAR: u8 = 34;
pub const T_MOON: u8 = 35;
pub const T_HAY: u8 = 36;
pub const T_PORTRAIT2: u8 = 37;
pub const T_DOT: u8 = 38;
pub const T_TALLGRASS: u8 = 39;
pub const T_DEADBUSH: u8 = 40;
pub const T_MUSHROOM: u8 = 41;
pub const T_CLOCK: u8 = 42;
pub const T_CANDY: u8 = 43;
pub const T_IRON: u8 = 44;
pub const T_CAKE_SIDE: u8 = 45;
pub const T_CAKE_TOP: u8 = 46;
pub const T_BANNER: u8 = 47;
pub const T_CHAIN: u8 = 48;
pub const T_SKULLS: u8 = 49;
pub const T_POT: u8 = 50;
pub const T_SLIME: u8 = 51;
pub const T_CANDY2: u8 = 52;
pub const T_PAINTING3: u8 = 53;
pub const T_SHIELD: u8 = 54;
pub const T_GOLD: u8 = 55;
pub const T_MIRROR: u8 = 56;

/// UV rectangle (u0, v0, u1, v1) for a tile.
pub fn tile_uv(t: u8) -> [f32; 4] {
    let x = (t as u32 % 16) * 16;
    let y = (t as u32 / 16) * 16;
    let s = SIZE as f32;
    let e = 0.02;
    [(x as f32 + e) / s, (y as f32 + e) / s, (x as f32 + 16.0 - e) / s, (y as f32 + 16.0 - e) / s]
}

/// UV rectangle for an 8x8 font glyph.
pub fn glyph_uv(c: u8) -> [f32; 4] {
    let x = (c as u32 % 32) * 8;
    let y = 128 + (c as u32 / 32) * 8;
    let s = SIZE as f32;
    [x as f32 / s, y as f32 / s, (x + 8) as f32 / s, (y + 8) as f32 / s]
}

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263) ^ seed.wrapping_mul(2246822519);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h ^= h >> 16;
    (h & 0xffff) as f32 / 65535.0
}

struct Painter {
    px: Vec<u8>,
}

impl Painter {
    fn set(&mut self, tile: u8, x: i32, y: i32, c: [f32; 3], a: f32) {
        let tx = (tile as i32 % 16) * 16 + x;
        let ty = (tile as i32 / 16) * 16 + y;
        let i = ((ty as u32 * SIZE + tx as u32) * 4) as usize;
        self.px[i] = (c[0].clamp(0.0, 1.0) * 255.0) as u8;
        self.px[i + 1] = (c[1].clamp(0.0, 1.0) * 255.0) as u8;
        self.px[i + 2] = (c[2].clamp(0.0, 1.0) * 255.0) as u8;
        self.px[i + 3] = (a.clamp(0.0, 1.0) * 255.0) as u8;
    }
    /// Fill a tile with a per-pixel colour function.
    fn tile<F: Fn(i32, i32) -> ([f32; 3], f32)>(&mut self, t: u8, f: F) {
        for y in 0..16 {
            for x in 0..16 {
                let (c, a) = f(x, y);
                self.set(t, x, y, c, a);
            }
        }
    }
}

fn mul(c: [f32; 3], k: f32) -> [f32; 3] {
    [c[0] * k, c[1] * k, c[2] * k]
}
fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}
pub fn hex(h: u32) -> [f32; 3] {
    [((h >> 16) & 255) as f32 / 255.0, ((h >> 8) & 255) as f32 / 255.0, (h & 255) as f32 / 255.0]
}

fn noisy(base: [f32; 3], amt: f32, seed: u32) -> impl Fn(i32, i32) -> ([f32; 3], f32) {
    move |x, y| (mul(base, 1.0 - amt + hash(x, y, seed) * amt * 2.0), 1.0)
}

fn bricks(x: i32, y: i32, mortar: [f32; 3], brick: [f32; 3], seed: u32, bw: i32, bh: i32) -> [f32; 3] {
    let row = y / bh;
    let off = if row % 2 == 0 { 0 } else { bw / 2 };
    let bx = (x + off) % bw;
    if y % bh == bh - 1 || bx == bw - 1 {
        mul(mortar, 0.9 + hash(x, y, seed) * 0.2)
    } else {
        let brick_id = (x + off) / bw + row * 7;
        mul(brick, 0.82 + hash(brick_id, row, seed + 1) * 0.18 + hash(x, y, seed) * 0.14)
    }
}

pub fn build() -> Vec<u8> {
    let mut p = Painter { px: vec![0u8; (SIZE * SIZE * 4) as usize] };

    p.tile(T_WHITE, |_, _| ([1.0; 3], 1.0));
    p.tile(T_DOT, |x, y| {
        let d = ((x as f32 - 7.5).powi(2) + (y as f32 - 7.5).powi(2)).sqrt();
        ([1.0; 3], if d < 7.5 { 1.0 } else { 0.0 })
    });
    // Grass
    let grass = hex(0x3f6b2a);
    let dirt = hex(0x6b4a2f);
    p.tile(T_GRASS_TOP, noisy(grass, 0.18, 1));
    p.tile(T_DIRT, noisy(dirt, 0.16, 2));
    p.tile(T_GRASS_SIDE, |x, y| {
        let edge = 3 + (hash(x, 0, 9) * 2.5) as i32;
        if y < edge {
            (mul(grass, 0.85 + hash(x, y, 1) * 0.3), 1.0)
        } else {
            (mul(dirt, 0.84 + hash(x, y, 2) * 0.32), 1.0)
        }
    });
    // Stones
    p.tile(T_STONE, |x, y| {
        let n = hash(x / 2, y / 2, 3) * 0.6 + hash(x, y, 4) * 0.4;
        (mul(hex(0x7a7a80), 0.78 + n * 0.3), 1.0)
    });
    p.tile(T_COBBLE, |x, y| {
        let cell = hash(x / 4 + (y / 4) * 3, y / 4, 5);
        let edge = x % 4 == 0 || y % 4 == 0;
        let c = if edge { hex(0x4e4e52) } else { mul(hex(0x86868c), 0.75 + cell * 0.35) };
        (mul(c, 0.92 + hash(x, y, 6) * 0.16), 1.0)
    });
    p.tile(T_MOSSY, |x, y| {
        let cell = hash(x / 4 + (y / 4) * 3, y / 4, 5);
        let edge = x % 4 == 0 || y % 4 == 0;
        let mut c = if edge { hex(0x3e423a) } else { mul(hex(0x7d8278), 0.75 + cell * 0.35) };
        if hash(x / 2, y / 3, 77) > 0.6 {
            c = mul(hex(0x4f7a35), 0.8 + hash(x, y, 7) * 0.3);
        }
        (c, 1.0)
    });
    p.tile(T_STONEBRICK, |x, y| (bricks(x, y, hex(0x555559), hex(0x8b8b90), 8, 8, 4), 1.0));
    p.tile(T_CRYPT, |x, y| {
        let mut c = bricks(x, y, hex(0x1c1c22), hex(0x45424f), 10, 8, 4);
        if hash(x / 2, y, 78) > 0.82 {
            c = hex(0x3c5e3a);
        }
        (c, 1.0)
    });
    p.tile(T_PURPLE, |x, y| (bricks(x, y, hex(0x2b1638), hex(0x6a3f86), 11, 8, 8), 1.0));
    // Woods
    let plank = |base: [f32; 3], seed: u32| {
        move |x: i32, y: i32| {
            let row = y / 4;
            let seam = y % 4 == 3 || (x + row * 5) % 16 == 0;
            let grain = hash(x / 3, y, seed) * 0.12 + hash(row, 0, seed + 3) * 0.1;
            (if seam { mul(base, 0.6) } else { mul(base, 0.86 + grain) }, 1.0)
        }
    };
    p.tile(T_PLANK, plank(hex(0x9c7445), 11));
    p.tile(T_DARKPLANK, plank(hex(0x4a3020), 12));
    p.tile(T_LOG, |x, y| {
        let stripe = (x + (hash(0, y / 5, 13) * 3.0) as i32) % 4 == 0;
        (mul(hex(0x3d2c1e), if stripe { 0.7 } else { 0.95 + hash(x, y, 14) * 0.15 }), 1.0)
    });
    p.tile(T_LOG_TOP, |x, y| {
        let d = ((x as f32 - 7.5).powi(2) + (y as f32 - 7.5).powi(2)).sqrt();
        if d > 6.5 {
            (mul(hex(0x3d2c1e), 0.9), 1.0)
        } else {
            let ring = (d as i32) % 2 == 0;
            (mul(hex(0x8c6c44), if ring { 0.85 } else { 1.0 }), 1.0)
        }
    });
    p.tile(T_HAY, |x, y| {
        let band = y == 4 || y == 11;
        (if band { hex(0x7a4b1c) } else { mul(hex(0xc9a53a), 0.8 + hash(x, y / 2, 15) * 0.3) }, 1.0)
    });
    // Wallpapers
    p.tile(T_WALL_PURPLE, |x, y| {
        let stripe = x % 8 < 2;
        let motif = ((x % 8) as i32 - 5).abs() + ((y % 8) as i32 - 4).abs() <= 1;
        let c = if stripe {
            hex(0x2a1533)
        } else if motif {
            hex(0x8a5fa0)
        } else {
            hex(0x4b2a5c)
        };
        (mul(c, 0.92 + hash(x, y, 16) * 0.12), 1.0)
    });
    p.tile(T_WALL_GREEN, |x, y| {
        let diamond = ((x % 8) - 4).abs() + ((y % 8) - 4).abs() == 3;
        let c = if diamond { hex(0x8a8a4a) } else { hex(0x24402e) };
        (mul(c, 0.9 + hash(x, y, 17) * 0.15), 1.0)
    });
    p.tile(T_CARPET, |x, y| {
        let border = x < 2 || x > 13;
        let pat = (x + y) % 6 == 0 || (x - y + 32) % 6 == 0;
        let c = if border {
            hex(0xc8a03c)
        } else if pat {
            hex(0x9a2030)
        } else {
            hex(0x6e1420)
        };
        (mul(c, 0.92 + hash(x, y, 18) * 0.12), 1.0)
    });
    p.tile(T_CLOTH, |x, y| {
        let c = if (x / 4 + y / 4) % 2 == 0 { hex(0xe8e0d0) } else { hex(0xc8bfae) };
        (mul(c, 0.94 + hash(x, y, 19) * 0.08), 1.0)
    });
    p.tile(T_BOOKSHELF, |x, y| {
        if y < 2 || y >= 14 || y == 7 || y == 8 {
            return (mul(hex(0x9c7445), 0.85 + hash(x, y, 20) * 0.15), 1.0);
        }
        let shelf = if y < 7 { 0 } else { 1 };
        let book = (x + shelf * 3) / 2;
        let colors = [0x7a1e1e, 0x1e3a7a, 0x2e6a2a, 0x8a6a1e, 0x5a2a6a, 0x6a3a1e, 0x1e5a5a, 0x8a2a4a];
        let c = hex(colors[((hash(book, shelf, 21) * 8.0) as usize) % 8]);
        let top = if shelf == 0 { 2 } else { 9 };
        let h = (hash(book, shelf, 22) * 2.0) as i32;
        if y < top + h {
            (hex(0x1e140c), 1.0)
        } else if x % 2 == 1 && (x + shelf) % 6 == 0 {
            (hex(0x1e140c), 1.0)
        } else {
            (mul(c, 0.85 + hash(x, y, 23) * 0.2), 1.0)
        }
    });
    // Pumpkins
    let orange = hex(0xe07a12);
    p.tile(T_PUMPKIN_SIDE, move |x, y| {
        let ridge = x % 4 == 0;
        (mul(orange, if ridge { 0.72 } else { 0.92 + hash(x, y, 24) * 0.14 }), 1.0)
    });
    p.tile(T_PUMPKIN_TOP, move |x, y| {
        let d = ((x as f32 - 7.5).abs()).max((y as f32 - 7.5).abs());
        if d < 2.0 {
            (hex(0x4a6a20), 1.0)
        } else {
            (mul(orange, 0.85 + hash(x, y, 25) * 0.12), 1.0)
        }
    });
    const FACE: [&str; 16] = [
        "................",
        "................",
        "................",
        "...##......##...",
        "..####....####..",
        "..####....####..",
        "................",
        ".......##.......",
        "................",
        "..#..........#..",
        "..############..",
        "...####..####...",
        "....###..###....",
        "................",
        "................",
        "................",
    ];
    p.tile(T_PUMPKIN_FACE, move |x, y| {
        if FACE[y as usize].as_bytes()[x as usize] == b'#' {
            (mix(hex(0xffe060), hex(0xffa020), hash(x, y, 26) * 0.5), 1.0)
        } else {
            let ridge = x % 4 == 0;
            (mul(orange, if ridge { 0.72 } else { 0.92 + hash(x, y, 24) * 0.14 }), 1.0)
        }
    });
    // Cobweb (alpha)
    p.tile(T_COBWEB, |x, y| {
        let cx = x - 8;
        let cy = y - 8;
        let spoke = cx == 0 || cy == 0 || cx == cy || cx == -cy;
        let r = cx.abs().max(cy.abs());
        let ring = r == 3 || r == 6;
        let on = (spoke || ring) && r <= 7;
        ([0.92, 0.92, 0.95], if on { 1.0 } else { 0.0 })
    });
    // Windows
    p.tile(T_WINDOW, |x, y| {
        let frame = x == 0 || y == 0 || x == 15 || y == 15 || x == 7 || x == 8 || y == 7 || y == 8;
        if frame {
            (hex(0x2a1c12), 1.0)
        } else {
            (mix(hex(0x1a2a40), hex(0x3a5a80), hash(x / 3, y / 3, 27) * 0.5), 1.0)
        }
    });
    p.tile(T_WINDOW_LIT, |x, y| {
        let frame = x == 0 || y == 0 || x == 15 || y == 15 || x == 7 || x == 8 || y == 7 || y == 8;
        if frame {
            (hex(0x2a1c12), 1.0)
        } else {
            (mix(hex(0xffb040), hex(0xffe080), hash(x / 2, y / 2, 28)), 1.0)
        }
    });
    p.tile(T_GRAVE, |x, y| {
        // Headstone face: bevelled edge, a small cross, and "RIP" carved underneath.
        const RIP: [&str; 5] = ["###.###.###", "#.#..#..#.#", "##...#..###", "#.#..#..#..", "#.#.###.#.."];
        let edge = x == 0 || x == 15 || y == 15 || y == 0;
        let cross = (x == 7 || x == 8) && (2..=6).contains(&y) || y == 3 && (5..=10).contains(&x);
        let text = (9..14).contains(&y) && (2..13).contains(&x) && RIP[(y - 9) as usize].as_bytes()[(x - 2) as usize] == b'#';
        let moss = y >= 13 && hash(x, y, 30) > 0.6;
        let c = if edge {
            mul(hex(0x6e6e76), 0.9 + hash(x, y, 29) * 0.15)
        } else if cross || text {
            hex(0x4a4a52)
        } else if moss {
            mul(hex(0x5a7a40), 0.9 + hash(x, y, 31) * 0.2)
        } else {
            mul(hex(0x9a9aa2), 0.88 + hash(x, y, 29) * 0.16)
        };
        (c, 1.0)
    });
    p.tile(T_METAL, |x, y| (mul(hex(0x8a8f98), 0.8 + hash(x, y / 4, 30) * 0.3), 1.0));
    p.tile(T_BONE, |x, y| {
        let gap = (x + y * 3) % 7 == 0;
        (if gap { hex(0x8e8a78) } else { mul(hex(0xe6e0c8), 0.9 + hash(x, y, 31) * 0.1) }, 1.0)
    });
    p.tile(T_RING, |x, y| {
        let d = ((x as f32 - 7.5).powi(2) + (y as f32 - 7.5).powi(2)).sqrt();
        let ring = d > 4.6 && d < 7.0;
        let cross = (x == 7 || x == 8) && (y < 4 || y > 11) || (y == 7 || y == 8) && (x < 4 || x > 11);
        ([1.0; 3], if ring || cross { 1.0 } else { 0.0 })
    });
    p.tile(T_LANTERN, |x, y| {
        let frame = x < 2 || x > 13 || y < 2 || y > 13 || x == 7 || x == 8;
        if frame {
            (hex(0x2a2a30), 1.0)
        } else {
            (mix(hex(0xffd070), hex(0xfff0b0), hash(x, y, 32)), 1.0)
        }
    });
    p.tile(T_ROOF, |x, y| {
        let row = y / 4;
        let off = if row % 2 == 0 { 0 } else { 2 };
        let edge = y % 4 == 3 || (x + off) % 4 == 0;
        (mul(hex(0x3a2a40), if edge { 0.6 } else { 0.9 + hash(x, y, 33) * 0.2 }), 1.0)
    });
    // Portraits: dark frame with a spooky abstract figure.
    let portrait = |bg: u32, fig: u32, seed: u32| {
        move |x: i32, y: i32| {
            let frame = x < 2 || x > 13 || y < 2 || y > 13;
            if frame {
                return (mul(hex(0xa07a2a), 0.75 + hash(x, y, seed) * 0.3), 1.0);
            }
            let head = ((x - 8) * (x - 8) + (y - 6) * (y - 6)) <= 6;
            let body = y >= 9 && (x - 8).abs() <= (y - 7);
            let eyes = y == 6 && (x == 7 || x == 9);
            if eyes {
                (hex(0xff4040), 1.0)
            } else if head || body {
                (mul(hex(fig), 0.9 + hash(x, y, seed + 1) * 0.15), 1.0)
            } else {
                (mul(hex(bg), 0.85 + hash(x / 2, y / 2, seed + 2) * 0.3), 1.0)
            }
        }
    };
    p.tile(T_PORTRAIT, portrait(0x2a3a2a, 0x1a1a22, 34));
    p.tile(T_PORTRAIT2, portrait(0x3a2030, 0x2a2018, 35));
    p.tile(T_AMETHYST, |x, y| {
        let facet = ((x * 3 + y * 5) / 5) % 3;
        let c = [hex(0xb070ff), hex(0xd8a8ff), hex(0x8040d0)][facet as usize];
        (mul(c, 0.9 + hash(x, y, 36) * 0.15), 1.0)
    });
    p.tile(T_FIRE, |x, y| {
        let h = 16.0 - (y as f32);
        let w = 6.5 - h * 0.35 + hash(x, y / 2, 37) * 2.0;
        let on = (x as f32 - 7.5).abs() < w && h > 1.0;
        let c = mix(hex(0xffe060), hex(0xff5010), y as f32 / 16.0 * 0.4 + hash(x, y, 38) * 0.3);
        (mix(c, hex(0xff3000), 1.0 - y as f32 / 16.0), if on { 1.0 } else { 0.0 })
    });
    p.tile(T_STAR, |x, y| {
        let on = (x == 7 || x == 8) && (5..11).contains(&y) || (y == 7 || y == 8) && (5..11).contains(&x);
        ([1.0, 1.0, 0.95], if on { 1.0 } else { 0.0 })
    });
    p.tile(T_TALLGRASS, |x, y| {
        let blade = [2, 5, 7, 10, 13][(x as usize) % 5];
        let h = 4 + (hash(x, 0, 50) * 11.0) as i32;
        let on = (x == blade || x % 3 == 1) && y >= 16 - h;
        (mul(hex(0x4f7a32), 0.75 + hash(x, y, 51) * 0.4), if on { 1.0 } else { 0.0 })
    });
    p.tile(T_DEADBUSH, |x, y| {
        // Twiggy branches spreading up from the bottom centre.
        let fx = x as f32 - 7.5;
        let fy = 15.0 - y as f32;
        let mut on = (fx.abs() < 1.0) && fy < 6.0;
        for (k, a) in [(-0.9f32), -0.4, 0.3, 0.8, 1.2].iter().enumerate() {
            let bx = fy * a.sin() * 0.9 + (k as f32 - 2.0) * 0.6;
            if (fx - bx).abs() < 0.8 && fy > 2.0 + k as f32 && fy < 14.0 - k as f32 {
                on = true;
            }
        }
        (mul(hex(0x6b4a2a), 0.8 + hash(x, y, 52) * 0.3), if on { 1.0 } else { 0.0 })
    });
    p.tile(T_MUSHROOM, |x, y| {
        let half = 7.2 * (1.0 - ((y as f32 - 8.6) / 5.0).powi(2)).max(0.0).sqrt();
        let cap = (4..=8).contains(&y) && (x as f32 - 7.5).abs() < half;
        let stem = (x == 7 || x == 8) && y > 8 && y < 16;
        let dot = cap && hash(x / 2, y / 2, 53) > 0.6;
        if cap {
            (if dot { hex(0xfff0e0) } else { hex(0xd02a2a) }, 1.0)
        } else if stem {
            (hex(0xe8dcc0), 1.0)
        } else {
            ([0.0; 3], 0.0)
        }
    });
    p.tile(T_CLOCK, |x, y| {
        let d = ((x as f32 - 7.5).powi(2) + (y as f32 - 7.5).powi(2)).sqrt();
        let c = if d < 5.5 {
            let hand = (x == 7 || x == 8) && y >= 3 && y <= 8 || (y == 7 || y == 8) && x >= 7 && x <= 11;
            let tick = (d > 4.3) && ((x == 7 || x == 8) || (y == 7 || y == 8));
            if hand || tick { hex(0x1a1410) } else { hex(0xf0e6c8) }
        } else if d < 6.5 {
            hex(0xc8a03c)
        } else {
            mul(hex(0x4a3020), 0.85 + hash(x, y, 54) * 0.2)
        };
        (c, 1.0)
    });
    p.tile(T_CANDY, |x, y| {
        let c = if ((x + y) / 3) % 2 == 0 { hex(0xffffff) } else { hex(0xe0304a) };
        (mul(c, 0.92 + hash(x, y, 55) * 0.08), 1.0)
    });
    p.tile(T_CANDY2, |x, y| {
        let c = match ((x - y + 32) / 3) % 3 { 0 => hex(0xff9a1e), 1 => hex(0x9a40e0), _ => hex(0x50d050) };
        (mul(c, 0.92 + hash(x, y, 56) * 0.08), 1.0)
    });
    p.tile(T_IRON, |x, y| {
        let edge = x == 0 || y == 0 || x == 15 || y == 15;
        let rivet = (x == 2 || x == 13) && (y == 2 || y == 13);
        (if rivet { hex(0xe8ecf4) } else if edge { hex(0x6a6e78) } else { mul(hex(0xb0b6c2), 0.85 + hash(x, y / 3, 57) * 0.2) }, 1.0)
    });
    p.tile(T_CAKE_SIDE, |x, y| {
        let c = if y < 4 {
            hex(0xf8f0f8)
        } else if y < 6 && x % 4 < 2 {
            hex(0xf8f0f8)
        } else if y == 9 {
            hex(0xff70a0)
        } else {
            hex(0x6a3a20)
        };
        (mul(c, 0.94 + hash(x, y, 58) * 0.08), 1.0)
    });
    p.tile(T_CAKE_TOP, |x, y| {
        let sprinkle = hash(x, y, 59) > 0.88;
        let c = if sprinkle { [hex(0xff4060), hex(0x40c0ff), hex(0xffe040)][(hash(y, x, 60) * 3.0) as usize % 3] } else { hex(0xf8f0f8) };
        (c, 1.0)
    });
    p.tile(T_BANNER, |x, y| {
        // Purple banner with an orange bat and a gold fringe; pointed bottom.
        if y > 12 && ((x as i32 - 7).abs() + (y - 12) > 6) {
            return ([0.0; 3], 0.0);
        }
        if x < 1 || x > 14 {
            return ([0.0; 3], 0.0);
        }
        let bat = (y == 6 && (3..13).contains(&x) && x != 7 && x != 8) || (y == 5 && (5..11).contains(&x)) || (y == 7 && (6..10).contains(&x)) || (y == 4 && (x == 6 || x == 9));
        let c = if y < 2 {
            hex(0xd8a020)
        } else if bat {
            hex(0xff8a1e)
        } else {
            mul(hex(0x4a1a6a), 0.85 + hash(x, y / 2, 61) * 0.2)
        };
        (c, 1.0)
    });
    p.tile(T_CHAIN, |x, y| {
        let link = if (y / 4) % 2 == 0 { (x == 6 || x == 9) && y % 4 != 0 || (x >= 6 && x <= 9 && y % 4 == 0) } else { x == 7 || x == 8 };
        (mul(hex(0x7a7e88), 0.8 + hash(x, y, 62) * 0.3), if link { 1.0 } else { 0.0 })
    });
    p.tile(T_SKULLS, |x, y| {
        let cx = x % 8;
        let cy = y % 8;
        let eye = cy == 3 && (cx == 2 || cx == 5);
        let mouth = cy == 6 && cx % 2 == 1;
        let c = if eye || mouth { hex(0x1a1a1e) } else if cy == 0 || cx == 0 { hex(0x8a8474) } else { mul(hex(0xe2dcc6), 0.9 + hash(x, y, 63) * 0.1) };
        (c, 1.0)
    });
    p.tile(T_POT, |x, y| {
        let band = y == 3 || y == 12;
        (mul(if band { hex(0xd8a020) } else { hex(0x8a4a2a) }, 0.85 + hash(x, y, 64) * 0.2), 1.0)
    });
    p.tile(T_SLIME, |x, y| (mix(hex(0x40ff70), hex(0xb0ffb0), hash(x / 2, y / 2, 65)), 1.0));
    p.tile(T_PAINTING3, |x, y| {
        // Moonlit landscape: a haunted house on a hill under a big moon.
        if !(2..=13).contains(&x) || !(2..=13).contains(&y) {
            return (mul(hex(0xc89a30), 0.75 + hash(x, y, 70) * 0.3), 1.0);
        }
        let moon = (x - 10) * (x - 10) + (y - 5) * (y - 5) <= 3;
        let hill = y > 10 - ((x as f32 - 6.0) * 0.6).abs() as i32 / 2;
        let house = (4..=7).contains(&x) && (6..=9).contains(&y) || (x == 5 || x == 6) && y == 5;
        let window = (x == 5 && y == 7) || (x == 6 && y == 8);
        let c = if moon {
            hex(0xf4efd0)
        } else if window {
            hex(0xffc040)
        } else if house {
            hex(0x1a1420)
        } else if hill {
            mul(hex(0x23301e), 0.85 + hash(x, y, 71) * 0.2)
        } else {
            mix(hex(0x1c2450), hex(0x3a3070), y as f32 / 12.0)
        };
        (c, 1.0)
    });
    p.tile(T_SHIELD, |x, y| {
        // Red and gold crest with a little jack-o'-lantern in the middle.
        let wall = mul(hex(0x4a3020), 0.85 + hash(x, y, 72) * 0.2);
        let width = if y < 10 { 6 } else { 6 - (y - 9) };
        let inside = y >= 2 && y <= 14 && (x as i32 - 7).abs() <= width && (x as i32 - 8).abs() <= width;
        if !inside {
            return (wall, 1.0);
        }
        let edge = y == 2 || (x as i32 - 7).abs() == width || (x as i32 - 8).abs() == width;
        let pumpkin = ((x - 7) * (x - 7) + (y - 8) * (y - 8)) <= 6;
        let face = pumpkin && ((y == 7 && (x == 6 || x == 9)) || (y == 9 && (6..=9).contains(&x)));
        let c = if edge {
            hex(0xf0c040)
        } else if face {
            hex(0xffe060)
        } else if pumpkin {
            hex(0xe07a12)
        } else if (x + y) % 2 == 0 {
            hex(0xa01828)
        } else {
            hex(0x8a1020)
        };
        (c, 1.0)
    });
    p.tile(T_GOLD, |x, y| {
        let edge = x == 0 || y == 0 || x == 15 || y == 15;
        let shine = (x + y) % 9 == 0 || (x + y) % 9 == 1;
        // Antique gold: rich but not glaring under torchlight.
        let c = if edge {
            hex(0x6e4a10)
        } else if shine {
            hex(0xe8cc70)
        } else {
            mul(hex(0xb8862a), 0.85 + hash(x / 2, y / 2, 73) * 0.2)
        };
        (c, 1.0)
    });
    p.tile(T_MIRROR, |x, y| {
        let frame = !(2..=13).contains(&x) || !(1..=14).contains(&y);
        let streak = (x + y) % 7 == 0 && !frame;
        let c = if frame {
            mul(hex(0xc89a30), 0.8 + hash(x, y, 74) * 0.3)
        } else if streak {
            hex(0xd0e0f0)
        } else {
            mix(hex(0x5a6a8a), hex(0x8aa0c0), y as f32 / 16.0)
        };
        (c, 1.0)
    });
    p.tile(T_MOON, |x, y| {
        let crater = hash(x / 3, y / 3, 39) > 0.72;
        (mul(hex(0xf4efd0), if crater { 0.85 } else { 1.0 }), 1.0)
    });

    // Font: 8x8 glyphs, 32 per row, starting at y = 128.
    for c in 0u32..128 {
        if let Some(rows) = font8x8::legacy::BASIC_LEGACY.get(c as usize) {
            for (gy, row) in rows.iter().enumerate() {
                for gx in 0..8u32 {
                    if row & (1 << gx) != 0 {
                        let x = (c % 32) * 8 + gx;
                        let y = 128 + (c / 32) * 8 + gy as u32;
                        let i = ((y * SIZE + x) * 4) as usize;
                        p.px[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
                    }
                }
            }
        }
    }
    p.px
}
