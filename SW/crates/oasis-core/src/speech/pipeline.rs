//! Ordered cleanup stages. Rules always produce a usable final; a language-model polish is a later
//! optional pass that [`meaning::accepts`](super::meaning::accepts) can reject. Port of
//! `TranscriptCleanupPipeline.cs`.

use super::text::collapse_whitespace;
use super::{
    filler, formatter, glossary, layout, meaning, numbers, punctuation, repair, repeat, revision, style, terms, CleanupContext,
    CleanupMode, CleanupRequest, CleanupResult, CorrectionPair,
};

/// The stages in the order they run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Repeat,
    SpokenForm,
    Revision,
    Filler,
    Glossary,
    Format,
}

/// Repeat, spoken form, revision, filler, glossary, format.
pub const DEFAULT_STAGES: [Stage; 6] =
    [Stage::Repeat, Stage::SpokenForm, Stage::Revision, Stage::Filler, Stage::Glossary, Stage::Format];

/// Run the default pipeline.
pub fn run(request: &CleanupRequest) -> CleanupResult {
    run_stages(request, &DEFAULT_STAGES)
}

pub fn run_stages(request: &CleanupRequest, stages: &[Stage]) -> CleanupResult {
    run_inner(request, stages, None)
}

/// The default pipeline, also recording the text after each stage as (stage name, text).
pub fn run_traced(request: &CleanupRequest, trace: &mut Vec<(String, String)>) -> CleanupResult {
    run_inner(request, &DEFAULT_STAGES, Some(trace))
}

fn run_inner(request: &CleanupRequest, stages: &[Stage], mut trace: Option<&mut Vec<(String, String)>>) -> CleanupResult {
    let mut note = |name: &str, text: &str| {
        if let Some(t) = trace.as_deref_mut() {
            t.push((name.to_string(), text.to_string()));
        }
    };
    let raw = request.raw_text.as_str();
    let dictate = request.context.mode == CleanupMode::Dictate;
    // Code editors and terminals: spoken punctuation, hesitation sounds out, the Dictionary.
    let exact = dictate && request.context.style == style::Style::Exact;
    let stages = if exact { &[Stage::SpokenForm, Stage::Filler, Stage::Glossary][..] } else { stages };
    let mut current = collapse_whitespace(raw);
    let mut corrections: Vec<CorrectionPair> = Vec::new();
    for stage in stages {
        current = apply_stage(*stage, &current, &request.context, &mut corrections);
        current = collapse_whitespace(&current);
        note(&format!("{stage:?}"), &current);
    }

    // Last, and outside the stage loop: it is the one step that adds line breaks, which the
    // whitespace collapse between stages would flatten again.
    if !exact {
        current = layout::apply(&current, &request.context);
        note("Layout", &current);
    }
    if dictate && request.context.apply_formatting && request.context.style == style::Style::Message {
        current = style::for_message(&current);
        note("ChatStyle", &current);
    }

    for pair in meaning::spelling_substitutions(raw, &current) {
        // "two" → "2" is a rule, not a spelling the user taught; it must not reach the glossary.
        if pair.to.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        let known = corrections
            .iter()
            .any(|c| c.from.to_lowercase() == pair.from.to_lowercase() && c.to.to_lowercase() == pair.to.to_lowercase());
        if !known {
            corrections.push(pair);
        }
    }

    CleanupResult { corrected_text: current, raw_text: raw.to_string(), corrections, used_llm: false }
}

