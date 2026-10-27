//! Wii Remote support through the Linux `hid-wiimote` kernel driver (evdev).
//!
//! Each paired remote shows up as several input devices that share one HID parent:
//!   "Nintendo Wii Remote"     - buttons (+ rumble via force feedback)
//!   "Nintendo Wii Remote IR"  - up to 4 IR dots from the sensor bar (ABS_HAT0X..HAT3Y)
//! A background thread hot-plugs new remotes; one reader thread per device node
//! updates shared state. The game polls `WiimoteManager::poll()` each frame.

use evdev::{Device, EventType, FFEffectData, FFEffectKind, FFReplay, FFTrigger};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// hid-wiimote key codes.
const BTN_A: u16 = 0x130;
const BTN_B: u16 = 0x131;
const BTN_1: u16 = 0x101;
const BTN_2: u16 = 0x102;
const BTN_MODE: u16 = 0x13c; // Home
const KEY_NEXT: u16 = 0x197; // Plus
const KEY_PREVIOUS: u16 = 0x19c; // Minus
const ABS_HAT0X: u16 = 0x10;

pub const B_A: u32 = 1;
pub const B_B: u32 = 2;
pub const B_PLUS: u32 = 4;
pub const B_MINUS: u32 = 8;
pub const B_HOME: u32 = 16;
pub const B_1: u32 = 32;
pub const B_2: u32 = 64;

#[derive(Clone, Default)]
struct Shared {
    connected: bool,
    buttons: u32,
    pressed: u32, // edges since last poll
    dots: [(i32, i32); 4],
    rumble: Option<Sender<u32>>,
    hid_dir: PathBuf,
}

/// Per-remote snapshot handed to the game.
#[derive(Clone, Debug)]
pub struct RemoteState {
    pub id: u32,
    pub buttons: u32,
    pub pressed: u32,
    /// Pointer position in 0..1 screen coordinates, if the sensor bar is visible.
    pub aim: Option<(f32, f32)>,
    pub raw_dots: Vec<(i32, i32)>,
}

#[derive(Clone)]
pub struct IrConfig {
    pub flip_x: bool,
    pub flip_y: bool,
    pub gain: f32,
    pub y_offset: f32,
}

pub struct WiimoteManager {
    remotes: Arc<Mutex<HashMap<u32, Shared>>>,
    half_sep: HashMap<u32, (f32, f32)>,
    last_mid: HashMap<u32, (f32, f32)>,
    pub cfg: IrConfig,
}

impl WiimoteManager {
    pub fn start(cfg: IrConfig) -> WiimoteManager {
        let remotes: Arc<Mutex<HashMap<u32, Shared>>> = Arc::new(Mutex::new(HashMap::new()));
        let r2 = remotes.clone();
        thread::Builder::new()
            .name("wiimote-scan".into())
            .spawn(move || scan_loop(r2))
            .ok();
        WiimoteManager { remotes, half_sep: HashMap::new(), last_mid: HashMap::new(), cfg }
    }

    pub fn poll(&mut self) -> Vec<RemoteState> {
        let mut snap = vec![];
        {
            let mut map = self.remotes.lock().unwrap();
            for (&id, s) in map.iter_mut() {
                if !s.connected {
                    continue;
                }
                let dots: Vec<(i32, i32)> = s.dots.iter().copied().filter(|&(x, y)| x < 1023 && y < 1023 && x >= 0).collect();
                snap.push((id, s.buttons, s.pressed, dots));
                s.pressed = 0;
            }
        }
        let mut out = vec![];
        for (id, buttons, pressed, dots) in snap {
            let aim = self.compute_aim(id, &dots);
            out.push(RemoteState { id, buttons, pressed, aim, raw_dots: dots });
        }
        out.sort_by_key(|r| r.id);
        out
    }

