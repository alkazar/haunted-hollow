//! Tiny offline synthesizer: renders the (original) music loops and all sound
//! effects into sample buffers at start-up. No audio files needed.

use std::f32::consts::TAU;

pub const RATE: f32 = 44100.0;

/// Stereo buffer, interleaved L/R.
#[derive(Clone)]
pub struct Buf {
    pub data: Vec<f32>,
}

impl Buf {
    pub fn new(seconds: f32) -> Buf {
        Buf { data: vec![0.0; ((seconds * RATE) as usize) * 2] }
    }
    pub fn frames(&self) -> usize {
        self.data.len() / 2
    }
    /// Adds a mono sample at frame `i` with constant-power pan; wraps if `wrap`.
    #[inline]
    fn add(&mut self, i: usize, v: f32, pan: f32, wrap: bool) {
        let n = self.frames();
        let i = if wrap { i % n } else if i >= n { return } else { i };
        let p = (pan.clamp(-1.0, 1.0) + 1.0) * 0.25 * std::f32::consts::PI;
        self.data[i * 2] += v * p.cos();
        self.data[i * 2 + 1] += v * p.sin();
    }
    pub fn normalize(&mut self, peak: f32) {
        let m = self.data.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        if m > 1e-6 {
            let k = peak / m;
            self.data.iter_mut().for_each(|s| *s *= k);
        }
    }
}

struct Noise(u32);
impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

pub fn note_freq(name: &str) -> f32 {
    let b = name.as_bytes();
    let base = match b[0] {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => 0,
    };
    let mut i = 1;
    let mut semi = base as i32;
    while i < b.len() && (b[i] == b'#' || b[i] == b'b') {
        semi += if b[i] == b'#' { 1 } else { -1 };
        i += 1;
    }
    let oct: i32 = name[i..].parse().unwrap_or(4);
    let midi = (oct + 1) * 12 + semi;
    440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0)
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Inst {
    Organ,
    Theremin,
    Xylo,
    Bass,
    Bell,
    Brass,
    Chip,
    Pad,
}

