# oasis-voice examples

## Overview
Hand-check and bench tools for the microphone and speech engine. `common/wav.rs` (`read_wav`: 16-bit PCM WAV to a 16 kHz mono `AudioClip`) is shared source: each example that reads WAV files pulls it in with `#[path = "common/wav.rs"]`.

## Interface
Run any of them with `cargo run [--release] -p oasis-voice --example <name> -- <args>`.

| File | Arguments and what it does |
|---|---|
| `mic_check.rs` | `[device name]`. Opens the microphone for 1.5 s, prints how much audio arrived and how loud. Saves nothing. |
| `mic_formats.rs` | none. Lists each input device's default and supported formats (for "format not supported"). |
| `transcribe.rs` | `<wav> <model folder> <dll folder>`. Prints the transcript. |
| `bench_stt.rs` | `<model folder> <dll folder> <clips folder> [threads] [runs]`. Speed, word error rate and peak memory of one model over `<name>.wav` plus `<name>.txt` pairs; the last output line is JSON. |
| `trim_ab.rs` | `<model folder> <dll folder> <folder>... [--threads N]`. A/B of trimming quiet before decoding (whole clip against `trim_to_speech`); reference text from `<name>.txt` or the `text` field of `<name>.json`. |
| `replay_pauses.rs` | `<model folder> <dll folder> <history folder> [threads]`. Replays recorded dictations through `cut_at_pause` at several pause settings; reports false sentence breaks, pieces per clip and the wait after release. Holds a copy of `join_dictation_pieces` from `oasis-engine`, which this crate cannot reach. |
| `common/wav.rs` | `read_wav(path)`; not an example by itself. |

`trim_ab` and `replay_pauses` use `oasis-core` (a dev-dependency); the others use only this crate and `oasis-models`.

## Invariants
- `bench_stt` allows `unsafe` for one Windows call (peak memory).
- Model folders are loaded with `detect_model`, on the CPU provider.
- `read_wav` accepts only 16-bit PCM.

## Side Effects
- `mic_check` opens the microphone; the rest read WAV, text and model files. Nothing is written and nothing goes over the network. `mic_check` starts the crate's microphone thread (see the crate README); the others run single-threaded apart from the decoder's own threads.

## Verification
- Run: `cargo build -p oasis-voice --examples`
