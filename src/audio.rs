//! Audio engine: a real-time mixer thread that plays the pre-rendered music loops
//! and sound effects. Output goes through ALSA, loaded at runtime with dlopen
//! (libasound.so.2), so there is no build-time dependency on ALSA headers and the
//! game still runs silently if no sound device is available. On PipeWire or
//! PulseAudio systems the ALSA "default" device routes to them automatically.

use crate::synth::{self, Buf, RATE};
use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void, CString};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sfx {
    Pop,
    Shot,
    Bonus,
    Poof,
    Ghost,
    Squeak,
    Rattle,
    Zip,
    Boing,
    Flutter,
    Beep,
    Go,
    Join,
    Whoosh,
    Bonk,
    Laugh,
    Rumble,
    Boom,
    Ding,
}
const SFX_COUNT: usize = 19;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Music {
    Title,
    Ride,
    Boss,
    Victory,
    Results,
}

#[derive(Clone, Copy, Debug)]
pub struct SoundEvent {
    pub sfx: Sfx,
    pub vol: f32,
    pub pan: f32,
    pub pitch: f32,
}

enum Cmd {
    Play(SoundEvent),
    Music(Option<Music>),
    Cart(f32),
}

pub struct Bank {
    sfx: Vec<Buf>,
    title: Buf,
    ride: Buf,
    boss: Buf,
    victory: Buf,
    results: Buf,
    cart: Buf,
}

impl Bank {
    pub fn build() -> Bank {
        let sfx: Vec<Buf> = vec![
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
        ];
        assert_eq!(sfx.len(), SFX_COUNT);
        Bank {
            sfx,
            title: synth::render_song(&synth::TITLE, true),
            ride: synth::render_song(&synth::RIDE, true),
            boss: synth::render_song(&synth::BOSS, true),
            victory: synth::victory(),
            results: synth::render_song(&synth::RESULTS, true),
            cart: synth::cart_loop(),
        }
    }

    fn music(&self, m: Music) -> (&Buf, bool) {
        match m {
            Music::Title => (&self.title, true),
            Music::Ride => (&self.ride, true),
            Music::Boss => (&self.boss, true),
            Music::Victory => (&self.victory, false),
            Music::Results => (&self.results, true),
        }
    }

    /// Writes every music loop and effect as WAV files (for listening / testing).
    pub fn dump(&self, dir: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        for (name, b) in [
            ("music_title", &self.title),
            ("music_ride", &self.ride),
            ("music_boss", &self.boss),
            ("music_victory", &self.victory),
            ("music_results", &self.results),
            ("cart_loop", &self.cart),
        ] {
            synth::write_wav(&format!("{dir}/{name}.wav"), b)?;
        }
        let names = [
            "pop", "shot", "bonus", "poof", "ghost", "squeak", "rattle", "zip", "boing", "flutter", "beep", "go", "join",
            "whoosh", "bonk", "laugh", "rumble", "boom", "ding",
        ];
        for (i, b) in self.sfx.iter().enumerate() {
            synth::write_wav(&format!("{dir}/sfx_{}.wav", names[i]), b)?;
        }
        Ok(())
    }
}

struct Voice {
    sfx: usize,
    pos: f32,
    rate: f32,
    vol: f32,
    pan: f32,
}

struct MusicVoice {
    which: Music,
    pos: usize,
    gain: f32,
    target: f32,
}

/// The mixer state (also usable offline for tests).
pub struct Mixer {
    bank: Bank,
    voices: Vec<Voice>,
    music: Vec<MusicVoice>,
    cart_pos: usize,
    cart_vol: f32,
    cart_target: f32,
    master: f32,
    requested: Option<Music>,
}

impl Mixer {
    pub fn new(bank: Bank) -> Mixer {
        Mixer { bank, voices: vec![], music: vec![], cart_pos: 0, cart_vol: 0.0, cart_target: 0.0, master: 0.9, requested: None }
    }

    fn command(&mut self, c: Cmd) {
        match c {
            Cmd::Play(e) => {
                if self.voices.len() >= 40 {
                    self.voices.remove(0);
                }
                self.voices.push(Voice { sfx: e.sfx as usize, pos: 0.0, rate: e.pitch.max(0.1), vol: e.vol, pan: e.pan });
            }
            Cmd::Music(m) => {
                // Only react to changes, so one-shot jingles don't restart when they end.
                if self.requested == m {
                    return;
                }
                self.requested = m;
                for v in self.music.iter_mut() {
                    v.target = 0.0;
                }
                if let Some(m) = m {
                    self.music.push(MusicVoice { which: m, pos: 0, gain: 0.0, target: 1.0 });
                }
            }
            Cmd::Cart(v) => self.cart_target = v.clamp(0.0, 1.0),
        }
    }