/// Renders one note into the buffer.
pub fn note(buf: &mut Buf, start: f32, freq: f32, dur: f32, inst: Inst, vol: f32, pan: f32, wrap: bool) {
    let s0 = (start * RATE) as usize;
    let (attack, release, tail): (f32, f32, f32) = match inst {
        Inst::Organ => (0.03, 0.12, 0.12),
        Inst::Theremin => (0.08, 0.25, 0.25),
        Inst::Xylo => (0.002, 0.0, 0.5),
        Inst::Bass => (0.005, 0.06, 0.1),
        Inst::Bell => (0.002, 0.0, 1.8),
        Inst::Brass => (0.02, 0.08, 0.1),
        Inst::Chip => (0.004, 0.05, 0.06),
        Inst::Pad => (0.3, 0.6, 0.6),
    };
    let total = dur + tail;
    let n = (total * RATE) as usize;
    let mut phase = 0.0f32;
    for k in 0..n {
        let t = k as f32 / RATE;
        let env = if t < attack {
            t / attack
        } else if t < dur {
            1.0
        } else {
            (1.0 - (t - dur) / release.max(1e-3)).max(0.0)
        };
        let vib = match inst {
            Inst::Theremin => 1.0 + 0.012 * (TAU * 6.0 * t).sin() * (t * 3.0).min(1.0),
            Inst::Organ | Inst::Pad => 1.0 + 0.003 * (TAU * 5.0 * t).sin(),
            _ => 1.0,
        };
        phase += freq * vib / RATE;
        let p = phase.fract();
        let x = TAU * phase;
        let v = match inst {
            Inst::Organ => {
                (x.sin() + 0.5 * (2.0 * x).sin() + 0.33 * (3.0 * x).sin() + 0.25 * (4.0 * x).sin() + 0.4 * (0.5 * x).sin()) * 0.35 * env
            }
            Inst::Theremin => (x.sin() + 0.12 * (2.0 * x).sin()) * env,
            Inst::Xylo => {
                let d = (-t * 9.0).exp();
                (x.sin() + 0.35 * (4.0 * x).sin() * (-t * 30.0).exp()) * d
            }
            Inst::Bass => {
                let tri = 4.0 * (p - 0.5).abs() - 1.0;
                tri * env * (-t * 3.0).exp().max(0.35)
            }
            Inst::Bell => {
                let d = (-t * 2.2).exp();
                (x.sin() + 0.5 * (2.76 * x).sin() * (-t * 4.0).exp() + 0.25 * (5.4 * x).sin() * (-t * 7.0).exp()) * d * 0.6
            }
            Inst::Brass => {
                let saw = 2.0 * p - 1.0;
                let bright = (-t * 6.0).exp() * 0.5 + 0.5;
                (saw * bright + x.sin() * (1.0 - bright)) * env * 0.6
            }
            Inst::Chip => {
                let sq = if p < 0.5 { 1.0 } else { -1.0 };
                sq * env * 0.35 * (-t * 2.0).exp().max(0.5)
            }
            Inst::Pad => ((x.sin() + (x * 1.005).sin() + 0.5 * (2.0 * x).sin()) * 0.3) * env,
        };
        buf.add(s0 + k, v * vol, pan, wrap);
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Drum {
    Kick,
    Snare,
    Hat,
    Tom,
    Clap,
}

pub fn drum(buf: &mut Buf, start: f32, d: Drum, vol: f32, pan: f32, wrap: bool, seed: u32) {
    let s0 = (start * RATE) as usize;
    let mut nz = Noise(seed | 1);
    let (len, _) = match d {
        Drum::Kick => (0.3, 0),
        Drum::Snare => (0.2, 0),
        Drum::Hat => (0.05, 0),
        Drum::Tom => (0.3, 0),
        Drum::Clap => (0.15, 0),
    };
    let n = (len * RATE) as usize;
    let mut phase = 0.0f32;
    let mut lp = 0.0f32;
    for k in 0..n {
        let t = k as f32 / RATE;
        let v = match d {
            Drum::Kick => {
                let f = 45.0 + 110.0 * (-t * 25.0).exp();
                phase += f / RATE;
                (TAU * phase).sin() * (-t * 9.0).exp()
            }
            Drum::Snare => {
                phase += 190.0 / RATE;
                (nz.next() * 0.7 + (TAU * phase).sin() * 0.4 * (-t * 30.0).exp()) * (-t * 18.0).exp()
            }
            Drum::Hat => {
                let x = nz.next();
                let hp = x - lp;
                lp = x;
                hp * (-t * 70.0).exp() * 0.5
            }
            Drum::Tom => {
                let f = 90.0 + 60.0 * (-t * 10.0).exp();
                phase += f / RATE;
                (TAU * phase).sin() * (-t * 8.0).exp()
            }
            Drum::Clap => {
                let burst = if t < 0.03 { ((t * 300.0) as i32 % 2) as f32 } else { 1.0 };
                nz.next() * (-t * 25.0).exp() * burst * 0.8
            }
        };
        buf.add(s0 + k, v * vol, pan, wrap);
    }
}

// ============================================================== music

pub struct Song {
    pub bpm: f32,
    /// One chord per bar: (root note, minor?)
    pub chords: &'static [(&'static str, bool)],
    /// "NOTE beats" pairs separated by spaces; R = rest.
    pub melody: &'static str,
    pub lead: Inst,
    pub lead_vol: f32,
    pub bass: &'static str, // 16 steps per bar: r root, f fifth, o octave, . rest
    pub kick: &'static str,
    pub snare: &'static str,
    pub hat: &'static str,
    pub organ_vol: f32,
    /// Instrument for the held chords (Organ or Pad).
    pub chord_inst: Inst,
    pub bells: bool,
    pub passes: u32,
}

