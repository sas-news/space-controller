use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::Result;
use evdev::{AbsoluteAxisCode, Device, EventType, RelativeAxisCode};

use crate::mapping::InputState;

const VENDOR_3DCONNEXION: u16 = 0x256f;
/// Old Logitech-era devices still carry the Logitech vendor id.
const VENDOR_LOGITECH: u16 = 0x046d;

/// Rough full-scale for REL deltas reported by old space mice; per-report
/// values at full deflection are around ±400.
const REL_FULL_SCALE: f32 = 400.0;
/// Absolute range of hidraw report values (i16, ±350 on SpaceNavigator).
const HID_FULL_SCALE: f32 = 350.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Evdev,
    Hidraw,
}

#[derive(Clone)]
pub struct Discovered {
    pub path: PathBuf,
    pub name: String,
    pub kind: SourceKind,
}

fn looks_like_spacemouse(name: &str, vendor: u16) -> bool {
    let lname = name.to_lowercase();
    ((vendor == VENDOR_3DCONNEXION || vendor == VENDOR_LOGITECH)
        && (lname.contains("3dconnexion") || lname.contains("space")))
        || lname.contains("3dconnexion")
        || lname.contains("spacemouse")
        || lname.contains("space navigator")
        || lname.contains("spaceball")
        || lname.contains("spacepilot")
}

/// List evdev input devices that look like 3Dconnexion space mice.
fn find_evdev() -> Vec<Discovered> {
    evdev::enumerate()
        .filter_map(|(path, dev)| {
            let name = dev.name().unwrap_or_default().to_string();
            looks_like_spacemouse(&name, dev.input_id().vendor()).then(|| Discovered {
                path,
                name: format!("{name} (evdev)"),
                kind: SourceKind::Evdev,
            })
        })
        .collect()
}

/// List hidraw nodes belonging to 3Dconnexion space mice. Reading hidraw
/// bypasses the kernel input layer entirely — needed for devices like the
/// old SpaceNavigator whose reports the bound hid driver drops.
fn find_hidraw() -> Vec<Discovered> {
    let rd = match fs::read_dir("/sys/class/hidraw") {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    rd.filter_map(|e| {
        let e = e.ok()?;
        let uevent = fs::read_to_string(e.path().join("device/uevent")).ok()?;
        let name = uevent
            .lines()
            .find_map(|l| l.strip_prefix("HID_NAME="))?
            .to_string();
        let hid_id = uevent
            .lines()
            .find_map(|l| l.strip_prefix("HID_ID="))
            .unwrap_or_default()
            .to_string();
        // HID_ID format: 0003:0000046D:0000C626
        let vendor = hid_id
            .split(':')
            .nth(1)
            .and_then(|v| u16::from_str_radix(v, 16).ok())
            .unwrap_or(0);
        if !looks_like_spacemouse(&name, vendor) {
            return None;
        }
        let devname = fs::read_to_string(e.path().join("uevent"))
            .ok()?
            .lines()
            .find_map(|l| l.strip_prefix("DEVNAME="))?
            .to_string();
        Some(Discovered {
            path: PathBuf::from(format!("/dev/{devname}")),
            name: format!("{name} (hidraw)"),
            kind: SourceKind::Hidraw,
        })
    })
    .collect()
}

pub fn find_devices() -> Vec<Discovered> {
    let mut v = find_hidraw();
    v.extend(find_evdev());
    v
}

/// Read the device and publish normalized state into `shared`.
/// Reopens on unplug until `stop` is set.
pub fn spawn_reader(
    dev: Discovered,
    shared: Arc<Mutex<InputState>>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let res = match dev.kind {
                SourceKind::Evdev => run_evdev(&dev.path, &shared, &stop),
                SourceKind::Hidraw => run_hidraw(&dev.path, &shared, &stop),
            };
            match res {
                Ok(()) => break,
                Err(e) => {
                    eprintln!("[space-controller] device read: {e:#} — retrying");
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(800));
                }
            }
        }
    })
}

const AXIS_CODES: [AbsoluteAxisCode; 6] = [
    AbsoluteAxisCode::ABS_X,
    AbsoluteAxisCode::ABS_Y,
    AbsoluteAxisCode::ABS_Z,
    AbsoluteAxisCode::ABS_RX,
    AbsoluteAxisCode::ABS_RY,
    AbsoluteAxisCode::ABS_RZ,
];

