//! Wii Remote support (Linux): through a Mayflash DolphinBar in mode 4 if one is plugged in,
//! otherwise over raw Bluetooth L2CAP.
//!
//! DolphinBar: in mode 4 the bar shows up as four USB HID devices (057e:0306, one per slot) that
//! pass the Wii Remote HID protocol straight through, so the game opens their hidraw nodes.
//! Remotes sync with the bar itself (1+2 or SYNC), and the bar is also the sensor bar. Writing to
//! a slot with no remote in it fails with EPIPE, which is how an empty slot is told apart. The
//! bar doesn't keep a remote in the same slot when it reconnects, so a remote is recognized by its
//! factory accelerometer calibration (different on every remote) and keeps its id, and so its
//! player, until the game quits.
//!
//! Direct: no pairing, kernel driver or udev rule: a remote put into discoverable mode with 1+2 is found
//! through BlueZ discovery (D-Bus), then the game opens the remote's two L2CAP channels itself
//! (PSM 0x11 control, 0x13 interrupt) and speaks the Wii Remote HID protocol on them:
//! report mode 0x33 (buttons + accelerometer + IR), the IR camera init sequence, player LEDs
//! and rumble. A background thread keeps discovering while fewer than four remotes are
//! connected; one thread per remote owns its sockets. The game polls `WiimoteManager::poll()`
//! each frame.
//!
//! Remotes paired with BlueZ are skipped: BlueZ hands their connections to the kernel driver.

use std::collections::HashMap;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

pub const B_A: u32 = 1;
pub const B_B: u32 = 2;
pub const B_PLUS: u32 = 4;
pub const B_MINUS: u32 = 8;
pub const B_HOME: u32 = 16;
pub const B_1: u32 = 32;
pub const B_2: u32 = 64;

/// Core button bits in input reports, mapped to the game's button bits.
const BUTTON_MAP: [(u16, u32); 7] = [
    (0x0010, B_PLUS),
    (0x0100, B_2),
    (0x0200, B_1),
    (0x0400, B_B),
    (0x0800, B_A),
    (0x1000, B_MINUS),
    (0x8000, B_HOME),
];

const MAX_REMOTES: usize = 4;
/// A remote with no button held for this long is turned off to save its batteries: on Bluetooth
/// by dropping its connection, on a DolphinBar (which can't drop one remote) by power-cycling the
/// bar once all its remotes are idle.
const IDLE_OFF: Duration = Duration::from_secs(30);
const PSM_CTRL: u16 = 0x11;
const PSM_INTR: u16 = 0x13;
const AF_BLUETOOTH: i32 = 31;
const BTPROTO_L2CAP: i32 = 0;

#[derive(Clone, Default)]
struct Shared {
    connected: bool,
    /// The connection driving this remote (see `add_remote`); a stale one stops when it changes.
    conn: u64,
    /// DolphinBar slot (0..3) the remote is in.
    slot: Option<usize>,
    buttons: u32,
    pressed: u32, // edges since last poll
    dots: [Option<(i32, i32)>; 4],
    /// Wanted LED bits (0x10 = player 1 .. 0x80 = player 4) and rumble end; the remote's
    /// thread sends them.
    leds: u8,
    rumble_until: Option<Instant>,
    report_rate: u32,
    /// No button held for `IDLE_OFF`.
    idle: bool,
}

/// Per-remote snapshot handed to the game.
#[derive(Clone, Debug)]
pub struct RemoteState {
    pub id: u32,
    /// DolphinBar slot (0..3), if the remote is on a DolphinBar.
    pub slot: Option<usize>,
    pub buttons: u32,
    pub pressed: u32,
    /// Pointer position in 0..1 screen coordinates, if the sensor bar is visible.
    pub aim: Option<(f32, f32)>,
    pub raw_dots: Vec<(i32, i32)>,
    /// Input reports per second, for the debug overlay.
    pub report_rate: u32,
}

#[derive(Clone)]
pub struct IrConfig {
    pub flip_x: bool,
    pub flip_y: bool,
    pub gain: f32,
    pub y_offset: f32,
}

type Remotes = Arc<Mutex<HashMap<u32, Shared>>>;