    pub fn render(&mut self, out: &mut [f32]) {
        out.iter_mut().for_each(|s| *s = 0.0);
        let frames = out.len() / 2;
        // Music with crossfades.
        let fade = 1.0 / (RATE * 0.8);
        let music_vol = 0.42;
        for mv in self.music.iter_mut() {
            let (buf, looping) = self.bank.music(mv.which);
            let n = buf.frames();
            for f in 0..frames {
                if mv.gain < mv.target {
                    mv.gain = (mv.gain + fade).min(mv.target);
                } else if mv.gain > mv.target {
                    mv.gain = (mv.gain - fade).max(mv.target);
                }
                if mv.pos >= n {
                    if looping {
                        mv.pos = 0;
                    } else {
                        break;
                    }
                }
                let g = mv.gain * music_vol;
                out[f * 2] += buf.data[mv.pos * 2] * g;
                out[f * 2 + 1] += buf.data[mv.pos * 2 + 1] * g;
                mv.pos += 1;
            }
        }
        self.music.retain(|m| m.gain > 0.0 || m.target > 0.0);
        self.music.retain(|m| {
            let (b, looping) = (self.bank.music(m.which).0.frames(), self.bank.music(m.which).1);
            looping || m.pos < b
        });
        // Cart rattle.
        let cart = &self.bank.cart;
        for f in 0..frames {
            self.cart_vol += (self.cart_target - self.cart_vol) * 0.0005;
            let v = self.cart_vol * 0.42; // 50% louder rails
            out[f * 2] += cart.data[self.cart_pos * 2] * v;
            out[f * 2 + 1] += cart.data[self.cart_pos * 2 + 1] * v;
            self.cart_pos = (self.cart_pos + 1) % cart.frames();
        }
        // Effects (with resampling for pitch variety).
        for v in self.voices.iter_mut() {
            let b = &self.bank.sfx[v.sfx];
            let n = b.frames();
            let pan = v.pan.clamp(-1.0, 1.0);
            let (gl, gr) = ((1.0 - pan * 0.7) * v.vol, (1.0 + pan * 0.7) * v.vol);
            for f in 0..frames {
                let i = v.pos as usize;
                if i + 1 >= n {
                    v.pos = n as f32;
                    break;
                }
                let fr = v.pos - i as f32;
                let l = b.data[i * 2] * (1.0 - fr) + b.data[i * 2 + 2] * fr;
                let r = b.data[i * 2 + 1] * (1.0 - fr) + b.data[i * 2 + 3] * fr;
                out[f * 2] += l * gl;
                out[f * 2 + 1] += r * gr;
                v.pos += v.rate;
            }
        }
        self.voices.retain(|v| (v.pos as usize) + 1 < self.bank.sfx[v.sfx].frames());
        // Soft clip.
        for s in out.iter_mut() {
            *s = (*s * self.master).tanh();
        }
    }
}

pub struct Audio {
    tx: Option<Sender<Cmd>>,
}

impl Audio {
    pub fn silent() -> Audio {
        Audio { tx: None }
    }

    /// Starts the audio thread (sounds are synthesized there, then played through
    /// ALSA). Returns a silent engine if no sound device is usable.
    pub fn start() -> Audio {
        let (tx, rx) = channel::<Cmd>();
        let (ok_tx, ok_rx) = channel::<Result<(), String>>();
        thread::Builder::new()
            .name("audio".into())
            .spawn(move || {
                let alsa = match Alsa::open() {
                    Ok(a) => {
                        let _ = ok_tx.send(Ok(()));
                        a
                    }
                    Err(e) => {
                        let _ = ok_tx.send(Err(e));
                        return;
                    }
                };
                let t = std::time::Instant::now();
                let bank = Bank::build();
                eprintln!("audio: synthesized music and effects in {:.2}s", t.elapsed().as_secs_f32());
                run(alsa, Mixer::new(bank), rx);
            })
            .ok();
        match ok_rx.recv() {
            Ok(Ok(())) => {
                eprintln!("audio: ALSA output started");
                Audio { tx: Some(tx) }
            }
            Ok(Err(e)) => {
                eprintln!("audio: disabled ({e})");
                Audio { tx: None }
            }
            Err(_) => Audio { tx: None },
        }
    }

    pub fn play(&self, e: SoundEvent) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Cmd::Play(e));
        }
    }
    pub fn music(&self, m: Option<Music>) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Cmd::Music(m));
        }
    }
    pub fn cart(&self, v: f32) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Cmd::Cart(v));
        }
    }
}

