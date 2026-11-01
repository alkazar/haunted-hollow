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
pub const T_GRAVE_PLAIN: u8 = 57;
// Christmas.
pub const T_SNOW_TOP: u8 = 58;
pub const T_SNOW_SIDE: u8 = 59;
pub const T_ICE: u8 = 60;
pub const T_ICE_BRICK: u8 = 61;
pub const T_PINE: u8 = 62;
pub const T_PINE_SNOW: u8 = 63;
pub const T_CANDY_CANE: u8 = 64;
pub const T_PRESENT_RED: u8 = 65;
pub const T_PRESENT_RED_TOP: u8 = 66;
pub const T_PRESENT_GREEN: u8 = 67;
pub const T_PRESENT_GREEN_TOP: u8 = 68;
pub const T_GINGERBREAD: u8 = 69;
pub const T_GARLAND: u8 = 70;
pub const T_STOCKING: u8 = 71;
pub const T_WREATH: u8 = 72;
pub const T_ICICLE: u8 = 73;
pub const T_ICE_CRYSTAL: u8 = 74;
pub const T_XSTAR: u8 = 75;
pub const T_WALL_RED: u8 = 76;
pub const T_WALL_PINE: u8 = 77;
pub const T_ROOF_RED: u8 = 78;
pub const T_TOYSHELF: u8 = 79;
pub const T_SNOWMAN_FACE: u8 = 80;
pub const T_XMAS_ART: u8 = 81;
pub const T_XMAS_ART2: u8 = 82;
pub const T_XMAS_ART3: u8 = 83;
pub const T_FROSTING: u8 = 84;
pub const T_BRICK: u8 = 85;

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
    // Headstone: bevelled stone with "RIP" carved in, and a plain version (no text)
    // for the back and sides.
    let headstone = |text: bool| {
        move |x: i32, y: i32| {
            const RIP: [&str; 5] = ["###.###.###", "#.#..#..#.#", "##...#..###", "#.#..#..#..", "#.#.###.#.."];
            let edge = x == 0 || x == 15 || y == 15 || y == 0;
            let carved = text && (5..10).contains(&y) && (2..13).contains(&x) && RIP[(y - 5) as usize].as_bytes()[(x - 2) as usize] == b'#';
            let moss = y >= 13 && hash(x, y, 30) > 0.6;
            let c = if edge {
                mul(hex(0x6e6e76), 0.9 + hash(x, y, 29) * 0.15)
            } else if carved {
                hex(0x4a4a52)
            } else if moss {
                mul(hex(0x5a7a40), 0.9 + hash(x, y, 31) * 0.2)
            } else {
                mul(hex(0x9a9aa2), 0.88 + hash(x, y, 29) * 0.16)
            };
            (c, 1.0)
        }
    };
    p.tile(T_GRAVE, headstone(true));
    p.tile(T_GRAVE_PLAIN, headstone(false));
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

    christmas_tiles(&mut p);

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