fn run_evdev(
    path: &PathBuf,
    shared: &Arc<Mutex<InputState>>,
    stop: &Arc<AtomicBool>,
) -> Result<()> {
    let mut dev = Device::open(path)?;
    // Best effort: exclusive grab so nothing else sees double input.
    let _ = dev.grab();

    // Older devices (e.g. Logitech-era SpaceNavigator, vendor 046d) report
    // movement as REL_* deltas; newer ones report ABS_* positions.
    let rel_mode = dev
        .supported_relative_axes()
        .is_some_and(|r| r.contains(RelativeAxisCode::REL_RX));

    {
        shared.lock().unwrap().rel_mode = rel_mode;
    }

    let mut ranges = [(-1.0f32, 1.0f32); 6];
    if !rel_mode {
        if let Ok(absinfo) = dev.get_absinfo() {
            for (code, info) in absinfo {
                if let Some(i) = AXIS_CODES.iter().position(|c| *c == code) {
                    let (min, max) = (info.minimum() as f32, info.maximum() as f32);
                    if max > min {
                        ranges[i] = (min, max);
                    }
                }
            }
        }
    }

    // Poll in nonblocking mode so the stop flag is honored even when the
    // device is silent.
    dev.set_nonblocking(true)?;

    let mut buttons: BTreeSet<u16> = BTreeSet::new();
    while !stop.load(Ordering::Relaxed) {
        match dev.fetch_events() {
            Ok(iter) => {
                for ev in iter {
                    let mut st = shared.lock().unwrap();
                    match ev.event_type() {
                        EventType::ABSOLUTE => {
                            let code = AbsoluteAxisCode(ev.code());
                            let Some(idx) = AXIS_CODES.iter().position(|c| *c == code) else {
                                continue;
                            };
                            let (min, max) = ranges[idx];
                            let v = ev.value() as f32;
                            st.axes[idx] =
                                ((v - min) / (max - min) * 2.0 - 1.0).clamp(-1.0, 1.0);
                        }
                        EventType::RELATIVE => {
                            // REL_X..REL_RZ are codes 0x00..0x05, same order
                            // as ABS.
                            let idx = ev.code() as usize;
                            if idx < 6 {
                                // Each report's value is proportional to cap
                                // deflection; the pipeline decays to zero on
                                // silence.
                                st.axes[idx] =
                                    (ev.value() as f32 / REL_FULL_SCALE).clamp(-1.0, 1.0);
                                st.rel_stamp[idx] = Some(std::time::Instant::now());
                            }
                        }
                        EventType::KEY => {
                            let code = ev.code();
                            if ev.value() != 0 {
                                buttons.insert(code);
                                st.last_key = Some(code);
                            } else {
                                buttons.remove(&code);
                            }
                            st.buttons = buttons.iter().copied().collect();
                        }
                        _ => {}
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(4));
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// Read raw HID reports. 3Dconnexion reports:
///   0x01: i16 LE X, Y, Z (translation)
///   0x02: i16 LE Rx, Ry, Rz (rotation)
///   0x03: buttons bitfield
/// Values are absolute (self-centering), ~±350.
fn run_hidraw(
    path: &PathBuf,
    shared: &Arc<Mutex<InputState>>,
    stop: &Arc<AtomicBool>,
) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;

    let f = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)?;
    let fd = f.as_raw_fd();

    let mut prev_buttons = 0u8;
    let mut buf = [0u8; 64];
    while !stop.load(Ordering::Relaxed) {
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut _, buf.len() as _) };
        if n < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::WouldBlock {
                std::thread::sleep(Duration::from_millis(2));
                continue;
            }
            return Err(e.into());
        }
        if n == 0 {
            continue;
        }
        let b = &buf[..n as usize];
        let mut st = shared.lock().unwrap();
        match b[0] {
            0x01 | 0x02 => {
                let base = if b[0] == 0x01 { 0 } else { 3 };
                for i in 0..3 {
                    if b.len() >= 1 + (i + 1) * 2 {
                        let v = i16::from_le_bytes([b[1 + i * 2], b[2 + i * 2]]);
                        st.axes[base + i] = (v as f32 / HID_FULL_SCALE).clamp(-1.0, 1.0);
                    }
                }
            }
            0x03 => {
                if b.len() >= 2 {
                    let bits = b[1];
                    let pressed: Vec<u16> = (0..8)
                        .filter(|bit| bits & (1 << bit) != 0)
                        .map(|bit| 0x100 + bit as u16)
                        .collect();
                    // record newly-pressed key for "press to assign"
                    let new = bits & !prev_buttons;
                    for bit in 0..8 {
                        if new & (1 << bit) != 0 {
                            st.last_key = Some(0x100 + bit as u16);
                        }
                    }
                    prev_buttons = bits;
                    st.buttons = pressed;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Synthetic input for development without hardware.
pub fn spawn_demo(shared: Arc<Mutex<InputState>>, stop: Arc<AtomicBool>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut t = 0.0f32;
        while !stop.load(Ordering::Relaxed) {
            t += 0.008;
            {
                let mut st = shared.lock().unwrap();
                st.axes[0] = (t * 0.9).sin();
                st.axes[1] = (t * 0.5).cos();
                st.axes[2] = (t * 1.3).sin() * 0.8;
                st.axes[3] = (t * 0.7).sin();
                st.axes[4] = (t * 1.1).cos() * 0.6;
                st.axes[5] = (t * 0.4).sin();
            }
            std::thread::sleep(Duration::from_millis(8));
        }
    })
}