pub struct WiimoteManager {
    remotes: Remotes,
    /// Remotes go through a DolphinBar instead of Bluetooth (decided at startup).
    dolphinbar: bool,
    half_sep: HashMap<u32, (f32, f32)>,
    last_mid: HashMap<u32, (f32, f32)>,
    pub cfg: IrConfig,
}

impl WiimoteManager {
    pub fn start(cfg: IrConfig) -> WiimoteManager {
        let remotes: Remotes = Arc::new(Mutex::new(HashMap::new()));
        let r2 = remotes.clone();
        let dolphinbar = !dolphinbar_slots().is_empty();
        thread::Builder::new()
            .name("wiimote-scan".into())
            .spawn(move || scan_loop(r2, dolphinbar))
            .ok();
        WiimoteManager { remotes, dolphinbar, half_sep: HashMap::new(), last_mid: HashMap::new(), cfg }
    }

    pub fn dolphinbar(&self) -> bool {
        self.dolphinbar
    }

    pub fn poll(&mut self) -> Vec<RemoteState> {
        let mut snap = vec![];
        {
            let mut map = self.remotes.lock().unwrap();
            for (&id, s) in map.iter_mut() {
                if !s.connected {
                    continue;
                }
                let dots: Vec<(i32, i32)> = s.dots.iter().flatten().copied().collect();
                snap.push((id, s.slot, s.buttons, s.pressed, dots, s.report_rate));
                s.pressed = 0;
            }
        }
        let mut out = vec![];
        for (id, slot, buttons, pressed, dots, report_rate) in snap {
            let aim = self.compute_aim(id, &dots);
            out.push(RemoteState { id, slot, buttons, pressed, aim, raw_dots: dots, report_rate });
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
        if let Some(s) = self.remotes.lock().unwrap().get_mut(&id) {
            s.rumble_until = Some(Instant::now() + Duration::from_millis(ms as u64));
        }
    }

    /// Lights player LED `slot` (0..3) on a remote, or all four for no player.
    pub fn set_led(&self, id: u32, slot: Option<usize>) {
        if let Some(s) = self.remotes.lock().unwrap().get_mut(&id) {
            match slot {
                Some(slot) => {
                    s.leds = 0x10 << slot.min(3);
                    eprintln!("[wiimote] remote #{id} is player {}", slot.min(3) + 1);
                }
                None => s.leds = 0xF0,
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

type Objects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

/// Uses the DolphinBar if one was plugged in at startup, and Bluetooth discovery otherwise. The
/// two never run at once: a remote pressing 1+2 to sync with the bar would be grabbed by
/// discovery too.
fn scan_loop(remotes: Remotes, dolphinbar: bool) {
    let mut ids: HashMap<String, u32> = HashMap::new();
    if dolphinbar {
        dolphinbar_session(&remotes);
    }
    loop {
        if let Err(e) = scan_session(&remotes, &mut ids) {
            eprintln!("[wiimote] Bluetooth unavailable ({e}), retrying");
            thread::sleep(Duration::from_secs(5));
        }
    }
}

/// Marks remote `id` connected through a new connection, and returns that connection's token.
/// A remote that reconnects keeps its player LED; a new one lights all four (connected, no player
/// yet) so it doesn't look switched off.
fn add_remote(map: &mut HashMap<u32, Shared>, id: u32, slot: Option<usize>) -> u64 {
    static NEXT_CONN: AtomicU64 = AtomicU64::new(1);
    let conn = NEXT_CONN.fetch_add(1, Ordering::Relaxed);
    let s = map.entry(id).or_default();
    let leds = if s.leds != 0 { s.leds } else { 0xF0 };
    *s = Shared { connected: true, conn, slot, leds, ..Shared::default() };
    conn
}

/// Marks remote `id` disconnected, unless another connection has taken it over since.
fn remove_remote(remotes: &Remotes, id: u32, conn: u64) {
    if let Some(s) = remotes.lock().unwrap().get_mut(&id).filter(|s| s.conn == conn) {
        s.connected = false;
        s.buttons = 0;
        eprintln!("[wiimote] remote #{id} disconnected");
    }
}

// ------------------------------------------------------------------ DolphinBar (hidraw)

/// The DolphinBar's slots as (HID_PHYS, /dev/hidrawN), in slot order.
fn dolphinbar_slots() -> Vec<(String, String)> {
    let mut out = vec![];
    for e in std::fs::read_dir("/sys/class/hidraw").into_iter().flatten().flatten() {
        let Ok(uevent) = std::fs::read_to_string(e.path().join("device/uevent")) else { continue };
        let field = |k: &str| uevent.lines().find_map(|l| l.strip_prefix(k)).map(str::to_string);
        // Bus 0003 is USB; remotes on Bluetooth (bus 0005) have the same IDs.
        if field("HID_ID=").is_some_and(|id| id.eq_ignore_ascii_case("0003:0000057E:00000306")) {
            let phys = field("HID_PHYS=").unwrap_or_default();
            out.push((phys, format!("/dev/{}", e.file_name().to_string_lossy())));
        }
    }
    out.sort();
    out
}

/// Runs one thread per DolphinBar slot, and waits for the bar to come back if it's unplugged.
fn dolphinbar_session(remotes: &Remotes) -> ! {
    eprintln!("[wiimote] using the DolphinBar (press 1+2 or SYNC on a remote to sync it with the bar)");
    let mut threads: HashMap<String, thread::JoinHandle<()>> = HashMap::new();
    let mut warned = false;
    let mut plugged = true;
    let known: Known = Arc::new(Mutex::new(HashMap::new()));
    let mut can_power_cycle = true;
    loop {
        let slots = dolphinbar_slots();
        if slots.is_empty() == plugged {
            plugged = !plugged;
            eprintln!("[wiimote] DolphinBar {}", if plugged { "plugged back in" } else { "unplugged" });
        }
        for (slot, (phys, dev)) in slots.into_iter().enumerate() {
            if threads.get(&phys).is_some_and(|t| !t.is_finished()) {
                continue;
            }
            let fd = match std::fs::OpenOptions::new().read(true).write(true).open(&dev) {
                Ok(f) => OwnedFd::from(f),
                Err(e) => {
                    if !warned {
                        eprintln!("[wiimote] can't open the DolphinBar ({dev}: {e}); see README for the udev rule");
                        warned = true;
                    }
                    continue;
                }
            };
            let (r, k) = (remotes.clone(), known.clone());
            if let Ok(t) = thread::Builder::new().name(format!("dolphinbar-{slot}")).spawn(move || run_slot(slot, fd, &r, &k)) {
                threads.insert(phys, t);
            }
        }
        let all_idle = {
            let map = remotes.lock().unwrap();
            let on: Vec<&Shared> = map.values().filter(|s| s.connected).collect();
            !on.is_empty() && on.iter().all(|s| s.idle)
        };
        if all_idle && can_power_cycle {
            eprintln!("[wiimote] no buttons pressed for {}s, power-cycling the DolphinBar to turn its remotes off", IDLE_OFF.as_secs());
            if let Err(e) = power_cycle_dolphinbar() {
                eprintln!("[wiimote] can't power-cycle the DolphinBar ({e}), so its remotes stay on; see README for the udev rule");
                can_power_cycle = false;
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
}

/// Cuts power to the DolphinBar's USB port for a moment, which drops all its remotes (they turn
/// off). Its slot threads then stop, and `dolphinbar_session` picks the bar up again when it's back.
fn power_cycle_dolphinbar() -> std::io::Result<()> {
    let bar = std::fs::read_dir("/sys/bus/usb/devices")?
        .flatten()
        .map(|e| e.path())
        .find(|d| {
            let attr = |k: &str| std::fs::read_to_string(d.join(k)).unwrap_or_default();
            attr("idVendor").trim() == "057e" && attr("idProduct").trim() == "0306"
        })
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "not found"))?;
    // The hub port's `disable` clears the port's power (if the hub switches ports separately).
    // Resolve it first: `bar` (and its `port` link) goes away as soon as the bar loses power.
    let disable = std::fs::canonicalize(bar.join("port"))?.join("disable");
    std::fs::write(&disable, "1")?;
    thread::sleep(Duration::from_secs(3));
    // Turning it back on must not fail for good, or the bar stays off until it's replugged.
    let mut result = Ok(());
    for _ in 0..5 {
        result = std::fs::write(&disable, "0");
        if result.is_ok() {
            break;
        }
        thread::sleep(Duration::from_millis(500));
    }
    result
}

/// Remote ids by factory accelerometer calibration, for remotes seen on the DolphinBar.
type Known = Arc<Mutex<HashMap<[u8; 10], u32>>>;

/// Waits for a remote to sync into a DolphinBar slot and runs it, until the bar goes away.
fn run_slot(slot: usize, fd: OwnedFd, remotes: &Remotes, known: &Known) {
    let link = Link { fd, hidraw: true };
    let mut warned = false;
    loop {
        match link.out(&[0x15, 0x00]) {
            Ok(()) => {}
            Err(e) if e.raw_os_error() == Some(libc::ENODEV) => return,
            Err(e) => {
                if e.raw_os_error() != Some(libc::EPIPE) && !warned {
                    eprintln!("[wiimote] DolphinBar slot {}: {e}", slot + 1);
                    warned = true;
                }
                thread::sleep(Duration::from_millis(500));
                continue;
            }
        }
        let calib = match read_calibration(&link) {
            Ok(c) => Some(c),
            Err(e) if e.raw_os_error() == Some(libc::ENODEV) => return,
            Err(e) => {
                eprintln!("[wiimote] DolphinBar slot {}: can't read the remote's calibration ({e}), so it won't get its player back if it reconnects", slot + 1);
                None
            }
        };
        let (id, conn) = {
            let mut map = remotes.lock().unwrap();
            let mut known = known.lock().unwrap();
            // A remote seen before gets its old id back, unless that id is still in use. A slot whose
            // remote moved elsewhere can still look connected, but it gets no reports; a second
            // remote with the same calibration does.
            let old = calib.and_then(|c| known.get(&c).copied());
            let id = match old {
                Some(id) if map.get(&id).is_none_or(|s| !s.connected || s.report_rate == 0) => id,
                _ => {
                    let id = map.len() as u32;
                    if let Some(c) = calib {
                        known.entry(c).or_insert(id);
                    }
                    id
                }
            };
            (id, add_remote(&mut map, id, Some(slot)))
        };
        eprintln!("[wiimote] remote #{id} connected (DolphinBar slot {})", slot + 1);
        run_remote(id, conn, &link, remotes);
        remove_remote(remotes, id, conn);
    }
}

/// Reads the remote's factory accelerometer calibration (EEPROM 0x16, 10 bytes).
fn read_calibration(link: &Link) -> std::io::Result<[u8; 10]> {
    link.out(&[0x17, 0x00, 0x00, 0x00, 0x16, 0x00, 0x0A])?;
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        // 0x21: buttons, size-1 and error nibbles, address, 16 data bytes.
        if let Some(r) = link.recv(Duration::from_millis(100))? {
            if r.len() >= 16 && r[0] == 0x21 && r[4..6] == [0x00, 0x16] {
                if r[3] & 0x0F != 0 {
                    return Err(std::io::Error::other(format!("read error {:#x}", r[3] & 0x0F)));
                }
                return Ok(r[6..16].try_into().unwrap());
            }
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "no reply"))
}

// ------------------------------------------------------------------ discovery (BlueZ D-Bus)

/// Discovers and connects remotes until BlueZ or the adapter goes away.
fn scan_session(remotes: &Remotes, ids: &mut HashMap<String, u32>) -> zbus::Result<()> {
    let conn = Connection::system()?;
    let om = Proxy::new(&conn, "org.bluez", "/", "org.freedesktop.DBus.ObjectManager")?;
    let objects: Objects = om.call("GetManagedObjects", &())?;
    let Some(adapter_path) = objects.iter().find(|(_, ifs)| ifs.contains_key("org.bluez.Adapter1")).map(|(p, _)| p.clone())
    else {
        return Err(zbus::Error::Failure("no Bluetooth adapter".into()));
    };
    let adapter = Proxy::new(&conn, "org.bluez", adapter_path.as_str(), "org.bluez.Adapter1")?;
    let mut filter: HashMap<&str, Value> = HashMap::new();
    filter.insert("Transport", Value::from("bredr"));
    adapter.call_method("SetDiscoveryFilter", &(filter,))?;
    eprintln!("[wiimote] scanning on {} (press 1+2 on a remote)", adapter_path.as_str());

    let mut last_try: HashMap<String, Instant> = HashMap::new();
    let mut warned_paired: Vec<String> = vec![];
    let mut discovering = false;
    loop {
        let active: Vec<u32> = remotes.lock().unwrap().iter().filter(|(_, s)| s.connected).map(|(&id, _)| id).collect();
        // Inquiry takes radio time from connected remotes, so stop once everyone is in.
        let want = active.len() < MAX_REMOTES;
        if want != discovering {
            set_discovery(&adapter, want)?;
            discovering = want;
        }
        thread::sleep(Duration::from_millis(500));
        if !discovering {
            continue;
        }

        let objects: Objects = om.call("GetManagedObjects", &())?;
        if !objects.contains_key(&adapter_path) {
            return Err(zbus::Error::Failure("adapter removed".into()));
        }
        for ifs in objects.values() {
            let Some(dev) = ifs.get("org.bluez.Device1") else { continue };
            // RSSI is only set for devices seen since discovery last started.
            if !prop_str(dev, "Name").is_some_and(|n| n.starts_with("Nintendo RVL-CNT-01")) || !dev.contains_key("RSSI") {
                continue;
            }
            let Some(addr) = prop_str(dev, "Address") else { continue };
            if ids.get(&addr).is_some_and(|id| active.contains(id))
                || last_try.get(&addr).is_some_and(|t| t.elapsed() < Duration::from_secs(3))
            {
                continue;
            }
            last_try.insert(addr.clone(), Instant::now());
            if matches!(dev.get("Paired").map(|v| &**v), Some(Value::Bool(true))) {
                if !warned_paired.contains(&addr) {
                    eprintln!("[wiimote] {addr} is paired with BlueZ, so the kernel takes it; unpair it with: bluetoothctl remove {addr}");
                    warned_paired.push(addr);
                }
                continue;
            }

            // Inquiry and paging share the radio; connecting is much more reliable without it.
            set_discovery(&adapter, false)?;
            discovering = false;
            match connect(&addr) {
                Ok((ctrl, intr)) => {
                    let next = ids.len() as u32;
                    let id = *ids.entry(addr.clone()).or_insert(next);
                    eprintln!("[wiimote] remote #{id} connected ({addr})");
                    // Count it as active right away so the next pass doesn't connect it twice.
                    let conn = add_remote(&mut remotes.lock().unwrap(), id, None);
                    let r = remotes.clone();
                    thread::Builder::new()
                        .name(format!("wiimote-{id}"))
                        .spawn(move || {
                            // Output and input reports both go on the interrupt channel; the
                            // control channel only has to stay open.
                            let _ctrl = ctrl;
                            run_remote(id, conn, &Link { fd: intr, hidraw: false }, &r);
                            remove_remote(&r, id, conn);
                        })
                        .ok();
                }
                Err(e) => eprintln!("[wiimote] connecting {addr} failed: {e}"),
            }
            break; // re-read the device list once discovery is running again
        }
    }
}

fn set_discovery(adapter: &Proxy, on: bool) -> zbus::Result<()> {
    match adapter.call_method(if on { "StartDiscovery" } else { "StopDiscovery" }, &()) {
        Ok(_) => Ok(()),
        // Already in the wanted state (or another client is discovering too).
        Err(zbus::Error::MethodError(name, _, _))
            if name.as_str().ends_with(".InProgress") || name.as_str().ends_with(".Failed") && !on =>
        {
            Ok(())
        }
        Err(e) => Err(e),
    }
}

fn prop_str(props: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    match props.get(key).map(|v| &**v) {
        Some(Value::Str(s)) => Some(s.to_string()),
        _ => None,
    }
}

// ------------------------------------------------------------------ L2CAP sockets

#[repr(C)]
struct SockaddrL2 {
    l2_family: u16,
    l2_psm: u16,
    l2_bdaddr: [u8; 6],
    l2_cid: u16,
    l2_bdaddr_type: u8,
}

/// "AA:BB:CC:DD:EE:FF" to bdaddr_t, which is little-endian (last printed byte first).
fn parse_addr(s: &str) -> Option<[u8; 6]> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 6 {
        return None;
    }
    let mut out = [0u8; 6];
    for (i, p) in parts.iter().enumerate() {
        out[5 - i] = u8::from_str_radix(p, 16).ok()?;
    }
    Some(out)
}

fn l2cap_connect(addr: &[u8; 6], psm: u16) -> std::io::Result<OwnedFd> {
    unsafe {
        let fd = libc::socket(AF_BLUETOOTH, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, BTPROTO_L2CAP);
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let fd = OwnedFd::from_raw_fd(fd);
        let sa = SockaddrL2 { l2_family: AF_BLUETOOTH as u16, l2_psm: psm.to_le(), l2_bdaddr: *addr, l2_cid: 0, l2_bdaddr_type: 0 };
        let r = libc::connect(
            fd.as_raw_fd(),
            &sa as *const SockaddrL2 as *const libc::sockaddr,
            std::mem::size_of::<SockaddrL2>() as libc::socklen_t,
        );
        if r < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(fd)
    }
}

fn connect(addr: &str) -> Result<(OwnedFd, OwnedFd), String> {
    let a = parse_addr(addr).ok_or("bad address")?;
    let ctrl = l2cap_connect(&a, PSM_CTRL).map_err(|e| format!("control channel: {e}"))?;
    let intr = l2cap_connect(&a, PSM_INTR).map_err(|e| format!("interrupt channel: {e}"))?;
    Ok((ctrl, intr))
}

fn send(fd: &OwnedFd, report: &[u8]) -> std::io::Result<()> {
    let r = unsafe { libc::write(fd.as_raw_fd(), report.as_ptr() as *const _, report.len()) };
    if r < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Waits up to `timeout` for one input report. Ok(None) on timeout, Err on disconnect.
fn recv(fd: &OwnedFd, timeout: Duration) -> std::io::Result<Option<Vec<u8>>> {
    let mut pfd = libc::pollfd { fd: fd.as_raw_fd(), events: libc::POLLIN, revents: 0 };
    let r = unsafe { libc::poll(&mut pfd, 1, timeout.as_millis() as i32) };
    if r < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if r == 0 {
        return Ok(None);
    }
    let mut buf = [0u8; 64];
    let n = unsafe { libc::read(fd.as_raw_fd(), buf.as_mut_ptr() as *mut _, buf.len()) };
    if n < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if n == 0 {
        return Err(std::io::Error::new(std::io::ErrorKind::ConnectionReset, "remote disconnected"));
    }
    Ok(Some(buf[..n as usize].to_vec()))
}

// ------------------------------------------------------------------ Wii Remote protocol

/// Where a remote's reports go: an L2CAP interrupt channel, where they carry a HID transaction
/// byte (0xA2 out, 0xA1 in), or a DolphinBar hidraw node, where they don't.
struct Link {
    fd: OwnedFd,
    hidraw: bool,
}

impl Link {
    /// Sends one output report (report ID first).
    fn out(&self, report: &[u8]) -> std::io::Result<()> {
        if self.hidraw {
            send(&self.fd, report)
        } else {
            send(&self.fd, &[&[0xA2], report].concat())
        }
    }

    /// Waits up to `timeout` for one input report (report ID first).
    fn recv(&self, timeout: Duration) -> std::io::Result<Option<Vec<u8>>> {
        Ok(match recv(&self.fd, timeout)? {
            Some(r) if self.hidraw => Some(r),
            Some(r) if r.first() == Some(&0xA1) => Some(r[1..].to_vec()),
            _ => None,
        })
    }
}

struct Remote<'a> {
    id: u32,
    conn: u64,
    link: &'a Link,
    remotes: &'a Remotes,
    /// What the remote currently has (sent with every output report).
    leds: u8,
    rumble: bool,
    reports: u32,
    /// Set by a buttons-only report (0x30), which is what a remote sends after it powers up.
    reset: bool,
    /// When a button was last held, for `IDLE_OFF`.
    last_press: Instant,
}

impl Remote<'_> {
    /// Byte 1 bit 0 of every output report is the rumble motor.
    fn out(&self, id: u8, payload: &[u8]) -> std::io::Result<()> {
        let mut r = vec![id];
        r.extend_from_slice(payload);
        if self.rumble {
            r[1] |= 1;
        }
        self.link.out(&r)
    }

    fn write_reg(&mut self, addr: u32, data: &[u8]) -> Result<(), String> {
        let mut p = [0u8; 21];
        p[0] = 0x04; // control registers
        p[1] = (addr >> 16) as u8;
        p[2] = (addr >> 8) as u8;
        p[3] = addr as u8;
        p[4] = data.len() as u8;
        p[5..5 + data.len()].copy_from_slice(data);
        self.out(0x16, &p).map_err(|e| e.to_string())?;
        // Wait for the 0x22 acknowledgement before the next write.
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            match self.link.recv(Duration::from_millis(100)).map_err(|e| e.to_string())? {
                Some(r) if r.len() >= 5 && r[0] == 0x22 && r[3] == 0x16 => {
                    return if r[4] == 0 { Ok(()) } else { Err(format!("write {addr:#x} error {:#x}", r[4])) };
                }
                Some(r) => self.handle(&r),
                None => {}
            }
        }
        Err(format!("no ack for write to {addr:#x}"))
    }

    fn set_report_mode(&self) -> std::io::Result<()> {
        self.out(0x12, &[0x00, 0x33]) // buttons + accelerometer + 12-byte IR
    }

    fn init(&mut self) -> Result<(), String> {
        // IR camera: enable clocks, then the wiibrew init sequence ("Wii level 3" sensitivity).
        self.out(0x13, &[0x04]).map_err(|e| e.to_string())?;
        thread::sleep(Duration::from_millis(50));
        self.out(0x1A, &[0x04]).map_err(|e| e.to_string())?;
        thread::sleep(Duration::from_millis(50));
        self.write_reg(0xB00030, &[0x08])?;
        self.write_reg(0xB00000, &[0x02, 0x00, 0x00, 0x71, 0x01, 0x00, 0xAA, 0x00, 0x64])?;
        self.write_reg(0xB0001A, &[0x63, 0x03])?;
        self.write_reg(0xB00033, &[0x03])?; // extended mode: x, y, size per dot
        self.write_reg(0xB00030, &[0x08])?;
        self.set_report_mode().map_err(|e| e.to_string())
    }

    /// Sends the LEDs / rumble the game asked for, if they changed. Fails once another connection
    /// has taken the remote over.
    fn sync_outputs(&mut self) -> std::io::Result<()> {
        let (leds, rumble) = {
            let map = self.remotes.lock().unwrap();
            let Some(s) = map.get(&self.id) else { return Ok(()) };
            if s.conn != self.conn {
                return Err(std::io::Error::other("taken over"));
            }
            (s.leds, s.rumble_until.is_some_and(|t| t > Instant::now()))
        };
        if leds != self.leds || rumble != self.rumble {
            self.leds = leds;
            self.rumble = rumble;
            self.out(0x11, &[leds])?;
        }
        Ok(())
    }

    fn handle(&mut self, r: &[u8]) {
        if r.len() < 3 {
            return;
        }
        let id = r[0];
        self.reports += 1;
        if id == 0x30 {
            self.reset = true;
        }
        let mut map = self.remotes.lock().unwrap();
        let Some(s) = map.get_mut(&self.id).filter(|s| s.conn == self.conn) else { return };
        // Every report from 0x20 up except 0x3D starts with the core buttons.
        if (0x20..=0x3F).contains(&id) && id != 0x3D {
            let raw = u16::from_le_bytes([r[1], r[2]]);
            let b = BUTTON_MAP.iter().filter(|(m, _)| raw & m != 0).fold(0, |acc, (_, bit)| acc | bit);
            s.pressed |= b & !s.buttons;
            s.buttons = b;
            if b != 0 {
                self.last_press = Instant::now();
            }
        }
        match id {
            0x20 => {
                // A status report (sent e.g. when an extension is plugged in) stops data
                // reporting until the mode is set again.
                drop(map);
                let _ = self.set_report_mode();
            }
            0x33 if r.len() >= 18 => {
                for i in 0..4 {
                    let d = &r[6 + i * 3..9 + i * 3];
                    let x = d[0] as i32 | (((d[2] >> 4) & 3) as i32) << 8;
                    let y = d[1] as i32 | (((d[2] >> 6) & 3) as i32) << 8;
                    // An empty slot reads as (1023, 1023), whatever the size nibble says.
                    s.dots[i] = if (x, y) == (0x3FF, 0x3FF) { None } else { Some((x, y)) };
                }
            }
            _ => {}
        }
    }
}

/// Runs a connected remote until it disconnects. On a DolphinBar it also stops when the remote
/// resets, so that `run_slot` can tell which remote it is now.
fn run_remote(id: u32, conn: u64, link: &Link, remotes: &Remotes) {
    let setup = |remote: &mut Remote| {
        // leds: 0xFF never matches, so the next sync stops the remote's search blinking.
        remote.leds = 0xFF;
        if let Err(e) = remote.init() {
            eprintln!("[wiimote] remote #{id}: IR setup failed: {e}");
        }
        remote.reset = false;
        // A short buzz so whoever pressed 1+2 knows it worked.
        remotes.lock().unwrap().entry(id).or_default().rumble_until = Some(Instant::now() + Duration::from_millis(150));
        Instant::now()
    };
    let mut remote = Remote { id, conn, link, remotes, leds: 0xFF, rumble: false, reports: 0, reset: false, last_press: Instant::now() };
    let mut setup_t = setup(&mut remote);
    let mut rate_t = Instant::now();
    let mut last_report = Instant::now();
    loop {
        // Short timeout so rumble and LED changes go out promptly.
        match link.recv(Duration::from_millis(10)) {
            Ok(Some(r)) => {
                remote.handle(&r);
                last_report = Instant::now();
            }
            Ok(None) => {}
            Err(_) => return,
        }
        // A remote that leaves a DolphinBar slot just goes quiet, so when nothing has come in
        // for a second, ask for a status report: the write fails once the remote is gone.
        if last_report.elapsed() >= Duration::from_secs(1) {
            if remote.out(0x15, &[0x00]).is_err() {
                return;
            }
            last_report = Instant::now();
        }
        // A DolphinBar doesn't always tell us that a remote left its slot, so a remote that
        // comes back (maybe a different one) can look like it never went away, except that it's
        // back in its power-on mode (buttons only, IR camera off). Set it up again. The wait keeps
        // a remote that won't take the mode from being set up over and over.
        if remote.reset && setup_t.elapsed() >= Duration::from_secs(2) {
            if link.hidraw {
                return;
            }
            eprintln!("[wiimote] remote #{id} reset (reconnected?), setting it up again");
            setup_t = setup(&mut remote);
        }
        if remote.sync_outputs().is_err() {
            return;
        }
        // A remote turns itself off when its Bluetooth connection drops, so returning (which
        // closes the L2CAP channels) switches it off. A DolphinBar has no way to drop one remote;
        // `dolphinbar_session` handles that through `Shared::idle`.
        if !link.hidraw && remote.last_press.elapsed() >= IDLE_OFF {
            eprintln!("[wiimote] remote #{id}: no buttons pressed for {}s, turning it off", IDLE_OFF.as_secs());
            return;
        }
        if rate_t.elapsed() >= Duration::from_secs(1) {
            rate_t = Instant::now();
            if let Some(s) = remotes.lock().unwrap().get_mut(&id).filter(|s| s.conn == conn) {
                s.report_rate = remote.reports;
                s.idle = remote.last_press.elapsed() >= IDLE_OFF;
            }
            remote.reports = 0;
        }
    }
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

    #[test]
    fn bdaddr_is_little_endian() {
        assert_eq!(parse_addr("B8:AE:6E:EE:F5:AB"), Some([0xAB, 0xF5, 0xEE, 0x6E, 0xAE, 0xB8]));
        assert_eq!(parse_addr("B8:AE:6E"), None);
    }
}
