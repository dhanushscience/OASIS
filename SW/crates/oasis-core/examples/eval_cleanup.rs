//! Scores the rule-based dictation cleanup against a set of spoken inputs and the text a person
//! expects (`scripts/bench/cleanup/cases.md`: markdown tables of `| n | raw | expected |`, one table
//! per section). Prints a score per section and writes every miss to a file.
//!
//! `cargo run -p oasis-core --example eval_cleanup -- scripts/bench/cleanup/cases.md [misses.txt [rules.jsonl]]`
//! The optional third file gets one JSON line per case (section, raw, expected, rules) so another tool
//! can run its own step on the rule text (`scripts/bench/polish/eval_s1.py`).
//!
//! "exact" is the text as typed; "close" ignores capitals, punctuation and spacing, so it shows
//! whether the words are right even when a full stop is not.

use oasis_core::speech::pipeline;
use oasis_core::speech::{CleanupContext, CleanupRequest};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).map_or("scripts/bench/cleanup/cases.md", String::as_str);
    let misses_path = args.get(2).map_or("target/cleanup-misses.txt", String::as_str);
    let text = std::fs::read_to_string(path).expect("the cases file");

    let mut section = 0;
    let mut scores: Vec<(usize, usize, usize)> = Vec::new(); // cases, exact, close
    let mut spoken: Vec<(usize, usize)> = Vec::new(); // spoken-only cases, exact (V51)
    let mut edits: Vec<EditTally> = Vec::new();
    let mut misses = String::new();
    let mut dump = String::new();
    for line in text.lines() {
        let cells: Vec<&str> = line.trim().trim_matches('|').split(" | ").map(str::trim).collect();
        if cells.first().is_some_and(|c| c.contains('#')) {
            section += 1;
            scores.push((0, 0, 0));
            spoken.push((0, 0));
            edits.push(EditTally::default());
            continue;
        }
        if cells.len() < 3 || !cells[0].chars().all(|c| c.is_ascii_digit()) || cells[0].is_empty() {
            continue;
        }
        let (raw, expected) = (unescape(cells[1]), unescape(cells[2]));
        let got = pipeline::run(&CleanupRequest::new(&raw, CleanupContext::dictate(true))).corrected_text;
        dump.push_str(&format!("{{\"section\":{section},\"n\":{},\"raw\":{:?},\"expected\":{:?},\"rules\":{:?}}}
", cells[0], raw, expected, got));
        let tally = edit_tally(&raw, &expected, &got);
        if tally.deleted + tally.added > 0 {
            misses.push_str(&format!("{section}.{} [minimal-edit: {} deleted, {} added]\n", cells[0], tally.deleted, tally.added));
        }
        *edits.last_mut().expect("a table header first") = edits.last().expect("a table header first").plus(&tally);
        let score = scores.last_mut().expect("a table header first");
        score.0 += 1;
        let only_spoken = spoken_only(&raw, &expected);
        if only_spoken {
            let s = spoken.last_mut().expect("a table header first");
            s.0 += 1;
            s.1 += usize::from(got == expected);
        }
        if got == expected {
            score.1 += 1;
            score.2 += 1;
            continue;
        }
        if loose(&got) == loose(&expected) {
            score.2 += 1;
        }
        let tag = if only_spoken { " [spoken-only]" } else { "" };
        misses.push_str(&format!("{section}.{}{tag}\n  raw:    {raw}\n  expected: {expected}\n  got:      {got}\n\n", cells[0]));
    }

    let (mut all, mut exact, mut close) = (0, 0, 0);
    let (mut s_all, mut s_exact) = (0, 0);
    for (i, ((n, e, c), (sn, se))) in scores.iter().zip(&spoken).enumerate() {
        println!("section {}: {n} cases, {e} exact, {c} close; spoken-only {se}/{sn}", i + 1);
        all += n;
        exact += e;
        close += c;
        s_all += sn;
        s_exact += se;
    }
    println!("all: {all} cases, {exact} exact, {close} close; spoken-only {s_exact}/{s_all}");
    for (i, m) in edits.iter().enumerate() {
        println!("minimal-edit section {}: {}", i + 1, m.line());
    }
    println!("Minimal-Edit Score (all): {}", edits.iter().fold(EditTally::default(), |a, b| a.plus(b)).line());
    std::fs::write(misses_path, misses).expect("the misses file");
    if let Some(path) = args.get(3) {
        std::fs::write(path, dump).expect("the rules file");
    }
    println!("misses: {misses_path}");
}

/// Markdown escapes in the table: `\:`, `\#`, `\.`, `\-`, `**bold**`.
fn unescape(cell: &str) -> String {
    cell.replace('\\', "").replace("**", "")
}

/// The expected text uses only words already said (V51: no new wording). Digits count as said,
/// since number words become digits by rule.
fn spoken_only(raw: &str, expected: &str) -> bool {
    let said = loose(raw);
    let said: std::collections::HashSet<&str> = said.split(' ').collect();
    loose(expected).split(' ').all(|w| said.contains(w) || w.chars().all(|c| c.is_ascii_digit()))
}

/// The Minimal-Edit Score, kept apart from the exact score on purpose (owner, 2026-11-12): a cleanup
/// can write better text and still lose the speaker's words. Over loose words (counted, any order):
/// a word the expected text keeps from the raw speech and the output dropped is a wrong deletion; a
/// word in the output that neither the raw speech nor the expected text has is a wrong addition.
/// Words the expected text removes or adds are a matter for the exact score, not this one.
#[derive(Default, Clone, Copy)]
struct EditTally {
    cases: usize,
    untouched: usize,
    kept: usize,
    deleted: usize,
    added: usize,
}

impl EditTally {
    fn plus(&self, o: &Self) -> Self {
        Self {
            cases: self.cases + o.cases,
            untouched: self.untouched + o.untouched,
            kept: self.kept + o.kept,
            deleted: self.deleted + o.deleted,
            added: self.added + o.added,
        }
    }

    fn line(&self) -> String {
        let score = 100.0 * (1.0 - (self.deleted + self.added) as f64 / self.kept.max(1) as f64);
        format!(
            "{score:.1}% ({} of {} cases with no wrong edit; {} words wrongly deleted, {} wrongly added, of {} kept)",
            self.untouched, self.cases, self.deleted, self.added, self.kept
        )
    }
}

fn edit_tally(raw: &str, expected: &str, got: &str) -> EditTally {
    use std::collections::HashMap;
    let count = |t: &str| {
        let mut m: HashMap<String, usize> = HashMap::new();
        for w in loose(t).split(' ').filter(|w| !w.is_empty()) {
            *m.entry(w.to_string()).or_default() += 1;
        }
        m
    };
    let (r, e, g) = (count(raw), count(expected), count(got));
    let n = |m: &HashMap<String, usize>, w: &str| m.get(w).copied().unwrap_or(0);
    let (mut kept, mut deleted, mut added) = (0, 0, 0);
    for (w, &re) in &r {
        let k = re.min(n(&e, w));
        kept += k;
        deleted += k.saturating_sub(n(&g, w));
    }
    for (w, &gc) in &g {
        added += gc.saturating_sub(n(&r, w).max(n(&e, w)));
    }
    EditTally { cases: 1, untouched: usize::from(deleted + added == 0), kept, deleted, added }
}

fn loose(text: &str) -> String {
    text.to_lowercase().split(|c: char| !c.is_alphanumeric() && c != '\'').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}