fn parse_melody(m: &str) -> Vec<(Option<String>, f32)> {
    let toks: Vec<&str> = m.split_whitespace().collect();
    toks.chunks(2)
        .map(|c| {
            let n = if c[0] == "R" { None } else { Some(c[0].to_string()) };
            (n, c.get(1).and_then(|b| b.parse().ok()).unwrap_or(1.0))
        })
        .collect()
}

fn semis(f: f32, s: i32) -> f32 {
    f * 2f32.powf(s as f32 / 12.0)
}

/// One note or drum hit of a song (times in seconds from the start).
#[derive(Clone, Copy)]
pub enum Ev {
    Note { t: f32, freq: f32, dur: f32, inst: Inst, vol: f32, pan: f32 },
    Drum { t: f32, drum: Drum, vol: f32, pan: f32, seed: u32 },
}

/// Every note and drum hit of the song (`passes` repetitions, later passes varied),
/// and the length of the loop in seconds.
pub fn song_events(s: &Song) -> (Vec<Ev>, f32) {
    let beat = 60.0 / s.bpm;
    let bar = beat * 4.0;
    let bars = s.chords.len();
    let pass_len = bar * bars as f32;
    let total = pass_len * s.passes as f32;
    let mel = parse_melody(s.melody);
    let step = bar / 16.0;
    let mut ev = vec![];
    let mut note = |t, freq, dur, inst, vol, pan| ev.push(Ev::Note { t, freq, dur, inst, vol, pan });
    let mut drums = vec![];
    for pass in 0..s.passes {
        let t0 = pass as f32 * pass_len;
        // Melody (an octave higher with a softer double on later passes).
        let mut t = t0;
        for (n, beats) in &mel {
            let d = beats * beat;
            if let Some(n) = n {
                let f = note_freq(n);
                note(t, f, d * 0.9, s.lead, s.lead_vol, 0.15);
                if pass % 2 == 1 {
                    note(t, f * 2.0, d * 0.8, Inst::Bell, s.lead_vol * 0.25, -0.3);
                }
            }
            t += d;
        }
        for (bi, &(root, minor)) in s.chords.iter().enumerate() {
            let tb = t0 + bi as f32 * bar;
            let rf = note_freq(&format!("{root}3"));
            // Organ chord.
            if s.organ_vol > 0.0 {
                for (k, iv) in [0, if minor { 3 } else { 4 }, 7].iter().enumerate() {
                    note(tb, semis(rf, *iv), bar * 0.95, s.chord_inst, s.organ_vol, -0.4 + k as f32 * 0.4);
                }
            }
            // Bass.
            for (i, c) in s.bass.bytes().enumerate() {
                let iv = match c {
                    b'r' => 0,
                    b'f' => 7,
                    b'o' => 12,
                    b't' => if minor { 3 } else { 4 },
                    _ => continue,
                };
                note(tb + i as f32 * step, semis(rf, iv - 12), step * 1.6, Inst::Bass, 0.55, 0.0);
            }
            // Drums.
            for (i, ((k, sn), h)) in s.kick.bytes().zip(s.snare.bytes()).zip(s.hat.bytes()).enumerate() {
                let t = tb + i as f32 * step;
                let seed = (pass * 1000 + bi as u32 * 16 + i as u32) * 7919 + 1;
                let mut drum = |drum, vol, pan, seed| drums.push(Ev::Drum { t, drum, vol, pan, seed });
                if k == b'x' {
                    drum(Drum::Kick, 0.7, 0.0, seed);
                }
                if k == b't' {
                    drum(Drum::Tom, 0.5, 0.3, seed);
                }
                if sn == b'x' {
                    drum(Drum::Snare, 0.35, 0.1, seed + 3);
                }
                if sn == b'c' {
                    drum(Drum::Clap, 0.4, -0.1, seed + 3);
                }
                if h == b'x' {
                    drum(Drum::Hat, 0.25, 0.35, seed + 5);
                }
            }
            // Spooky bells on alternating bars.
            if s.bells && bi % 2 == 0 {
                note(tb, semis(rf, 24), beat * 2.0, Inst::Bell, 0.18, 0.5);
            }
        }
    }
    ev.extend(drums);
    (ev, total)
}

