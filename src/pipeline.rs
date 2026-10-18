use crate::input::Discovered;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::Result;

use crate::input;
use crate::mapping::{apply_axis, InputState, Profile};
use crate::output::VirtualPad;

#[derive(Clone)]
pub enum Source {
    Device(Discovered),
    Demo,
}

const TICK_HZ: f32 = 240.0;
/// Silence window after which a REL-mode axis is treated as centered.
const REL_DECAY: Duration = Duration::from_millis(120);

pub struct Pipeline {
    stop: Arc<AtomicBool>,
    handles: Vec<JoinHandle<()>>,
}

impl Pipeline {
    pub fn start(
        source: Source,
        state: Arc<Mutex<InputState>>,
        profile: Arc<RwLock<Profile>>,
        preview_out: Arc<Mutex<[f32; 6]>>,
    ) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));

        // Create the virtual pad before spawning the reader so a uinput
        // failure doesn't leave the source device grabbed.
        let mut pad = VirtualPad::create()?;
        eprintln!("[space-controller] virtual pad created");

        let reader = match &source {
            Source::Device(d) => input::spawn_reader(d.clone(), state.clone(), stop.clone()),
            Source::Demo => input::spawn_demo(state.clone(), stop.clone()),
        };
        let stop_out = stop.clone();
        let out_thread = std::thread::spawn(move || {
            let tick = Duration::from_secs_f32(1.0 / TICK_HZ);
            let dt = tick.as_secs_f32();
            let mut acc = [0.0f32; 6];
            let mut emit_err_logged = false;
            while !stop_out.load(Ordering::Relaxed) {
                let (axes_in, btns_in) = {
                    let mut st = state.lock().unwrap();
                    if st.rel_mode {
                        // REL sources stop reporting when the cap is centered;
                        // decay silent axes back to zero.
                        let now = std::time::Instant::now();
                        for i in 0..6 {
                            let stale = st.rel_stamp[i]
                                .is_none_or(|t| now.duration_since(t) > REL_DECAY);
                            if stale {
                                st.axes[i] = 0.0;
                            }
                        }
                    }
                    (st.axes, st.buttons.clone())
                };
                let (out, pressed) = {
                    let prof = profile.read().unwrap();
                    let mut out = [0.0f32; 6];
                    for (i, m) in prof.axes.iter().enumerate().take(6) {
                        let v = apply_axis(m, axes_in[i], &mut acc[i], dt);
                        out[m.dest.code() as usize] = v;
                    }
                    let pressed: Vec<u16> = btns_in
                        .iter()
                        .filter_map(|src| {
                            prof.buttons
                                .iter()
                                .find(|b| b.src == *src)
                                .map(|b| b.dest.code())
                        })
                        .collect();
                    (out, pressed)
                };
                if let Err(e) = pad.emit(&out, &pressed) {
                    if !emit_err_logged {
                        eprintln!("[space-controller] emit: {e:#}");
                        emit_err_logged = true;
                    }
                }
                *preview_out.lock().unwrap() = out;
                std::thread::sleep(tick);
            }
        });

        Ok(Self {
            stop,
            handles: vec![reader, out_thread],
        })
    }

    pub fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        for h in self.handles {
            let _ = h.join();
        }
    }
}
