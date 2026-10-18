# space-controller

Turn a 3Dconnexion SpaceMouse / SpaceNavigator into a virtual gamepad on Linux.

Reads the 6-DOF cap directly (hidraw or evdev) and emits a standard
uinput joystick device — "Space Controller Gamepad" — that games, SDL2,
Wine/Proton and the browser Gamepad API see as an ordinary controller.

## Features

- Live egui control panel: raw input bars → mapped output bars per DOF
- Per-axis mapping to gamepad axes (LX, LY, RX, RY, LT, RT) with
  deadzone, expo curve, gain, invert
- **Integral mode** per axis: deflect to move the value, release to hold —
  i.e. a throttle/collective that doesn't snap back to center
- Button remapping with press-to-assign
- TOML profiles in `~/.config/space-controller/profiles/`
- Demo input source for development without hardware
- Dual input backends:
  - `evdev` — newer devices (SpaceMouse Compact/Pro, vendor `0x256f`)
  - `hidraw` — required for old Logitech-era devices like the
    SpaceNavigator (`046d:c626`), whose reports the `hid-logitech`
    driver silently drops before they reach evdev

## Install

```sh
# udev rules: give the seat user access to the device and the virtual pad,
# and tag the virtual pad as a joystick
sudo cp 60-space-controller.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger

cargo build --release
```

## Usage

```sh
./target/release/space-controller
```

1. Pick your device in the source dropdown — prefer the `(hidraw)` entry
   for Logitech-era devices, `(evdev)` for newer ones.
2. Press **Start** — the virtual gamepad appears as `/dev/input/js*`.
3. Move the cap; `in` bars show raw input, `out` bars show the emitted
   gamepad axes.
4. In your game, select "Space Controller Gamepad" as the controller.

Verify output without a game:

```sh
jstest /dev/input/js0     # joyutils package
```

## Default flight-oriented mapping

| Cap motion          | Output axis  | Mode                    |
| ------------------- | ------------ | ----------------------- |
| Tilt fwd/back       | LY (pitch)   | Direct, inverted        |
| Tilt left/right     | LX (roll)    | Direct                  |
| Twist               | RX (yaw)     | Direct                  |
| Push fwd/back       | RY           | Direct                  |
| Push up/down        | RT           | **Integral** (throttle) |
| Push left/right     | —            | disabled                |
| BTN_0 / BTN_1       | A / B        |                         |

## Architecture

```
space mouse → hidraw/evdev reader → mapping engine (240 Hz) → uinput pad
                                    ↑ egui panel edits live
```

- `src/input.rs` — device discovery + readers (hidraw, evdev, demo)
- `src/mapping.rs` — profile model + axis shaping
- `src/output.rs` — uinput virtual gamepad
- `src/pipeline.rs` — fixed-rate output loop
- `src/app.rs` — egui control panel
- `examples/dump.rs` — raw event dump tool (`cargo run --example dump /dev/input/eventN`)

## Windows

Not yet. The core mapping layer is portable, but input/output would need
hidapi + ViGEmBus/vJoy ports. PRs welcome.

## License

MIT