/// Renders events into a buffer.
pub fn render_events(buf: &mut Buf, events: &[Ev], wrap: bool) {
    for e in events {
        match *e {
            Ev::Note { t, freq, dur, inst, vol, pan } => note(buf, t, freq, dur, inst, vol, pan, wrap),
            Ev::Drum { t, drum: d, vol, pan, seed } => drum(buf, t, d, vol, pan, wrap, seed),
        }
    }
}

/// Renders a seamless loop of the song.
pub fn render_song(s: &Song, looping: bool) -> Buf {
    let (events, total) = song_events(s);
    let mut buf = Buf::new(if looping { total } else { total + 2.0 });
    render_events(&mut buf, &events, looping);
    buf.normalize(0.8);
    buf
}

pub const TITLE: Song = Song {
    bpm: 84.0,
    chords: &[("D", true), ("Bb", false), ("G", true), ("A", false)],
    melody: "A4 1 D5 1 F5 1.5 E5 .5  D5 1 F5 1 Bb4 2  G4 1 Bb4 1 D5 1.5 C#5 .5  E5 2 A4 2",
    lead: Inst::Theremin,
    lead_vol: 0.55,
    bass: "r...............",
    kick: "................",
    snare: "................",
    hat: "................",
    organ_vol: 0.22,
    chord_inst: Inst::Organ,
    bells: true,
    passes: 2,
};

pub const RIDE: Song = Song {
    // A creepy music box over a dark pad: D harmonic minor, a slow clock-tick beat,
    // a chromatic creep in the melody and a spooky E-flat chord before each turnaround.
    bpm: 108.0,
    chords: &[("D", true), ("D", true), ("Bb", false), ("A", false), ("G", true), ("D", true), ("Eb", false), ("A", false)],
    melody: "D5 .5 F5 .5 A5 .5 G#5 .5 A5 1 F5 1  E5 .5 F5 .5 E5 .5 D5 .5 C#5 1 A4 1  \
             Bb4 .5 D5 .5 F5 .5 E5 .5 F5 1 D5 1  C#5 .5 E5 .5 G5 .5 F5 .5 E5 2  \
             G5 .5 Bb5 .5 D6 .5 C#6 .5 D6 1 Bb5 1  A5 .5 F5 .5 D5 .5 F5 .5 A5 1 R 1  \
             G5 .5 Eb5 .5 Bb4 .5 Eb5 .5 D5 1 Eb5 1  C#5 .5 D5 .5 E5 .5 G5 .5 F5 .5 E5 .5 C#5 .5 A4 .5",
    lead: Inst::Bell,
    lead_vol: 0.9,
    bass: "r.......f...t...",
    kick: "x.....t.x.......",
    snare: "............x...",
    hat: "x...x...x...x...",
    organ_vol: 0.4,
    chord_inst: Inst::Pad,
    bells: true,
    passes: 2,
};

pub const BOSS: Song = Song {
    bpm: 156.0,
    chords: &[("D", true), ("D", true), ("Eb", false), ("Eb", false), ("D", true), ("D", true), ("A", false), ("A", false)],
    melody: "D5 .5 D5 .5 F5 .5 D5 .5 A5 1 G#5 1  A5 .5 F5 .5 D5 .5 F5 .5 E5 1 D5 1  \
             Eb5 .5 Eb5 .5 G5 .5 Eb5 .5 Bb5 1 A5 1  G5 .5 Eb5 .5 Bb4 .5 Eb5 .5 D5 2  \
             D6 .5 C6 .5 A5 .5 F5 .5 D6 .5 C6 .5 A5 1  F5 .5 A5 .5 D6 .5 A5 .5 F5 1 D5 1  \
             E5 .5 A5 .5 C#6 .5 A5 .5 E6 1 C#6 1  A5 .5 G5 .5 F5 .5 E5 .5 C#5 1 A4 1",
    lead: Inst::Brass,
    lead_vol: 0.6,
    bass: "r.r.r.r.o.r.r.f.",
    kick: "x...x...x...x.t.",
    snare: "....x.......x...",
    hat: "x.x.x.x.x.x.x.x.",
    organ_vol: 0.12,
    chord_inst: Inst::Organ,
    bells: false,
    passes: 2,
};

