//! Dictation quality against Wispr Flow (`docs/VOICE_DECISIONS.md` V38): the rules run over each
//! OASIS take of the owner's sample, and the lines of Wispr's text they reach are counted.
//!
//! `cargo test -p oasis-core --test quality -- --nocapture` prints every target line and what the
//! rules gave. The lines reached today are pinned, so a rule change cannot lose one unnoticed.

use oasis_core::speech::{layout, pipeline};
use oasis_core::speech::{CleanupContext, CleanupRequest};

const TARGET: &str = include_str!("fixtures/quality/wispr.txt");
const TAKES: [(&str, &str); 2] = [
    ("take_new", include_str!("fixtures/quality/take_new.txt")),
    ("take_s1", include_str!("fixtures/quality/take_s1.txt")),
];
/// Target lines (counted from 1) every take reaches with rules alone. Lines 1 and 6 need a model:
/// joined sentences, "a mail" → "an email", and a name the speech model misheard.
const REACHED: [usize; 4] = [2, 3, 4, 5];

#[test]
fn the_rules_reach_the_pinned_lines_of_wisprs_text() {
    for (name, take) in TAKES {
        // A take a model already laid out is cleaned line by line and then laid out as a whole, as
        // polish output is in the app; its list lines go to the layout as they are.
        let context = CleanupContext::dictate(true);
        let cleaned: Vec<String> = take
            .trim()
            .lines()
            .map(|l| if l.starts_with(|c: char| c.is_ascii_digit()) { l.to_string() } else { pipeline::run(&CleanupRequest::new(l, context.clone())).corrected_text })
            .collect();
        let got = layout::apply(&cleaned.join("
"), &context);
        let lines: Vec<&str> = got.lines().collect();
        let mut reached = Vec::new();
        println!("\n== {name} ==");
        for (i, want) in TARGET.lines().enumerate() {
            let hit = lines.contains(&want);
            println!("{} {}: {want}", if hit { "ok  " } else { "MISS" }, i + 1);
            if hit {
                reached.push(i + 1);
            }
        }
        // Kept apart from the Minimal-Edit Score (`examples/eval_cleanup.rs`) on purpose.
        println!("-- the rules gave --\n{got}\n-- Wispr Fixture Score: {} of {} lines", reached.len(), TARGET.lines().count());
        assert!(REACHED.iter().all(|n| reached.contains(n)), "{name}: reached {reached:?}, pinned {REACHED:?}");
    }
}
