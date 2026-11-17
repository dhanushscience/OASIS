# `oasis-core/src/speech/` — cleaning up what was said

## Overview
**What:** the rules that turn the raw words the speech model heard into the text you wanted: spoken
punctuation, numbers, self-corrections, fillers, your own spellings, capitals, lists and line
breaks. Port of `Oasis.Core/Speech` (all 21 C# files).

**Why:** a speech model writes "um send it today comma then call me at five fifteen". You meant
"Send it today, then call me at 5:15." Each rule is small and has a "stays as spoken" side,
because the same words are ordinary prose far more often ("put a comma there", "this one", "wait
for me").

**When:** after the microphone is released and the model has produced text. In the engine,
`type_dictation` runs the pipeline before typing; `Dictate` mode is for typing, `Command` mode is
lighter (no capitals, no numbers) so the spoken-command router still sees plain words.

**How:** `pipeline::run` runs these in order, each followed by a whitespace tidy:
repeats → spoken punctuation + numbers → corrections (`repair`, `revision`) → fillers → glossary →
capitals/full stop. Layout (`new line`, numbered lists) runs last and only on a final.
Everything is pure: strings in, strings out, no clock, files or network.

## Interface
Types in `mod.rs`: `CleanupMode`, `GlossaryEntry`, `CorrectionPair`, `CleanupContext`, `CleanupRequest`, `CleanupResult`. Entry point: `pipeline::run`.

| File | Does |
|---|---|
| `text.rs` | Words, tokens, whitespace, trailing punctuation, fuzzy match |
| `punctuation.rs` | "comma", "full stop", brackets, quotes → symbols |
| `numbers.rs` | "seventy five percent" → "75%", years, clock times, decimals |
| `repair.rs`, `revision.rs` | "Tuesday, no actually Wednesday", "scratch that" |
| `filler.rs`, `repeat.rs` | "um", "please", stutters and decode loops |
| `glossary.rs` | Your spellings plus built-in "Spotify" and "Oasis" fixes |
| `formatter.rs`, `layout.rs` | Capitals, closing full stop; "new line", numbered lists |
| `meaning.rs` | Rejects a rewrite that invents a command or loses text |
| `pipeline.rs` | Runs the stages; returns the text and what changed |
| `polish.rs`, `polish_prompt.rs` | Optional language-model polish (model passed in as a function) |
| `vocabulary.rs` | Learns a spelling after it has been corrected twice |
| `fragments.rs` | A fragment said on its own ("insert my LinkedIn") types the stored text exactly |
| `terms.rs` | Technical words as written: UART, 3.3 V, 32-bit (dictation only, never in code editors) |
| `style.rs` | Dictation style per app in front: chat, mail, notes, exact (code) |
| `fixes.rs` | Finds words the person retyped right after dictation, for their dictionary |
| `audio.rs` | Noise gate and silence cut points |

**Used today:** `pipeline` (dictation, pasted once after you finish speaking, with the glossary and `DictationCleanup` from
`profiles.json`). **Built, connection status not verified here:** `vocabulary` (learning), `audio` (noise gate and voice-activity cuts). The earlier `overlap` and `incremental` modules do not exist in this folder.

## Invariants
- Pure: strings in, strings out; no clock, files or network. The polish step receives the model as a function.
- `Command` mode is lighter (no capitals, no numbers) so the command router still sees plain words.
- `meaning.rs` rejects a rewrite that invents a command or loses text.
- Style never changes the words said and never decides command versus text (`style.rs`).
- Layout runs last and only on a final.

## Side Effects
- None.

## Verification
- Run: `cargo test -p oasis-core`