pub const RESULTS: Song = Song {
    bpm: 120.0,
    chords: &[("D", false), ("G", false), ("A", false), ("D", false)],
    melody: "F#5 .5 A5 .5 D6 1 A5 .5 F#5 .5 D5 1  G5 .5 B5 .5 D6 1 B5 .5 G5 .5 B5 1  \
             A5 .5 C#6 .5 E6 1 C#6 .5 A5 .5 E5 1  D6 .5 A5 .5 F#5 .5 A5 .5 D6 2",
    lead: Inst::Chip,
    lead_vol: 0.6,
    bass: "r...t...f...t...",
    kick: "x.......x.......",
    snare: "....c.......c...",
    hat: "..x...x...x...x.",
    organ_vol: 0.0,
    chord_inst: Inst::Organ,
    bells: true,
    passes: 2,
};

/// The victory fanfare's notes and hits.
pub fn victory_events() -> Vec<Ev> {
    let mut ev = vec![];
    let seq = [("D5", 0.0, 0.12), ("F#5", 0.12, 0.12), ("A5", 0.24, 0.12), ("D6", 0.36, 0.4), ("A5", 0.9, 0.15), ("D6", 1.05, 1.2)];
    for (n, t, d) in seq {
        ev.push(Ev::Note { t, freq: note_freq(n), dur: d, inst: Inst::Brass, vol: 0.7, pan: 0.0 });
        ev.push(Ev::Note { t, freq: note_freq(n) * 2.0, dur: d, inst: Inst::Bell, vol: 0.3, pan: 0.3 });
    }
    for n in ["D4", "F#4", "A4"] {
        ev.push(Ev::Note { t: 1.05, freq: note_freq(n), dur: 1.5, inst: Inst::Organ, vol: 0.3, pan: -0.3 });
    }
    ev.push(Ev::Drum { t: 1.05, drum: Drum::Kick, vol: 0.8, pan: 0.0, seed: 3 });
    ev.push(Ev::Drum { t: 1.05, drum: Drum::Clap, vol: 0.6, pan: 0.0, seed: 5 });
    ev
}

pub fn victory() -> Buf {
    let mut b = Buf::new(3.5);
    render_events(&mut b, &victory_events(), false);
    b.normalize(0.85);
    b
}

// ============================================================== sound effects

fn sweep(len: f32, f0: f32, f1: f32, decay: f32, shape: impl Fn(f32) -> f32) -> Buf {
    let mut b = Buf::new(len);
    let n = b.frames();
    let mut ph = 0.0f32;
    for k in 0..n {
        let t = k as f32 / RATE;
        let u = t / len;
        let f = f0 * (f1 / f0).powf(u);
        ph += f / RATE;
        let v = shape(ph) * (-t * decay).exp() * (t * 400.0).min(1.0);
        b.data[k * 2] = v;
        b.data[k * 2 + 1] = v;
    }
    b
}

fn sine(p: f32) -> f32 {
    (TAU * p).sin()
}
fn square(p: f32) -> f32 {
    if p.fract() < 0.5 { 0.6 } else { -0.6 }
}

fn mix_into(dst: &mut Buf, src: &Buf, at: f32, vol: f32) {
    let o = (at * RATE) as usize * 2;
    for (i, s) in src.data.iter().enumerate() {
        if o + i < dst.data.len() {
            dst.data[o + i] += s * vol;
        }
    }
}

