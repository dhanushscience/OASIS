# oasis-voice examples

## Overview
Hand-check tools for the microphone and speech engine. `common/wav.rs` (`read_wav`: 16-bit PCM WAV to a 16 kHz mono `AudioClip`) is shared source for the examples.

## Interface
- `mic_check.rs`: opens a microphone for 1.5 s, prints how much audio arrived and how loud.
- `mic_formats.rs`: lists each input device's default and supported formats.
- `transcribe.rs`: `-- speech.wav <model folder> <dll folder>` prints the transcript.
- `bench_stt.rs`: speed, word error rate and peak memory of one model over a clips folder (`<name>.wav` plus `<name>.txt`); the last output line is JSON.

## Invariants
- `bench_stt` allows `unsafe` for one Windows call (peak memory).

## Side Effects
- Open the microphone or read WAV and model files; nothing goes over the network.

## Verification
- Run: `cargo build -p oasis-voice --examples`
