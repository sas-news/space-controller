use anyhow::Result;
use evdev::{
    AbsInfo, AbsoluteAxisCode, AttributeSet, EventType, InputEvent, KeyCode, UinputAbsSetup,
};

use crate::mapping::DestKey;

pub const PAD_NAME: &str = "Space Controller Gamepad";
const ABS_RANGE: i32 = 32767;

pub struct VirtualPad {
    dev: evdev::uinput::VirtualDevice,
}

impl VirtualPad {
    pub fn create() -> Result<Self> {
        let mut keys = AttributeSet::<KeyCode>::new();
        for k in DestKey::ALL {
            keys.insert(KeyCode(k.code()));
        }

        let mut builder = evdev::uinput::VirtualDevice::builder()?.name(PAD_NAME);
        for code in 0..6u16 {
            // flat=0: the mapping layer owns deadzone handling.
            builder = builder.with_absolute_axis(&UinputAbsSetup::new(
                AbsoluteAxisCode(code),
                AbsInfo::new(0, -ABS_RANGE, ABS_RANGE, 0, 0, 0),
            ))?;
        }
        let dev = builder.with_keys(&keys)?.build()?;
        Ok(Self { dev })
    }

    /// Dev nodes (e.g. /dev/input/eventN) of the virtual device.
    pub fn dev_nodes(
        &mut self,
    ) -> Result<impl Iterator<Item = std::io::Result<std::path::PathBuf>> + '_> {
        Ok(self.dev.enumerate_dev_nodes_blocking()?)
    }

    /// Emit a full state snapshot. `axes` are -1.0..=1.0 indexed by
    /// OutAxis order; `pressed` are source->dest resolved key codes.
    pub fn emit(&mut self, axes: &[f32; 6], pressed: &[u16]) -> Result<()> {
        let mut evs = Vec::with_capacity(6 + DestKey::ALL.len());
        for (i, v) in axes.iter().enumerate() {
            evs.push(InputEvent::new_now(
                EventType::ABSOLUTE.0,
                i as u16,
                (v.clamp(-1.0, 1.0) * ABS_RANGE as f32) as i32,
            ));
        }
        for k in DestKey::ALL {
            evs.push(InputEvent::new_now(
                EventType::KEY.0,
                k.code(),
                pressed.contains(&k.code()) as i32,
            ));
        }
        self.dev.emit(&evs)?;
        Ok(())
    }
}