pub fn sfx_pop() -> Buf {
    let mut b = sweep(0.18, 380.0, 1300.0, 22.0, sine);
    let sparkle = sweep(0.25, 2400.0, 3200.0, 18.0, sine);
    mix_into(&mut b, &sparkle, 0.05, 0.25);
    b.normalize(0.8);
    b
}

pub fn sfx_shot() -> Buf {
    let mut b = sweep(0.07, 1400.0, 700.0, 50.0, square);
    b.normalize(0.5);
    b
}

pub fn sfx_bonus() -> Buf {
    let mut b = Buf::new(0.7);
    for (i, n) in ["C6", "E6", "G6", "C7"].iter().enumerate() {
        note(&mut b, i as f32 * 0.07, note_freq(n), 0.1, Inst::Bell, 0.7, 0.0, false);
    }
    b.normalize(0.8);
    b
}

pub fn sfx_poof() -> Buf {
    let mut b = Buf::new(0.3);
    let mut nz = Noise(77);
    let mut lp = 0.0;
    for k in 0..b.frames() {
        let t = k as f32 / RATE;
        lp += (nz.next() - lp) * 0.08;
        let v = lp * (-t * 14.0).exp() * 3.0;
        b.data[k * 2] = v;
        b.data[k * 2 + 1] = v;
    }
    b.normalize(0.6);
    b
}

pub fn sfx_ghost() -> Buf {
    // "Woooo!"
    let len = 1.1;
    let mut b = Buf::new(len);
    let mut ph = 0.0;
    for k in 0..b.frames() {
        let t = k as f32 / RATE;
        let f = 420.0 + 220.0 * (t / len * std::f32::consts::PI).sin() + 12.0 * (TAU * 6.0 * t).sin();
        ph += f / RATE;
        let env = (t * 6.0).min(1.0) * (1.0 - t / len).max(0.0);
        let v = (sine(ph) + 0.2 * sine(ph * 2.0)) * env;
        b.data[k * 2] = v;
        b.data[k * 2 + 1] = v;
    }
    b.normalize(0.5);
    b
}

pub fn sfx_squeak() -> Buf {
    let mut b = Buf::new(0.3);
    for i in 0..3 {
        let c = sweep(0.05, 3200.0, 4200.0, 30.0, sine);
        mix_into(&mut b, &c, i as f32 * 0.08, 1.0);
    }
    b.normalize(0.45);
    b
}

pub fn sfx_rattle() -> Buf {
    let mut b = Buf::new(0.5);
    let mut nz = Noise(5);
    for i in 0..7 {
        let at = i as f32 * 0.06 + if i % 2 == 0 { 0.0 } else { 0.015 };
        let s0 = (at * RATE) as usize;
        let f = 900.0 + (i % 3) as f32 * 300.0;
        for k in 0..(0.035 * RATE) as usize {
            let t = k as f32 / RATE;
            let v = ((TAU * f * t).sin() * 0.6 + nz.next() * 0.4) * (-t * 90.0).exp();
            if (s0 + k) * 2 + 1 < b.data.len() {
                b.data[(s0 + k) * 2] += v;
                b.data[(s0 + k) * 2 + 1] += v;
            }
        }
    }
    b.normalize(0.5);
    b
}

/// The haunted painting: the pumpkin's boing, but deeper (about 7 semitones down), with a
/// slower wobble and a longer tail.
pub fn sfx_boing_deep() -> Buf {
    let len = 0.65;
    let mut b = Buf::new(len);
    let mut ph = 0.0;
    for k in 0..b.frames() {
        let t = k as f32 / RATE;
        let f = 147.0 * (1.0 + 0.5 * (TAU * 10.0 * t).sin() * (-t * 4.0).exp()) * (1.0 + 0.7 * t);
        ph += f / RATE;
        // A touch of the second harmonic keeps it audible on small speakers.
        let v = (sine(ph) + 0.25 * sine(ph * 2.0)) * (-t * 4.5).exp();
        b.data[k * 2] = v;
        b.data[k * 2 + 1] = v;
    }
    b.normalize(0.55);
    b
}