/// The Christmas level's tiles.
fn christmas_tiles(p: &mut Painter) {
    let snow = hex(0xeef4ff);
    p.tile(T_SNOW_TOP, move |x, y| {
        let sparkle = hash(x, y, 101) > 0.96;
        let c = if sparkle { hex(0xffffff) } else { mul(snow, 0.9 + hash(x / 2, y / 2, 102) * 0.08 + hash(x, y, 103) * 0.04) };
        (c, 1.0)
    });
    p.tile(T_SNOW_SIDE, move |x, y| {
        // Snow drifts down over the dirt in a wavy edge.
        let edge = 4 + (hash(x / 2, 0, 104) * 3.0) as i32;
        if y < edge {
            (mul(snow, 0.88 + hash(x, y, 105) * 0.1), 1.0)
        } else {
            (mul(hex(0x6b4a2f), 0.84 + hash(x, y, 2) * 0.32), 1.0)
        }
    });
    p.tile(T_FROSTING, move |x, y| {
        let swirl = (x + y / 2) % 5 == 0;
        (mul(hex(0xfdf8f2), if swirl { 0.88 } else { 0.96 + hash(x, y, 106) * 0.04 }), 1.0)
    });
    p.tile(T_ICE, |x, y| {
        let crack = (x * 3 + y * 5) % 17 == 0 || (x * 7 - y * 2 + 64) % 23 == 0;
        let base = mix(hex(0x7fb4e8), hex(0xb8dcff), hash(x / 3, y / 3, 107));
        (if crack { hex(0xe8f6ff) } else { mul(base, 0.92 + hash(x, y, 108) * 0.1) }, 1.0)
    });
    p.tile(T_BRICK, |x, y| (bricks(x, y, hex(0xd8d0c8), hex(0x9a3a2a), 140, 8, 4), 1.0));
    p.tile(T_ICE_BRICK, |x, y| (bricks(x, y, hex(0xd8ecff), hex(0x6fa2d8), 109, 8, 4), 1.0));
    let needles = |x: i32, y: i32, seed: u32| -> [f32; 3] {
        let n = hash(x, y, seed);
        let dark = hash(x / 2 + y, y / 2, seed + 1) > 0.55;
        mul(if dark { hex(0x1c4a2a) } else { hex(0x2b6a3a) }, 0.8 + n * 0.35)
    };
    p.tile(T_PINE, move |x, y| (needles(x, y, 110), 1.0));
    p.tile(T_PINE_SNOW, move |x, y| {
        let edge = 3 + (hash(x / 2, 0, 112) * 3.0) as i32;
        if y < edge {
            (mul(hex(0xeef4ff), 0.9 + hash(x, y, 113) * 0.1), 1.0)
        } else {
            (needles(x, y, 110), 1.0)
        }
    });
    p.tile(T_CANDY_CANE, |x, y| {
        let red = (x + y).rem_euclid(8) < 3;
        let c = if red { hex(0xd8202a) } else { hex(0xfaf6f0) };
        (mul(c, 0.94 + hash(x, y, 114) * 0.08), 1.0)
    });
    let present = |paper: [f32; 3], ribbon: [f32; 3], top: bool| {
        move |x: i32, y: i32| {
            let band = (x == 7 || x == 8) || (top && (y == 7 || y == 8));
            let bow = top && (6..=9).contains(&x) && (6..=9).contains(&y) && (x + y) % 2 == 0;
            let dots = !band && (x % 4 == 1 && y % 4 == 1);
            let c = if bow {
                mul(ribbon, 1.15)
            } else if band {
                ribbon
            } else if dots {
                mix(paper, [1.0; 3], 0.6)
            } else {
                paper
            };
            (mul(c, 0.94 + hash(x, y, 115) * 0.08), 1.0)
        }
    };
    p.tile(T_PRESENT_RED, present(hex(0xc8202c), hex(0xf0c840), false));
    p.tile(T_PRESENT_RED_TOP, present(hex(0xc8202c), hex(0xf0c840), true));
    p.tile(T_PRESENT_GREEN, present(hex(0x1f7a3a), hex(0xe83040), false));
    p.tile(T_PRESENT_GREEN_TOP, present(hex(0x1f7a3a), hex(0xe83040), true));
    p.tile(T_GINGERBREAD, |x, y| {
        // Cookie bricks with white icing lines.
        let icing = y % 8 == 0 || (x + if (y / 8) % 2 == 0 { 0 } else { 8 }) % 16 == 0;
        let crumb = hash(x, y, 116) > 0.85;
        let c = if icing {
            hex(0xfdf8f2)
        } else if crumb {
            hex(0x7a4420)
        } else {
            mul(hex(0xa8622c), 0.9 + hash(x / 2, y / 2, 117) * 0.15)
        };
        (c, 1.0)
    });
    p.tile(T_GARLAND, |x, y| {
        // A drooping swag of pine with red berries and gold baubles (alpha outside).
        let sag = 4.0 + 5.0 * (1.0 - ((x as f32 - 7.5) / 8.0).powi(2));
        let d = (y as f32 - sag).abs();
        if d > 2.6 {
            return ([0.0; 3], 0.0);
        }
        let c = if x % 5 == 2 && d < 1.0 {
            if x % 10 == 2 { hex(0xe02030) } else { hex(0xf0c040) }
        } else {
            needles(x, y, 118)
        };
        (c, 1.0)
    });
    const STOCKING: [&str; 16] = [
        "................",
        "....WWWWWWW.....",
        "....WWWWWWW.....",
        "....WWWWWWW.....",
        ".....RRRRR......",
        ".....RRGRR......",
        ".....RRRRR......",
        ".....RRGRR......",
        ".....RRRRR......",
        ".....RRRRRR.....",
        ".....RRGRRRRR...",
        ".....RRRRRRRRR..",
        "......RRRRRRRR..",
        ".......RRRRRR...",
        "................",
        "................",
    ];
    p.tile(T_STOCKING, |x, y| match STOCKING[y as usize].as_bytes()[x as usize] {
        b'W' => (mul(hex(0xfaf6f0), 0.92 + hash(x, y, 119) * 0.08), 1.0),
        b'R' => (mul(hex(0xc8202c), 0.9 + hash(x, y, 120) * 0.12), 1.0),
        b'G' => (hex(0x2a8a40), 1.0),
        _ => ([0.0; 3], 0.0),
    });
    p.tile(T_WREATH, |x, y| {
        let d = ((x as f32 - 7.5).powi(2) + (y as f32 - 7.0).powi(2)).sqrt();
        let bow = y >= 11 && y <= 14 && (x as i32 - 7).abs() + (y as i32 - 12).abs() <= 3 && (x - 7).abs() != 0;
        if bow {
            return (hex(0xd8202a), 1.0);
        }
        if !(3.5..=7.2).contains(&d) {
            return ([0.0; 3], 0.0);
        }
        let berry = hash(x, y, 121) > 0.84;
        (if berry { hex(0xe02030) } else { needles(x, y, 122) }, 1.0)
    });
    p.tile(T_ICICLE, |x, y| {
        // Three icicles of different lengths hanging from the top.
        let len = |cx: i32, l: i32| -> bool {
            let w = 2 - (y * 2 / l.max(1)).min(2);
            y < l && (x - cx).abs() <= w.max(0)
        };
        if len(3, 14) || len(8, 9) || len(12, 12) {
            (mix(hex(0xbfe4ff), hex(0xffffff), hash(x, y, 123) * 0.6), 1.0)
        } else {
            ([0.0; 3], 0.0)
        }
    });
    p.tile(T_ICE_CRYSTAL, |x, y| {
        let facet = ((x - 8).abs() + (y - 8).abs()) % 5 == 0;
        (if facet { hex(0xe8ffff) } else { mix(hex(0x48c8f0), hex(0x9af0ff), hash(x / 2, y / 2, 124)) }, 1.0)
    });
    p.tile(T_XSTAR, |x, y| {
        // A five-pointed star: points around a centre.
        let (dx, dy) = (x as f32 - 7.5, y as f32 - 8.0);
        let r = (dx * dx + dy * dy).sqrt();
        let a = dy.atan2(dx) + std::f32::consts::FRAC_PI_2;
        let k = (a / std::f32::consts::TAU * 5.0).rem_euclid(1.0);
        let reach = 3.2 + 4.3 * (1.0 - (k - 0.5).abs() * 2.0).powi(2);
        if r <= reach {
            (mix(hex(0xfff2a0), hex(0xffc020), r / 8.0), 1.0)
        } else {
            ([0.0; 3], 0.0)
        }
    });
    p.tile(T_WALL_RED, |x, y| {
        // Deep red with gold stripes and little holly sprigs.
        let stripe = x % 8 == 0;
        let holly = (x % 8 == 4) && (y % 8 == 3 || y % 8 == 4);
        let berry = (x % 8 == 4) && y % 8 == 5;
        let c = if stripe {
            hex(0xb8902a)
        } else if berry {
            hex(0xff4040)
        } else if holly {
            hex(0x2a7a3a)
        } else {
            hex(0x7a1420)
        };
        (mul(c, 0.92 + hash(x, y, 125) * 0.1), 1.0)
    });
    p.tile(T_WALL_PINE, |x, y| {
        // Green with a white snowflake motif.
        let (fx, fy) = ((x % 8) - 4, (y % 8) - 4);
        let flake = (fx == 0 || fy == 0 || fx == fy || fx == -fy) && fx.abs() <= 2 && fy.abs() <= 2;
        let c = if flake { hex(0xd8ecd8) } else { hex(0x1e4a2e) };
        (mul(c, 0.92 + hash(x, y, 126) * 0.1), 1.0)
    });
    p.tile(T_ROOF_RED, |x, y| {
        let row = y / 4;
        let off = if row % 2 == 0 { 0 } else { 2 };
        let seam = y % 4 == 3 || (x + off) % 4 == 0;
        (mul(hex(0x8a1c1c), if seam { 0.65 } else { 0.9 + hash(x, y, 127) * 0.15 }), 1.0)
    });
    p.tile(T_TOYSHELF, |x, y| {
        if y < 2 || y >= 14 || y == 7 || y == 8 {
            return (mul(hex(0x9c7445), 0.85 + hash(x, y, 128) * 0.15), 1.0);
        }
        // Toys on two shelves: a ball, a block, a little soldier and a teddy.
        let shelf = if y < 7 { 0 } else { 1 };
        let slot = (x / 4 + shelf * 2) % 4;
        let (lx, ly) = (x % 4, y - if shelf == 0 { 2 } else { 9 });
        let back = (hex(0x3a2414), 1.0);
        let c = match slot {
            0 if (lx - 1).abs() + (ly - 3).abs() <= 2 && lx < 3 => hex(0xe03030),
            1 if lx < 3 && ly >= 2 => if (lx + ly) % 2 == 0 { hex(0x3a70e0) } else { hex(0xf0d040) },
            2 if lx == 1 && ly >= 0 => if ly < 2 { hex(0x202020) } else if ly < 4 { hex(0xd02020) } else { hex(0x2040a0) },
            3 if lx < 3 && ly >= 1 => hex(0xa86a30),
            _ => return back,
        };
        (mul(c, 0.9 + hash(x, y, 129) * 0.15), 1.0)
    });
    const SNOWMAN: [&str; 16] = [
        "................",
        "................",
        "................",
        "....##....##....",
        "....##....##....",
        "................",
        ".......oo.......",
        ".......ooooo....",
        "................",
        "...#........#...",
        "....#......#....",
        ".....##..##.....",
        ".......##.......",
        "................",
        "................",
        "................",
    ];
    p.tile(T_SNOWMAN_FACE, move |x, y| match SNOWMAN[y as usize].as_bytes()[x as usize] {
        b'#' => (hex(0x1a1a20), 1.0),
        b'o' => (hex(0xf07818), 1.0),
        _ => (mul(snow, 0.9 + hash(x / 2, y / 2, 102) * 0.08), 1.0),
    });
    // Paintings: a snowy cottage at night, a Christmas tree, and a reindeer under the moon.
    let frame = |x: i32, y: i32| x == 0 || y == 0 || x == 15 || y == 15;
    p.tile(T_XMAS_ART, move |x, y| {
        let c = if frame(x, y) {
            hex(0x6e4a10)
        } else if y >= 11 {
            mul(snow, 0.9)
        } else if (5..=10).contains(&x) && y >= 7 {
            if (x == 7 || x == 8) && y >= 9 { hex(0x4a2a14) } else if x == 6 && y == 8 { hex(0xffd060) } else { hex(0x8a3020) }
        } else if (4..=11).contains(&x) && y >= 4 && (x as i32 - 7).abs() <= (y - 3) && y < 7 {
            mul(snow, 0.95)
        } else if hash(x, y, 130) > 0.9 {
            hex(0xffffff)
        } else {
            mix(hex(0x10183a), hex(0x2a3870), y as f32 / 11.0)
        };
        (c, 1.0)
    });
    p.tile(T_XMAS_ART2, move |x, y| {
        let tree = y >= 3 && y <= 12 && (x as i32 - 7).abs() * 2 <= (y - 2) && (x as i32 - 8).abs() * 2 <= (y - 2);
        let c = if frame(x, y) {
            hex(0x6e4a10)
        } else if y == 2 && (x == 7 || x == 8) {
            hex(0xffe060)
        } else if tree {
            if (x * 3 + y * 5) % 7 == 0 { [hex(0xff3030), hex(0x40a0ff), hex(0xffd030)][(x as usize + y as usize) % 3] } else { hex(0x1f6a30) }
        } else if y >= 13 {
            if (x == 7 || x == 8) && y == 13 { hex(0x5a3418) } else { hex(0x7a1420) }
        } else {
            mul(hex(0x8a2028), 0.9 + hash(x, y, 131) * 0.1)
        };
        (c, 1.0)
    });
    p.tile(T_XMAS_ART3, move |x, y| {
        let moon = (x - 11) * (x - 11) + (y - 4) * (y - 4) <= 5;
        let body = (4..=9).contains(&x) && (8..=10).contains(&y);
        let legs = (y == 11 || y == 12) && (x == 4 || x == 9);
        let neck = x == 10 && (6..=8).contains(&y);
        let head = (10..=11).contains(&x) && (5..=6).contains(&y);
        let antler = (y == 3 || y == 4) && (x == 9 || x == 11) || (y == 3 && x == 10);
        let c = if frame(x, y) {
            hex(0x6e4a10)
        } else if body || legs || neck || head {
            if head && x == 11 && y == 6 { hex(0xff3030) } else { hex(0x6a4020) }
        } else if antler {
            hex(0xc8a070)
        } else if moon {
            hex(0xfff6d0)
        } else if y >= 13 {
            mul(snow, 0.9)
        } else {
            mix(hex(0x14204a), hex(0x34407a), y as f32 / 13.0)
        };
        (c, 1.0)
    });
}