fn run(alsa: Alsa, mut mixer: Mixer, rx: Receiver<Cmd>) {
    let frames = 512;
    let mut fbuf = vec![0.0f32; frames * 2];
    let mut ibuf = vec![0i16; frames * 2];
    loop {
        loop {
            match rx.try_recv() {
                Ok(c) => mixer.command(c),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
            }
        }
        mixer.render(&mut fbuf);
        for (d, s) in ibuf.iter_mut().zip(fbuf.iter()) {
            *d = (s * 32767.0) as i16;
        }
        if !alsa.write(&ibuf, frames) {
            return;
        }
    }
}

// ------------------------------------------------------------------ ALSA via dlopen

type PcmOpen = unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int;
type PcmSetParams = unsafe extern "C" fn(*mut c_void, c_int, c_int, c_uint, c_uint, c_int, c_uint) -> c_int;
type PcmWritei = unsafe extern "C" fn(*mut c_void, *const c_void, c_ulong) -> c_long;
type PcmRecover = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int;
type PcmClose = unsafe extern "C" fn(*mut c_void) -> c_int;

struct Alsa {
    _lib: libloading::Library,
    pcm: *mut c_void,
    writei: PcmWritei,
    recover: PcmRecover,
    close: PcmClose,
}

unsafe impl Send for Alsa {}

impl Alsa {
    fn open() -> Result<Alsa, String> {
        unsafe {
            let lib = libloading::Library::new("libasound.so.2")
                .or_else(|_| libloading::Library::new("libasound.so"))
                .map_err(|e| format!("libasound not found: {e}"))?;
            let open: PcmOpen = *lib.get::<PcmOpen>(b"snd_pcm_open\0").map_err(|e| e.to_string())?;
            let set_params: PcmSetParams = *lib.get::<PcmSetParams>(b"snd_pcm_set_params\0").map_err(|e| e.to_string())?;
            let writei: PcmWritei = *lib.get::<PcmWritei>(b"snd_pcm_writei\0").map_err(|e| e.to_string())?;
            let recover: PcmRecover = *lib.get::<PcmRecover>(b"snd_pcm_recover\0").map_err(|e| e.to_string())?;
            let close: PcmClose = *lib.get::<PcmClose>(b"snd_pcm_close\0").map_err(|e| e.to_string())?;
            let dev = std::env::var("HAUNTED_ALSA_DEVICE").unwrap_or_else(|_| "default".into());
            let name = CString::new(dev).unwrap();
            let mut pcm: *mut c_void = std::ptr::null_mut();
            // SND_PCM_STREAM_PLAYBACK = 0
            let r = open(&mut pcm, name.as_ptr(), 0, 0);
            if r < 0 || pcm.is_null() {
                return Err(format!("snd_pcm_open failed ({r})"));
            }
            // SND_PCM_FORMAT_S16_LE = 2, SND_PCM_ACCESS_RW_INTERLEAVED = 3, 2 channels, 60 ms latency
            let r = set_params(pcm, 2, 3, 2, RATE as c_uint, 1, 60_000);
            if r < 0 {
                close(pcm);
                return Err(format!("snd_pcm_set_params failed ({r})"));
            }
            Ok(Alsa { _lib: lib, pcm, writei, recover, close })
        }
    }

    fn write(&self, data: &[i16], frames: usize) -> bool {
        let mut off = 0;
        while off < frames {
            let r = unsafe { (self.writei)(self.pcm, data[off * 2..].as_ptr() as *const c_void, (frames - off) as c_ulong) };
            if r < 0 {
                let rr = unsafe { (self.recover)(self.pcm, r as c_int, 1) };
                if rr < 0 {
                    eprintln!("audio: write failed ({r}), stopping sound");
                    return false;
                }
            } else {
                off += r as usize;
            }
        }
        true
    }
}

impl Drop for Alsa {
    fn drop(&mut self) {
        unsafe { (self.close)(self.pcm) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixer_produces_sound_without_clipping() {
        let mut m = Mixer::new(Bank::build());
        m.command(Cmd::Music(Some(Music::Ride)));
        m.command(Cmd::Play(SoundEvent { sfx: Sfx::Pop, vol: 1.0, pan: 0.5, pitch: 1.2 }));
        let mut buf = vec![0.0; 44100 * 2];
        let mut peak = 0.0f32;
        let mut energy = 0.0f32;
        for _ in 0..4 {
            m.render(&mut buf);
            for s in &buf {
                peak = peak.max(s.abs());
                energy += s * s;
            }
        }
        assert!(peak <= 1.0 && peak > 0.05, "peak {peak}");
        assert!(energy > 100.0);
    }
}
