//! Runs the "learn from my fixes" loop against a real window, to see what it does in an app:
//! type the dictated sentence into the app, then correct a word by hand.
//!
//!   cargo run -p oasis-desktop --example fixes_probe -- "Open keycat and the board."
#[path = "../src/fixes_core.rs"]
mod fixes_core;

use std::time::Duration;

fn main() {
    let dictated = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Open keycat and the board.".into());
    let reader = oasis_win::uia::FieldReader::new();
    let read = || {
        reader
            .focused_text(Duration::from_millis(800))
            .map(|t| t.replace("\r\n", "\n").replace('\r', "\n"))
    };
    println!("watching for: {dictated}");
    fixes_core::run(
        &dictated,
        fixes_core::REAL,
        &|| true,
        &read,
        &oasis_win::desktop::foreground,
        &mut |f| println!("OFFER: {f:?}"),
        &|n| println!("log: {n}"),
    );
    println!("done");
}
