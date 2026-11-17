//! Prints the text of the focused field of another program, three times a second apart, after a
//! pause to switch windows. Used to check what "learn from fixes" can see in an app:
//!
//!   cargo run -p oasis-win --example focused_text -- 6
use std::time::Duration;

fn main() {
    let wait: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(5);
    std::thread::sleep(Duration::from_secs(wait));
    let reader = oasis_win::uia::FieldReader::new();
    for n in 1..=3 {
        let started = std::time::Instant::now();
        println!("read {n}: {:?} ({} ms)", reader.focused_text(Duration::from_millis(2000)), started.elapsed().as_millis());
        std::thread::sleep(Duration::from_secs(1));
    }
}