    fn compute_aim(&mut self, id: u32, dots: &[(i32, i32)]) -> Option<(f32, f32)> {
        let mid = if dots.len() >= 2 {
            let (a, b) = (dots[0], dots[1]);
            let m = ((a.0 + b.0) as f32 * 0.5, (a.1 + b.1) as f32 * 0.5);
            // Remember the half separation (left dot -> midpoint) for single-dot fallback.
            let (l, r) = if a.0 < b.0 { (a, b) } else { (b, a) };
            self.half_sep.insert(id, ((r.0 - l.0) as f32 * 0.5, (r.1 - l.1) as f32 * 0.5));
            m
        } else if dots.len() == 1 {
            let d = (dots[0].0 as f32, dots[0].1 as f32);
            let hs = *self.half_sep.get(&id)?;
            let last = *self.last_mid.get(&id)?;
            let c1 = (d.0 + hs.0, d.1 + hs.1);
            let c2 = (d.0 - hs.0, d.1 - hs.1);
            let d1 = (c1.0 - last.0).powi(2) + (c1.1 - last.1).powi(2);
            let d2 = (c2.0 - last.0).powi(2) + (c2.1 - last.1).powi(2);
            if d1 < d2 { c1 } else { c2 }
        } else {
            return None;
        };
        self.last_mid.insert(id, mid);
        Some(ir_to_screen(mid.0, mid.1, &self.cfg))
    }

    pub fn rumble(&self, id: u32, ms: u32) {
        if let Some(s) = self.remotes.lock().unwrap().get(&id) {
            if let Some(tx) = &s.rumble {
                let _ = tx.send(ms);
            }
        }
    }

    /// Lights player LED `slot` (0..3) on a remote (best effort; needs write access).
    pub fn set_led(&self, id: u32, slot: usize) {
        let dir = match self.remotes.lock().unwrap().get(&id) {
            Some(s) => s.hid_dir.join("leds"),
            None => return,
        };
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                for k in 0..4 {
                    if name.ends_with(&format!(":blue:p{k}")) {
                        let _ = std::fs::write(e.path().join("brightness"), if k == slot { "1" } else { "0" });
                    }
                }
            }
        }
    }
}

/// Maps the IR-camera midpoint (0..1023, 0..767) to 0..1 screen coordinates.
/// The camera sees the sensor bar move opposite to where the remote points.
pub fn ir_to_screen(mx: f32, my: f32, cfg: &IrConfig) -> (f32, f32) {
    let nx = mx / 1023.0 - 0.5;
    let ny = my / 767.0 - 0.5;
    let sx = if cfg.flip_x { -nx } else { nx };
    let sy = if cfg.flip_y { -ny } else { ny };
    (
        (0.5 + sx * cfg.gain).clamp(0.0, 1.0),
        (0.5 + sy * cfg.gain + cfg.y_offset).clamp(0.0, 1.0),
    )
}

fn hid_parent(event_path: &Path) -> Option<PathBuf> {
    let name = event_path.file_name()?;
    let sys = Path::new("/sys/class/input").join(name).join("device");
    let input_dir = std::fs::canonicalize(sys).ok()?;
    Some(input_dir.parent()?.to_path_buf())
}

