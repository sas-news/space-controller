use space_controller::output::{PadIdentity, VirtualPad, PAD_NAME};

#[test]
fn creates_and_emits_virtual_gamepad() {
    let mut pad = VirtualPad::create(PadIdentity::Generic).expect("create virtual pad");
    pad.emit(&[0.0; 6], &[]).unwrap();
    pad.emit(&[1.0, -1.0, 0.5, -0.5, 0.25, -0.25], &[0x130])
        .unwrap();

    let devices = std::fs::read_to_string("/proc/bus/input/devices").unwrap();
    assert!(devices.contains(PAD_NAME), "virtual pad not registered");
}
