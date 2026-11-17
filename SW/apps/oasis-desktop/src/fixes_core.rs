//! The loop behind "learn from my fixes", with no window code in it: it is given a way to read the
//! focused field, to ask which window is in front, and to say a fix was found. That keeps it
//! testable (see the tests here and `examples/fixes_probe.rs`).

use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub struct Timing {
    /// How long after typing a fix is still looked for.
    pub watch_for: Duration,
    /// The field must sit unchanged this long before it is compared (the person may still be typing).
    pub settled: Duration,
    pub tick: Duration,
}

pub const REAL: Timing = Timing {
    watch_for: Duration::from_secs(60),
    settled: Duration::from_secs(2),
    tick: Duration::from_millis(700),
};

/// Watch the field `dictated` was just typed into. Calls `offer` with the `(heard, written)` pairs
/// each time the person has retyped misheard words, and goes on watching until the time is up: a
/// second word fixed later in the same text is offered too, and a pair is never offered twice.
pub fn run(
    dictated: &str,
    timing: Timing,
    alive: &dyn Fn() -> bool,
    read: &dyn Fn() -> Option<String>,
    foreground: &dyn Fn() -> i64,
    offer: &mut dyn FnMut(Vec<(String, String)>),
    note: &dyn Fn(&str),
) {
    let dictated = dictated.replace("\r\n", "\n");
    let dictated = dictated.trim();
    // The paste goes through the action queue; give it a moment to land.
    let mut before = None;
    for _ in 0..4 {
        thread::sleep(timing.tick);
        before = read().filter(|t| t.contains(dictated));
        if before.is_some() {
            break;
        }
    }
    let Some(before) = before else {
        note("the typed text could not be read back from the field, so nothing is watched (this program may not share its text)");
        return;
    };
    note("watching the field for a fix");
    let window = foreground();

    let started = Instant::now();
    let (mut last, mut since) = (before.clone(), Instant::now());
    let mut checked: Option<String> = None; // the text last compared, so it is judged once
    let mut offered: Vec<(String, String)> = Vec::new();
    while alive() && started.elapsed() < timing.watch_for {
        thread::sleep(timing.tick);
        if foreground() != window {
            continue; // looking at another program for now
        }
        let Some(now) = read() else { continue };
        if now != last {
            (last, since) = (now, Instant::now());
            continue;
        }
        if last == before || since.elapsed() < timing.settled || checked.as_ref() == Some(&last) {
            continue;
        }
        // Always against the text as first typed: the words fixed earlier are still among the changes.
        let fixes: Vec<_> = oasis_core::speech::fixes::fixed_words(&before, &last, dictated)
            .into_iter()
            .filter(|f| !offered.contains(f))
            .collect();
        checked = Some(last.clone());
        if fixes.is_empty() {
            if offered.is_empty() {
                note("the field was changed, but not in a way that looks like a misheard word");
            }
            continue;
        }
        if alive() {
            note(&format!("{} fixed word(s) offered", fixes.len()));
            offered.extend(fixes.iter().cloned());
            offer(fixes);
        }
    }
    note(if offered.is_empty() { "no fix seen, stopped watching" } else { "stopped watching" });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const FAST: Timing = Timing {
        watch_for: Duration::from_secs(3),
        settled: Duration::from_millis(60),
        tick: Duration::from_millis(20),
    };

    /// The field shows each of `texts` in turn (the last one for ever).
    fn watch(dictated: &str, texts: &[&str]) -> Vec<(String, String)> {
        let step = RefCell::new(0usize);
        let read = || {
            let mut i = step.borrow_mut();
            let t = texts[(*i).min(texts.len() - 1)].to_string();
            *i += 1;
            Some(t)
        };
        let mut found = Vec::new();
        run(
            dictated,
            FAST,
            &|| true,
            &read,
            &|| 7,
            &mut |f| found = f,
            &|_| {},
        );
        found
    }

    #[test]
    fn a_word_retyped_after_the_paste_is_offered() {
        let d = "Open keycat and the board.";
        let found = watch(
            d,
            &[
                &format!("{d} "),
                &format!("{d} "),
                "Open K and the board. ",
                "Open KiCad and the board. ",
            ],
        );
        assert_eq!(found, [("keycat".to_string(), "KiCad".to_string())]);
    }

    #[test]
    fn a_second_word_fixed_later_is_offered_too() {
        let d = "Open keycat and the bored.";
        let offers = RefCell::new(Vec::new());
        let step = RefCell::new(0usize);
        let texts = ["Open keycat and the bored. ", "Open KiCad and the bored. ", "Open KiCad and the board. "];
        let read = || {
            let mut i = step.borrow_mut();
            let t = texts[(*i / 12).min(texts.len() - 1)].to_string(); // each text stays for 12 reads
            *i += 1;
            Some(t)
        };
        run(d, FAST, &|| true, &read, &|| 7, &mut |f| offers.borrow_mut().push(f), &|_| {});
        let offers = offers.into_inner();
        assert_eq!(offers.len(), 2, "{offers:?}");
        assert_eq!(offers[0], [("keycat".to_string(), "KiCad".to_string())]);
        assert_eq!(offers[1], [("bored".to_string(), "board".to_string())], "the first pair is not offered again");
    }

    #[test]
    fn nothing_is_offered_when_the_text_is_left_alone() {
        let d = "Open keycat and the board.";
        assert!(watch(d, &[&format!("{d} ")]).is_empty());
    }

    #[test]
    fn a_field_that_never_holds_the_text_ends_the_watch() {
        assert!(watch("hello there", &["something else"]).is_empty());
    }
}
