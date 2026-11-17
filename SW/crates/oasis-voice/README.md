# `crates/oasis-voice/` — microphone and on-device speech-to-text

## Overview

**What:** recording from a microphone, and turning the recording into text with a speech model on
this computer. Nothing is sent anywhere.

**Why a separate crate:** it is the only place that touches the audio device and the speech engine's
native code, so the rest of OASIS sees only the `SttEngine` trait (`oasis-models`) and the engine's
`VoiceMic` trait. Like `oasis-win`, it is allowed `unsafe` (D13), with a `SAFETY:` note on each use.

**When:** the first time push-to-talk is used. The speech model takes a few seconds to load, so it
loads on the first recording and stays loaded.

## Interface

**How:**
- `mic.rs`: `Microphone::new(name)`, `start()`, `stop()` → 16 kHz mono audio. The device is picked by
  a possibly shortened name (C# saved shortened names), else the default. If Windows refuses the
  device's default format, its other formats are tried (a Logitech webcam microphone needed this).
- `resample.rs`: stereo to mono, any rate to 16 kHz (averaging, so high tones do not fold into
  speech), a level meter.
- `sherpa.rs`: sherpa-onnx's C API loaded from `sherpa-onnx-c-api.dll` (D19). The C structs mirror
  `c-api.h` of **1.13.5**; the DLL must be that version. `detect_transducer` finds a model's files
  (int8 preferred on a CPU). `dll_search_folders` looks in `OASIS_SHERPA_DIR`, beside the program,
  and in a `native` folder beside it.

**Putting the speech engine in place:** `scripts\copy-speech-engine.ps1 -From <C# OASIS folder>`.

**Checking it by hand:**
- `cargo run -p oasis-voice --example mic_check -- "Microphone name"`: opens the microphone for
  1.5 s and prints how much audio arrived and how loud (saves nothing).
- `cargo run -p oasis-voice --example transcribe -- speech.wav <model folder> <dll folder>`: prints
  what the model hears in a 16-bit WAV file.
- With `OASIS_SHERPA_DIR` and `OASIS_STT_MODEL_DIR` set, `cargo test -p oasis-voice` also loads the
  real model and checks the struct layout.

| File | Contents |
|---|---|
| `src/mic.rs` | The microphone |
| `src/resample.rs` | Audio conversion and level |
| `src/sherpa.rs` | The speech engine and where its files are |
| `examples/` | `mic_check`, `mic_formats`, `transcribe`, `bench_stt` → [README](examples/README.md) |

Public symbols: `device_names`, `Microphone` (`new`, `start`, `stop`, `take`, `level`), `MODEL_RATE`,
`to_mono`, `to_model_rate`, `level`, `SherpaTransducer` (`load`, `load_model`), `TransducerLayout`,
`ModelLayout`, `detect_model`, `detect_transducer`, `dll_search_folders`, `find_dll_folder`,
`DLL_NAME`, `ONNX_RUNTIME_DLL`.

## Invariants

- Audio handed to the model is 16 kHz mono (`MODEL_RATE`).
- The sherpa-onnx DLL must match the C structs mirrored from `c-api.h` 1.13.5.
- `unsafe` carries a `SAFETY:` note (D13).

## Side Effects

- Opens the microphone; loads native DLLs at run time and keeps the model in memory; no network.

## Verification

- Run: `cargo test -p oasis-voice`