pub fn sfx_zip() -> Buf {
    let mut b = sweep(0.35, 1800.0, 300.0, 5.0, square);
    b.normalize(0.35);
    b
}

pub fn sfx_boing() -> Buf {
    let mut b = Buf::new(0.4);
    let mut ph = 0.0;
    for k in 0..b.frames() {
        let t = k as f32 / RATE;
        let f = 220.0 * (1.0 + 0.5 * (TAU * 14.0 * t).sin() * (-t * 6.0).exp()) * (1.0 + t);
        ph += f / RATE;
        let v = sine(ph) * (-t * 7.0).exp();
        b.data[k * 2] = v;
        b.data[k * 2 + 1] = v;
    }
    b.normalize(0.5);
    b
}

pub fn sfx_flutter() -> Buf {
    let mut b = Buf::new(0.4);
    let mut nz = Noise(9);
    for k in 0..b.frames() {
        let t = k as f32 / RATE;
        let flap = (0.5 + 0.5 * (TAU * 18.0 * t).sin()).powi(3);
        let v = nz.next() * flap * (1.0 - t / 0.4) * 0.6;
        b.data[k * 2] = v;
        b.data[k * 2 + 1] = v;
    }
    b.normalize(0.35);
    b
}

pub fn sfx_beep(high: bool) -> Buf {
    let f = if high { 1320.0 } else { 660.0 };
    let len = if high { 0.45 } else { 0.18 };
    let mut b = Buf::new(len + 0.05);
    note(&mut b, 0.0, f, len, Inst::Chip, 0.8, 0.0, false);
    b.normalize(0.6);
    b
}

pub fn sfx_join() -> Buf {
    let mut b = Buf::new(0.5);
    for (i, n) in ["G5", "D6"].iter().enumerate() {
        note(&mut b, i as f32 * 0.09, note_freq(n), 0.12, Inst::Xylo, 0.8, 0.0, false);
    }
    b.normalize(0.6);
    b
}

pub fn sfx_whoosh() -> Buf {
    let len = 0.8;
    let mut b = Buf::new(len);
    let mut nz = Noise(31);
    let mut lp = 0.0;
    for k in 0..b.frames() {
        let t = k as f32 / RATE;
        let u = t / len;
        let a = 0.02 + 0.2 * (u * std::f32::consts::PI).sin();
        lp += (nz.next() - lp) * a;
        let v = lp * (u * std::f32::consts::PI).sin() * 2.0;
        let pan = (u * 2.0 - 1.0) * 0.8;
        b.data[k * 2] = v * (1.0 - pan) * 0.5;
        b.data[k * 2 + 1] = v * (1.0 + pan) * 0.5;
    }
    b.normalize(0.45);
    b
}

pub fn sfx_bonk() -> Buf {
    let mut b = sweep(0.22, 260.0, 110.0, 14.0, |p| sine(p) + 0.3 * square(p * 2.0));
    b.normalize(0.65);
    b
}

