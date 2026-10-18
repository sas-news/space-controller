use serde::{Deserialize, Serialize};

/// Human-readable names for the 6 input degrees of freedom.
/// Indexed 0..6 matching evdev ABS_X, ABS_Y, ABS_Z, ABS_RX, ABS_RY, ABS_RZ.
pub const DOF_NAMES: [&str; 6] = [
    "Push L/R",        // ABS_X  translate
    "Push Fwd/Back",   // ABS_Y
    "Push Up/Down",    // ABS_Z
    "Tilt Fwd/Back",   // ABS_RX pitch
    "Tilt L/R",        // ABS_RY roll
    "Twist",           // ABS_RZ yaw
];

/// Output axis on the virtual gamepad.
/// Same numbering as evdev ABS_X..ABS_RZ (0..6); on Xbox-style pads Z/RZ are
/// the analog triggers LT/RT.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum OutAxis {
    X,
    Y,
    Z,
    Rx,
    Ry,
    Rz,
}

impl OutAxis {
    pub const ALL: [OutAxis; 6] = [
        OutAxis::X,
        OutAxis::Y,
        OutAxis::Z,
        OutAxis::Rx,
        OutAxis::Ry,
        OutAxis::Rz,
    ];

    pub fn label(self) -> &'static str {
        match self {
            OutAxis::X => "LX (stick X)",
            OutAxis::Y => "LY (stick Y)",
            OutAxis::Z => "LT (trigger)",
            OutAxis::Rx => "RX (stick X)",
            OutAxis::Ry => "RY (stick Y)",
            OutAxis::Rz => "RT (trigger)",
        }
    }

    /// evdev ABS_* code.
    pub fn code(self) -> u16 {
        match self {
            OutAxis::X => 0x00,
            OutAxis::Y => 0x01,
            OutAxis::Z => 0x02,
            OutAxis::Rx => 0x03,
            OutAxis::Ry => 0x04,
            OutAxis::Rz => 0x05,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum AxisMode {
    /// Output follows the cap position directly (self-centering).
    Direct,
    /// Output accumulates while the cap is deflected and holds on release
    /// (throttle / collective behavior).
    Integral,
}

impl AxisMode {
    pub fn label(self) -> &'static str {
        match self {
            AxisMode::Direct => "Direct (self-centering)",
            AxisMode::Integral => "Integral (holds value)",
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AxisMap {
    pub enabled: bool,
    pub dest: OutAxis,
    pub invert: bool,
    /// 0.0..0.9
    pub deadzone: f32,
    /// 0.0..0.95, out = (1-e)*x + e*x^3
    pub expo: f32,
    /// 0.0..4.0
    pub gain: f32,
    pub mode: AxisMode,
    /// Integral mode: output units per second at full deflection.
    pub rate: f32,
}

impl Default for AxisMap {
    fn default() -> Self {
        Self {
            enabled: true,
            dest: OutAxis::X,
            invert: false,
            deadzone: 0.05,
            expo: 0.2,
            gain: 1.0,
            mode: AxisMode::Direct,
            rate: 1.0,
        }
    }
}

/// Gamepad buttons the virtual device exposes.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum DestKey {
    South,
    East,
    North,
    West,
    TL,
    TR,
    TL2,
    TR2,
    Select,
    Start,
    ThumbL,
    ThumbR,
}

impl DestKey {
    pub const ALL: [DestKey; 12] = [
        DestKey::South,
        DestKey::East,
        DestKey::North,
        DestKey::West,
        DestKey::TL,
        DestKey::TR,
        DestKey::TL2,
        DestKey::TR2,
        DestKey::Select,
        DestKey::Start,
        DestKey::ThumbL,
        DestKey::ThumbR,
    ];

    /// Short label for compact indicators.
    pub fn short_label(self) -> &'static str {
        match self {
            DestKey::South => "A",
            DestKey::East => "B",
            DestKey::North => "Y",
            DestKey::West => "X",
            DestKey::TL => "L1",
            DestKey::TR => "R1",
            DestKey::TL2 => "L2",
            DestKey::TR2 => "R2",
            DestKey::Select => "Sl",
            DestKey::Start => "St",
            DestKey::ThumbL => "L3",
            DestKey::ThumbR => "R3",
        }
    }

    /// evdev key code.
    pub fn code(self) -> u16 {
        match self {
            DestKey::South => 0x130,  // BTN_SOUTH
            DestKey::East => 0x131,   // BTN_EAST
            DestKey::North => 0x133,  // BTN_NORTH
            DestKey::West => 0x134,   // BTN_WEST
            DestKey::TL => 0x136,
            DestKey::TR => 0x137,
            DestKey::TL2 => 0x138,
            DestKey::TR2 => 0x139,
            DestKey::Select => 0x13a,
            DestKey::Start => 0x13b,
            DestKey::ThumbL => 0x13d,
            DestKey::ThumbR => 0x13e,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DestKey::South => "A / Cross",
            DestKey::East => "B / Circle",
            DestKey::North => "Y / Triangle",
            DestKey::West => "X / Square",
            DestKey::TL => "L1",
            DestKey::TR => "R1",
            DestKey::TL2 => "L2 (btn)",
            DestKey::TR2 => "R2 (btn)",
            DestKey::Select => "Select/Back",
            DestKey::Start => "Start",
            DestKey::ThumbL => "L3",
            DestKey::ThumbR => "R3",
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ButtonMap {
    /// Source evdev key code, e.g. BTN_0 = 0x100.
    pub src: u16,
    pub dest: DestKey,
}

impl Default for ButtonMap {
    fn default() -> Self {
        Self {
            src: 0x100,
            dest: DestKey::South,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    /// Per input DOF (index 0..6).
    pub axes: Vec<AxisMap>,
    pub buttons: Vec<ButtonMap>,
}

impl Default for Profile {
    fn default() -> Self {
        // Default is a plain 1:1 pass-through: every DOF enabled, Direct
        // mode, identity mapping to the same-numbered output axis.
        let axes = (0..6)
            .map(|i| AxisMap {
                dest: OutAxis::ALL[i],
                expo: 0.0,
                ..AxisMap::default()
            })
            .collect();
        Self {
            name: "Default".into(),
            axes,
            buttons: vec![
                ButtonMap {
                    src: 0x100, // BTN_0 (Compact left)
                    dest: DestKey::South,
                },
                ButtonMap {
                    src: 0x101, // BTN_1 (Compact right)
                    dest: DestKey::East,
                },
            ],
        }
    }
}

/// Normalized input snapshot, all axes in -1.0..=1.0.
#[derive(Clone, Default)]
pub struct InputState {
    pub axes: [f32; 6],
    /// Currently-pressed source key codes.
    pub buttons: Vec<u16>,
    /// Most recently pressed source key code (for "press to assign" UI).
    pub last_key: Option<u16>,
    /// True when the source reports REL_* deltas instead of ABS_* positions
    /// (older Logitech-era 3Dconnexion devices).
    pub rel_mode: bool,
    /// Per-axis time of the last REL event; the pipeline decays an axis to
    /// zero after a silence window.
    pub rel_stamp: [Option<std::time::Instant>; 6],
}

/// Apply deadzone/expo/gain/invert to a normalized input value.
/// Used both by the realtime path and by the UI's response-curve preview.
pub fn shape(map: &AxisMap, raw: f32) -> f32 {
    if !map.enabled {
        return 0.0;
    }
    let mut x = raw.clamp(-1.0, 1.0);

    let dz = map.deadzone.clamp(0.0, 0.95);
    if x.abs() <= dz {
        x = 0.0;
    } else {
        x = (x.abs() - dz) / (1.0 - dz) * x.signum();
    }

    let e = map.expo.clamp(0.0, 0.95);
    x = x * (1.0 - e) + x * x * x * e;

    x *= map.gain.clamp(0.0, 8.0);
    if map.invert {
        x = -x;
    }
    x.clamp(-1.0, 1.0)
}

/// Apply one axis mapping. `acc` is the integrator accumulator for
/// Integral-mode axes; `dt` is the tick length in seconds.
pub fn apply_axis(map: &AxisMap, raw: f32, acc: &mut f32, dt: f32) -> f32 {
    let x = shape(map, raw);
    match map.mode {
        AxisMode::Direct => x,
        AxisMode::Integral => {
            *acc = (*acc + x * map.rate * dt).clamp(-1.0, 1.0);
            *acc
        }
    }
}
