//! Optional language-model polish with a hard meaning-preservation reject. Port of
//! `TranscriptPolish.cs`.
//!
//! The model itself lives outside this crate: it is passed in as a function that takes the text to
//! polish, the raw transcript and the context, and returns the polished text or `None` when the
//! call failed, timed out or was cancelled.

use super::text::{collapse_whitespace_keep_lines, words};
use super::{layout, meaning, CleanupContext, CleanupResult, CorrectionPair};

/// Most lines polished one request each when the whole-text answer lost a line break.
const MAX_LINES_POLISHED_SEPARATELY: usize = 4;

/// Shorter lines ("Thanks", "2. Eggs") have no grammar worth a model call.
const MIN_WORDS_WORTH_POLISHING: usize = 5;

/// `(text, raw, context)` → polished text, or `None` if the model gave nothing usable.
pub type PolishFn<'a> = dyn FnMut(&str, &str, &CleanupContext) -> Option<String> + 'a;

pub fn maybe_polish(rules: CleanupResult, llm: &mut PolishFn<'_>, context: &CleanupContext) -> CleanupResult {
    if rules.corrected_text.trim().is_empty() {
        return rules;
    }
    // ")" or "Remove all the text" has no grammar to fix; asking a model anyway cost time and came
    // back unchanged.
    if words(&rules.corrected_text).len() < MIN_WORDS_WORTH_POLISHING && !rules.corrected_text.contains('\n') {
        return rules;
    }

    let mut polished = try_polish_text(&rules.corrected_text, true, &rules, llm, context);

    // A line break in the rule text is something the speaker asked for ("next line") or a list.
    // Models merge lines when they tidy a long paragraph; an answer with fewer lines has lost one.
    // The lines are then polished one at a time, so the break cannot be lost.
    if polished.as_deref().is_some_and(|p| line_count(p) < line_count(&rules.corrected_text)) {
        polished = polish_line_by_line(&rules, llm, context);
    }

    let Some(polished) = polished.filter(|p| *p != rules.corrected_text) else { return rules };

    let mut corrections = rules.corrections.clone();
    for pair in meaning::spelling_substitutions(&rules.raw_text, &polished) {
        // "two" → "2" is formatting, not a spelling to remember.
        if !pair.to.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            corrections.push(CorrectionPair { kind: "llm".into(), ..pair });
        }
    }
    CleanupResult { corrected_text: polished, corrections, used_llm: true, ..rules }
}

/// One model call; `None` when it fails, times out, or changes the meaning.
fn try_polish_text(
    text: &str,
    whole_text: bool,
    rules: &CleanupResult,
    llm: &mut PolishFn<'_>,
    context: &CleanupContext,
) -> Option<String> {
    let polished = llm(text, &rules.raw_text, context)?;
    if polished.trim().is_empty() {
        return None;
    }

    // The model is asked for lists and line breaks, so its lines are kept. The layout pass then
    // tidies what it left half-done: "1 buy milk" → "1. Buy milk".
    let polished = layout::apply(&collapse_whitespace_keep_lines(&polished), context);

    if whole_text {
        return (meaning::accepts(text, &polished) || meaning::accepts(&rules.raw_text, &polished)).then_some(polished);
    }

    // A single line that comes back longer has pulled in its neighbours from the model's
    // imagination; grammar fixes add a word or two at most.
    let grown = words(&polished).len() > max_polished_words(words(text).len());
    (!grown && meaning::accepts(text, &polished)).then_some(polished)
}

/// Most words a polished line may have. Grammar fixes add articles and a noun ("the SPI bus"): a
/// quarter more, and at least four ("make sure UART initialized before SPI starts" gains four).
pub fn max_polished_words(original: usize) -> usize {
    original + (original / 4).max(4)
}

fn polish_line_by_line(rules: &CleanupResult, llm: &mut PolishFn<'_>, context: &CleanupContext) -> Option<String> {
    let mut lines: Vec<String> = rules.corrected_text.split('\n').map(str::to_string).collect();
    let mut calls = 0;
    for slot in &mut lines {
        if calls >= MAX_LINES_POLISHED_SEPARATELY {
            break;
        }
        if words(slot).len() < MIN_WORDS_WORTH_POLISHING {
            continue;
        }
        calls += 1;
        if let Some(line) = try_polish_text(slot, false, rules, llm, context) {
            *slot = line;
        }
    }
    Some(lines.join("\n"))
}

fn line_count(text: &str) -> usize {
    text.matches('\n').count() + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speech::{pipeline, CleanupRequest};

    fn rules(raw: &str) -> (CleanupResult, CleanupContext) {
        let ctx = CleanupContext::dictate(true);
        (pipeline::run(&CleanupRequest::new(raw, ctx.clone())), ctx)
    }

    #[test]
    fn polish_keeps_the_lines_of_a_list() {
        let (r, ctx) = rules("one, buy the milk, two, call the bank");
        assert_eq!(r.corrected_text, "1. Buy the milk\n2. Call the bank");
        let out = maybe_polish(r, &mut |_, _, _| Some("1. Buy the fresh milk.\r\n2. Call the bank.".into()), &ctx);
        assert!(out.used_llm);
        assert_eq!(out.corrected_text, "1. Buy the fresh milk\n2. Call the bank");
    }

    #[test]
    fn polish_may_lay_out_a_list_the_rules_missed() {
        let (r, ctx) = rules("so the plan is to finish the video first and after that test the product");
        assert!(!r.corrected_text.contains('\n'));
        let out = maybe_polish(
            r,
            &mut |_, _, _| Some("So the plan is:\n1 finish the video\n2 test the product".into()),
            &ctx,
        );
        assert!(out.used_llm);
        // The lead-in of a list starts afresh: the model's "So" goes.
        assert_eq!(out.corrected_text, "The plan is:\n1. Finish the video\n2. Test the product");
    }

    #[test]
    fn polish_that_joins_a_spoken_line_break_is_rejected_then_retried_by_line() {
        let (r, ctx) = rules("this is the first part next line thanks for reading");
        assert_eq!(r.corrected_text, "This is the first part\nThanks for reading.");
        let out = maybe_polish(
            r.clone(),
            // Long enough to be past the growth limit of the first line (5 words + 4).
            &mut |_, _, _| Some("This is the first part. Thanks so much for reading all of this today.".into()),
            &ctx,
        );
        assert!(!out.used_llm);
        assert_eq!(out.corrected_text, r.corrected_text);
    }

    #[test]
    fn a_blank_line_before_a_list_is_removed() {
        let (r, ctx) = rules("the plan is to finish the video and then test the product");
        let out = maybe_polish(
            r,
            &mut |_, _, _| Some("The plan is:\n\n1. Finish the video\n2. Test the product".into()),
            &ctx,
        );
        assert_eq!(out.corrected_text, "The plan is:\n1. Finish the video\n2. Test the product");
    }

    #[test]
    fn growth_limit_is_a_quarter_and_at_least_four_words() {
        assert_eq!(max_polished_words(7), 11);
        assert_eq!(max_polished_words(40), 50);
    }

    #[test]
    fn short_text_and_failures_keep_the_rules() {
        let (r, ctx) = rules("remove all the text");
        let mut called = false;
        let out = maybe_polish(r.clone(), &mut |_, _, _| { called = true; None }, &ctx);
        assert!(!called && !out.used_llm);

        let (r, ctx) = rules("this sentence is long enough to be sent to a model");
        let out = maybe_polish(r.clone(), &mut |_, _, _| None, &ctx);
        assert_eq!(out.corrected_text, r.corrected_text);
        assert!(!out.used_llm);
    }
}