fn apply_stage(stage: Stage, text: &str, context: &CleanupContext, corrections: &mut Vec<CorrectionPair>) -> String {
    let dictate = context.mode == CleanupMode::Dictate;
    match stage {
        Stage::Repeat => {
            if dictate { repeat::collapse_dictation(text) } else { repeat::collapse(text) }
        }
        // Dictation only: spoken punctuation, then number words to digits. Before the revision
        // stage, which lines corrections up by digits and sentence ends. Commands keep their words.
        Stage::SpokenForm => match (dictate, context.style) {
            (true, style::Style::Exact) => punctuation::apply(text),
            (true, _) => numbers::apply(&punctuation::apply(text)),
            _ => text.to_string(),
        },
        Stage::Revision => {
            let repaired = if dictate { repair::apply(text) } else { text.to_string() };
            let mut next = revision::apply_for(&repaired, context.mode);
            if !text.eq_ignore_ascii_case(&next) && !text.is_empty() && !next.is_empty() {
                corrections.push(CorrectionPair::new(text, &next, "revision"));
                // The formatter leaves under 3 words unended ("Ok"); a sentence a revision cut that
                // short ("Use ESP32, actually nRF52" -> "Use nRF52") was a full one and keeps its stop.
                if dictate && context.apply_formatting && super::text::words(text).len() >= 3 && !next.ends_with(['.', '!', '?', ':', ',']) {
                    next.push('.');
                }
            }
            next
        }
        // Fillers gone can leave a restart behind ("I uh I think"): repeats are looked at again.
        Stage::Filler if dictate && context.style == style::Style::Exact => filler::hesitations(text),
        Stage::Filler if dictate =>repeat::collapse_dictation(&filler::apply(text, context.mode)),
        Stage::Filler => filler::apply(text, context.mode),
        Stage::Glossary => {
            let (next, applied) = glossary::apply(text, &context.glossary);
            corrections.extend(applied);
            // Dictation, and not in code (exact style): names heard a letter off take the
            // Dictionary's spelling (V41), then the built-in technical words.
            if dictate && context.style != style::Style::Exact { terms::apply(&glossary::near(&next, &context.glossary)) } else { next }
        }
        Stage::Format => formatter::apply(text, context),
    }
}

/// The rule-based corrector: the default pipeline.
pub fn correct(request: &CleanupRequest) -> CleanupResult {
    run(request)
}

