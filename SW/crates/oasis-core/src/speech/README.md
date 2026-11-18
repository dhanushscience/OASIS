# `oasis-core/src/speech/` — cleaning up what was said

## Overview
**What:** the rules that turn the raw words the speech model heard into the text you wanted: spoken
punctuation, numbers, self-corrections, fillers, your own spellings, capitals, lists and line
breaks. Port of `Oasis.Core/Speech`, plus modules written new for the Rust app (`fragments`, `terms`, `style`, `fixes`).

**Why:** a speech model writes "um send it today comma then call me at five fifteen". You meant
"Send it today, then call me at 5:15." Each rule is small and has a "stays as spoken" side,
because the same words are ordinary prose far more often ("put a comma there", "this one", "wait
for me").

**When:** after the microphone is released and the model has produced text. In the engine,
`type_dictation` runs the pipeline before typing; `Dictate` mode is for typing, `Command` mode is
lighter (no capitals, no numbers) so the spoken-command router still sees plain words.

**How:** `pipeline::run` runs six stages in order (`Stage`, `DEFAULT_STAGES`), each followed by a whitespace tidy:
repeats → spoken punctuation + numbers (dictation only) → corrections (`repair` for dictation, then `revision`) → fillers → glossary (then near-spelling and technical `terms`, dictation only) → capitals/full stop.
Layout (`new line`, numbered lists) runs after the stages, outside the loop, because the whitespace tidy would flatten its line breaks. Chat style (`Style::Message`) then drops the closing full stop of a one-sentence message.
In the `Exact` style (terminals) only spoken punctuation, hesitation sounds and the glossary run: no capitals, numbers, corrections or layout.
Finally the pipeline lists spelling changes it made as `corrections` (never a change to a digit, so "two" -> "2" cannot reach the dictionary).
Pure: strings in, strings out, no clock, files or network.

## Interface
Types in `mod.rs`: `CleanupMode` (`Command`, `Dictate`), `GlossaryEntry`, `CorrectionPair`, `CleanupContext` (mode, `style::Style`, glossary, `apply_formatting`; `command()` and `dictate(bool)` constructors), `CleanupRequest`, `CleanupResult` (`corrected_text`, `raw_text`, `corrections`, `used_llm`). Entry points in `pipeline`: `run`, `run_stages`, `run_traced` (records the text after each stage), `correct` (same as `run`), `identity` (whitespace tidy only). Every submodule is `pub`.

| File | Does |
|---|---|
| `text.rs` | Words, tokens, whitespace, trailing punctuation, fuzzy match |
| `punctuation.rs` | "comma", "full stop", brackets, quotes → symbols |
| `numbers.rs` | "seventy five percent" → "75%", years, clock times, decimals |
| `repair.rs`, `revision.rs` | "Tuesday, no actually Wednesday" (dictation); "scratch that", "actually 6pm", command restarts |
| `filler.rs`, `repeat.rs` | "um", "please", stutters and decode loops |
| `glossary.rs` | Your spellings plus built-in "Spotify" and "Oasis" fixes |
| `formatter.rs`, `layout.rs` | Capitals, closing full stop; "new line", numbered lists |
| `meaning.rs` | Rejects a rewrite that invents a command or loses text |
| `pipeline.rs` | Runs the stages; returns the text and what changed |
| `polish.rs`, `polish_prompt.rs` | Optional language-model polish (`maybe_polish`; the model is passed in as a `PolishFn` closure) and the prompt text and limits both engines share |
| `vocabulary.rs` | Learns a spelling after it has been corrected twice |
| `fragments.rs` | `whole_phrase`: a fragment said on its own ("insert my LinkedIn") types the stored text exactly |
| `terms.rs` | Technical words as written: UART, 3.3 V, 32-bit (dictation only, never in code editors) |
| `style.rs` | Dictation style per app in front: chat, mail, notes, exact (code) |
| `fixes.rs` | Finds words the person retyped right after dictation, for their dictionary |
| `audio.rs` | Noise gate (16 kHz mono 16-bit bytes) and silence / valley / pause cut points (`segment_cut`, `command_cut`) |

Which of these the app calls (`pipeline`, `style`, `terms`, `fragments`, `polish`, `vocabulary`, `fixes`, `audio`) is decided by `oasis-app`, not here; this crate only holds the rules. There is no `overlap` or `incremental` module in this folder.

## Invariants
- Pure: strings in, strings out; no clock, files or network. The polish step receives the model as a function.
- `Command` mode is lighter (no capitals, no numbers) so the command router still sees plain words.
- `meaning.rs` rejects a rewrite that invents a command or loses text.
- Style never changes the words said and never decides command versus text (`style.rs`).
- `terms` and near-spelling glossary matching run only in dictation, never in the `Exact` style.
- `polish::maybe_polish` skips text under five words and never lets a rejected rewrite through.
- Layout runs last and only on a final.

## Side Effects
- None. `punctuation.rs` builds its command table once in a `OnceLock`; nothing else keeps state.

## Verification
- Run: `cargo test -p oasis-core`