fn scan_loop(remotes: Arc<Mutex<HashMap<u32, Shared>>>) {
    let mut opened: HashSet<PathBuf> = HashSet::new();
    let mut ids: HashMap<PathBuf, u32> = HashMap::new();
    let mut next_id = 0u32;
    loop {
        for (path, dev) in evdev::enumerate() {
            if opened.contains(&path) {
                continue;
            }
            let name = dev.name().unwrap_or("").to_string();
            let is_buttons = name == "Nintendo Wii Remote";
            let is_ir = name == "Nintendo Wii Remote IR";
            if !is_buttons && !is_ir {
                continue;
            }
            let Some(hid) = hid_parent(&path) else { continue };
            let id = *ids.entry(hid.clone()).or_insert_with(|| {
                next_id += 1;
                next_id - 1
            });
            opened.insert(path.clone());
            {
                let mut map = remotes.lock().unwrap();
                let s = map.entry(id).or_default();
                s.connected = true;
                s.hid_dir = hid.clone();
                s.dots = [(1023, 1023); 4];
                if is_buttons {
                    s.rumble = start_rumble(&path);
                }
            }
            eprintln!("[wiimote] remote #{id}: {name} at {}", path.display());
            let r = remotes.clone();
            let p2 = path.clone();
            thread::Builder::new()
                .name(format!("wiimote-{id}"))
                .spawn(move || {
                    read_loop(dev, id, is_ir, r.clone());
                    eprintln!("[wiimote] remote #{id} disconnected ({})", p2.display());
                    if let Some(s) = r.lock().unwrap().get_mut(&id) {
                        s.connected = false;
                        s.buttons = 0;
                    }
                })
                .ok();
        }
        // Forget nodes that disappeared so a re-paired remote is picked up again.
        opened.retain(|p| p.exists());
        thread::sleep(Duration::from_millis(1500));
    }
}

fn read_loop(mut dev: Device, id: u32, is_ir: bool, remotes: Arc<Mutex<HashMap<u32, Shared>>>) {
    loop {
        let events: Vec<_> = match dev.fetch_events() {
            Ok(evs) => evs.collect(),
            Err(_) => return,
        };
        let mut map = remotes.lock().unwrap();
        let Some(s) = map.get_mut(&id) else { return };
        s.connected = true;
        for ev in events {
            if is_ir && ev.event_type() == EventType::ABSOLUTE {
                let c = ev.code();
                if (ABS_HAT0X..ABS_HAT0X + 8).contains(&c) {
                    let k = ((c - ABS_HAT0X) / 2) as usize;
                    if (c - ABS_HAT0X) % 2 == 0 {
                        s.dots[k].0 = ev.value();
                    } else {
                        s.dots[k].1 = ev.value();
                    }
                }
            } else if !is_ir && ev.event_type() == EventType::KEY {
                let bit = match ev.code() {
                    BTN_A => B_A,
                    BTN_B => B_B,
                    KEY_NEXT => B_PLUS,
                    KEY_PREVIOUS => B_MINUS,
                    BTN_MODE => B_HOME,
                    BTN_1 => B_1,
                    BTN_2 => B_2,
                    _ => 0,
                };
                if ev.value() != 0 {
                    if s.buttons & bit == 0 {
                        s.pressed |= bit;
                    }
                    s.buttons |= bit;
                } else {
                    s.buttons &= !bit;
                }
            }
        }
    }
}

/// Opens a second handle on the button device for force-feedback rumble.
fn start_rumble(path: &Path) -> Option<Sender<u32>> {
    let mut dev = Device::open(path).ok()?;
    let effect = FFEffectData {
        direction: 0,
        trigger: FFTrigger::default(),
        replay: FFReplay { length: 90, delay: 0 },
        kind: FFEffectKind::Rumble { strong_magnitude: 0xc000, weak_magnitude: 0x8000 },
    };
    let mut eff = dev.upload_ff_effect(effect).ok()?;
    let (tx, rx) = channel::<u32>();
    thread::Builder::new()
        .name("wiimote-rumble".into())
        .spawn(move || {
            let _keep = dev;
            while let Ok(_ms) = rx.recv() {
                let _ = eff.play(1);
            }
        })
        .ok()?;
    Some(tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ir_center_maps_to_screen_center() {
        let cfg = IrConfig { flip_x: true, flip_y: true, gain: 1.4, y_offset: 0.0 };
        let (x, y) = ir_to_screen(511.5, 383.5, &cfg);
        assert!((x - 0.5).abs() < 1e-3 && (y - 0.5).abs() < 1e-3);
        // Bar seen on the left of the camera image => pointing right.
        let (x, _) = ir_to_screen(200.0, 383.5, &cfg);
        assert!(x > 0.5);
    }
}