/// The text with only its whitespace tidied: no filler, revision or glossary step.
pub fn identity(request: &CleanupRequest) -> CleanupResult {
    CleanupResult {
        corrected_text: collapse_whitespace(&request.raw_text),
        raw_text: request.raw_text.clone(),
        corrections: Vec::new(),
        used_llm: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speech::GlossaryEntry;

    fn dictate(raw: &str) -> String {
        dictate_fmt(raw, true)
    }

    fn dictate_fmt(raw: &str, format: bool) -> String {
        run(&CleanupRequest::new(raw, CleanupContext::dictate(format))).corrected_text
    }

    fn command(raw: &str) -> String {
        run(&CleanupRequest::new(raw, CleanupContext::command())).corrected_text
    }

    #[test]
    fn commands_are_cleaned_but_keep_their_words() {
        assert_eq!(command("um I think we should um go"), "I think we should go");
        assert_eq!(command("please open open Spotify"), "open Spotify");
        assert_eq!(command("open chrome actually open firefox"), "open firefox");
        assert_eq!(command("open spot i fi"), "open Spotify");
        assert_eq!(command("Oasis open notepad"), "Oasis open notepad");
        assert_eq!(command("set brightness seventy five"), "set brightness seventy five");
    }

    #[test]
    fn glossary_applies_domain_terms() {
        let mut ctx = CleanupContext::command();
        ctx.glossary = vec![GlossaryEntry::new("kicad", "KiCad")];
        let result = run(&CleanupRequest::new("open kicad please", ctx));
        assert_eq!(result.corrected_text, "open KiCad");
    }

    #[test]
    fn a_name_in_the_dictionary_is_fixed_in_dictation_only() {
        let mut ctx = CleanupContext::dictate(true);
        ctx.glossary = vec![GlossaryEntry::new("Shashank", "Shashank")];
        assert_eq!(run(&CleanupRequest::new("send the mail to Chashank today", ctx.clone())).corrected_text, "Send the mail to Shashank today.");
        ctx.mode = CleanupMode::Command;
        assert_eq!(run(&CleanupRequest::new("call Chashank", ctx)).corrected_text, "call Chashank");
    }

    #[test]
    fn dictation_is_capitalized_ended_and_converted() {
        assert_eq!(dictate("hello comma how are you question mark"), "Hello, how are you?");
        assert_eq!(dictate("send it today full stop then call me"), "Send it today. Then call me.");
        assert_eq!(dictate("please call nine nine eight"), "Please call 998.");
        assert_eq!(dictate("meet at 5 actually let's do lunch"), "Meet at 5 let's do lunch."); // "actually" is a filler anywhere (V45)
        assert_eq!(dictate("what is the rating what is the rating of it"), "What is the rating of it?");
        assert_eq!(
            dictate("And and it was not able to detect user user"),
            "And it was not able to detect user."
        );
        assert_eq!(dictate("thank you thank you so much"), "Thank you thank you so much.");
    }

    #[test]
    fn corrections_in_dictation() {
        assert_eq!(
            dictate("Let's meet on Tuesday two pm. No, actually Wednesday three p.m."),
            "Let's meet on Wednesday 3 p.m."
        );
        assert_eq!(dictate("send it to John, no, actually send it to Mark"), "Send it to Mark.");
        assert_eq!(dictate("It is at four thirty, no sorry, five thirty."), "It is at 5:30.");
        assert_eq!(dictate("We need two, no, three chairs."), "We need three chairs.");
        assert_eq!(dictate("I have two kids, I mean three"), "I have three kids.");
        assert_eq!(dictate("No, actually it was fine."), "No, it was fine.");
        assert_eq!(dictate("Use ESP32, actually nRF52"), "Use nRF52."); // a sentence cut short by a revision keeps its stop
        assert_eq!(dictate("ok"), "Ok", "a bare short dictation stays unpunctuated");
        assert_eq!(dictate("Please use ESP32, actually nRF52"), "Please use nRF52.");
        assert_eq!(dictate("I actually tested it"), "I actually tested it.");
    }

    #[test]
    fn lists_are_laid_out_only_on_a_final() {
        assert_eq!(dictate_fmt("one, milk, two, eggs", false), "one, milk, two, eggs");
        assert_eq!(dictate("one, milk, two, eggs"), "1. Milk\n2. Eggs");
        assert_eq!(dictate("hello there new line how are you"), "Hello there\nHow are you.");
        assert_eq!(
            dictate("first topic done. New paragraph. Second topic starts"),
            "First topic done.\n\nSecond topic starts."
        );
        assert_eq!(dictate("add a new line of code here"), "Add a new line of code here.");
    }

    #[test]
    fn a_converted_number_is_not_learned_as_a_spelling() {
        let result = run(&CleanupRequest::new("continue with phase two", CleanupContext::dictate(true)));
        assert!(result.corrections.iter().all(|c| c.to != "2"));
    }

    #[test]
    fn the_owners_examples_come_out_right_without_a_model() {
        assert_eq!(
            dictate("Can you send the draft by Friday... actually, wait, let's make that next Monday."),
            "Can you send the draft by next Monday?"
        );
        assert_eq!(
            dictate("The three things I need to complete are designing the PCB, writing the firmware, and testing the power consumption."),
            "The three things I need to complete are:
1. Designing the PCB
2. Writing the firmware
3. Testing the power consumption"
        );
        assert_eq!(dictate("is this ready for review"), "Is this ready for review?");
        assert_eq!(dictate("Uh uh uh I think I I I think uh that was good. Okay."), "I think that was good.");
        assert_eq!(dictate("uh uh uh i think i i i think uh that was good okay"), "I think that was good okay.");
    }

    #[test]
    fn the_app_in_front_sets_the_style() {
        let styled = |raw: &str, style: style::Style| {
            let mut context = CleanupContext::dictate(true);
            context.style = style;
            context.glossary = vec![GlossaryEntry::new("kay cad", "KiCad")];
            run(&CleanupRequest::new(raw, context)).corrected_text
        };
        assert_eq!(styled("sounds good see you there", style::Style::Message), "Sounds good see you there");
        assert_eq!(styled("sounds good see you there", style::Style::Mail), "Sounds good see you there.");
        // Code: the Dictionary, spoken punctuation and no hesitations; no capitals, numbers or full stop.
        assert_eq!(styled("um open kay cad two times comma", style::Style::Exact), "open KiCad two times,");
        assert_eq!(styled("Uh now it is working good I guess. Um ", style::Style::Exact), "Now it is working good I guess.");
        assert_eq!(styled("Uh now it is working good I guess. Um ", style::Style::Default), "Now it is working good I guess.");
        // A command keeps its words whatever the app.
        let mut command = CleanupContext::command();
        command.style = style::Style::Message;
        assert_eq!(run(&CleanupRequest::new("open chrome", command)).corrected_text, "open chrome");
    }

    #[test]
    fn code_editor_gets_default_rules_but_a_terminal_stays_exact() {
        let raw = "so um i wanted to to explain the the problem";
        let in_app = |app: &str| {
            let mut context = CleanupContext::dictate(true);
            context.style = style::for_app(app, "");
            run(&CleanupRequest::new(raw, context)).corrected_text
        };
        assert_eq!(in_app("Code"), "I wanted to explain the problem.");
        assert_ne!(in_app("cmd"), "I wanted to explain the problem.");
        assert!(in_app("cmd").starts_with("so "), "{}", in_app("cmd"));
    }

    #[test]
    fn identity_only_tidies_whitespace() {
        let result = identity(&CleanupRequest::new("  hi   there ", CleanupContext::command()));
        assert_eq!(result.corrected_text, "hi there");
    }
}
