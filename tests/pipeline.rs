use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use space_controller::mapping::{InputState, Profile};
use space_controller::pipeline::{Pipeline, Source};

#[test]
fn demo_source_drives_virtual_pad() {
    let state = Arc::new(Mutex::new(InputState::default()));
    let preview = Arc::new(Mutex::new([0.0f32; 6]));
    let profile = Arc::new(RwLock::new(Profile::default()));

    let p = Pipeline::start(
        Source::Demo,
        state.clone(),
        profile,
        preview.clone(),
    )
    .expect("pipeline start");

    std::thread::sleep(Duration::from_millis(300));
    let out = *preview.lock().unwrap();
    p.stop();

    // The demo source is always moving on several axes; at least one
    // mapped output should be non-zero.
    assert!(
        out.iter().any(|v| v.abs() > 0.01),
        "expected non-zero output, got {out:?}"
    );
}
