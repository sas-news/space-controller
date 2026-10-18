use evdev::Device;
use std::env;

fn main() {
    let path = env::args().nth(1).expect("usage: dump <eventN path>");
    let mut dev = Device::open(&path).expect("open device");
    eprintln!("opened {}: {:?}", path, dev.name());
    loop {
        for ev in dev.fetch_events().expect("fetch") {
            if ev.value() != 0 {
                println!("type={} code={} value={}", ev.event_type().0, ev.code(), ev.value());
            }
        }
    }
}