pub fn sfx_laugh() -> Buf {
    // "MWA-HA-HA-HA!" - formant-ish bursts with falling pitch.
    let mut b = Buf::new(1.6);
    let mut nz = Noise(13);
    let syl = [(0.0, 150.0, 0.26), (0.32, 175.0, 0.16), (0.55, 160.0, 0.16), (0.78, 145.0, 0.16), (1.01, 125.0, 0.4)];
    for (at, f0, len) in syl {
        let s0 = (at * RATE) as usize;
        let mut ph = 0.0f32;
        let (mut bp1, mut bp2) = (0.0f32, 0.0f32);
        for k in 0..(len * RATE) as usize {
            let t = k as f32 / RATE;
            let f = f0 * (1.0 - 0.25 * t / len);
            ph += f / RATE;
            let saw = 2.0 * ph.fract() - 1.0 + nz.next() * 0.1;
            // Crude "ah" vowel: two resonant one-pole smoothers.
            bp1 += (saw - bp1) * 0.18;
            bp2 += (bp1 - bp2) * 0.35;
            let env = (t * 60.0).min(1.0) * (1.0 - t / len).max(0.0).powf(0.6);
            let v = (bp1 - bp2 * 0.6) * env;
            if (s0 + k) * 2 + 1 < b.data.len() {
                b.data[(s0 + k) * 2] += v;
                b.data[(s0 + k) * 2 + 1] += v;
            }
        }
    }
    let mut out = b.clone();
    // A little echo for spookiness.
    let d = (0.12 * RATE) as usize * 2;
    for i in d..out.data.len() {
        out.data[i] += b.data[i - d] * 0.35;
    }
    out.normalize(0.8);
    out
}

pub fn sfx_rumble() -> Buf {
    let mut b = Buf::new(2.0);
    let mut nz = Noise(41);
    let mut lp = 0.0;
    for k in 0..b.frames() {
        let t = k as f32 / RATE;
        lp += (nz.next() - lp) * 0.01;
        let env = (t * 2.0).min(1.0) * (1.0 - t / 2.0);
        let v = (lp * 6.0 + (TAU * 45.0 * t).sin() * 0.3) * env;
        b.data[k * 2] = v;
        b.data[k * 2 + 1] = v;
    }
    b.normalize(0.7);
    b
}

pub fn sfx_boom() -> Buf {
    let mut b = Buf::new(1.5);
    let mut nz = Noise(43);
    let mut lp = 0.0;
    let mut ph = 0.0;
    for k in 0..b.frames() {
        let t = k as f32 / RATE;
        lp += (nz.next() - lp) * (0.3 * (-t * 3.0).exp() + 0.02);
        ph += (60.0 + 100.0 * (-t * 8.0).exp()) / RATE;
        let v = (lp * 2.0 + sine(ph)) * (-t * 3.0).exp();
        b.data[k * 2] = v;
        b.data[k * 2 + 1] = v;
    }
    let bonus = sfx_bonus();
    mix_into(&mut b, &bonus, 0.1, 0.5);
    b.normalize(0.85);
    b
}

pub fn sfx_ding() -> Buf {
    let mut b = Buf::new(1.0);
    note(&mut b, 0.0, note_freq("A5"), 0.2, Inst::Bell, 0.8, 0.0, false);
    b.normalize(0.5);
    b
}

/// One second of clickety-clack for the cart (looped, volume follows speed).
pub fn cart_loop() -> Buf {
    let mut b = Buf::new(1.0);
    let mut nz = Noise(99);
    for &(at, f) in &[(0.0, 160.0), (0.09, 140.0), (0.5, 160.0), (0.59, 140.0)] {
        let s0 = (at * RATE) as usize;
        for k in 0..(0.06 * RATE) as usize {
            let t = k as f32 / RATE;
            let v = ((TAU * f * t).sin() * 0.7 + nz.next() * 0.3) * (-t * 50.0).exp();
            b.data[(s0 + k) * 2] += v;
            b.data[(s0 + k) * 2 + 1] += v;
        }
    }
    let mut lp = 0.0;
    for k in 0..b.frames() {
        lp += (nz.next() - lp) * 0.02;
        b.data[k * 2] += lp * 0.4;
        b.data[k * 2 + 1] += lp * 0.4;
    }
    b.normalize(0.5);
    b
}

pub fn write_wav(path: &str, b: &Buf) -> std::io::Result<()> {
    let n = b.data.len();
    let mut out = Vec::with_capacity(44 + n * 2);
    let byte_rate = RATE as u32 * 4;
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + n as u32 * 2).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&(RATE as u32).to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(n as u32 * 2).to_le_bytes());
    for s in &b.data {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, out)
}
