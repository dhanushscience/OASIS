# bench

## Overview

Benchmarks for the voice pipeline: speech-to-text speed on pinned CPUs, and cleanup/polish quality and latency.

## Interface

- `stt-run.cmd <affinity hex> <model folder> <threads> <runs>`: runs `target\release\examples\bench_stt.exe` pinned with `start /affinity`, against clips in `%LOCALAPPDATA%\ProjectOASIS\bench\clips`. Build first: `cargo build --release -p oasis-voice --example bench_stt`.
- [clips](./clips/README.md): synthetic test audio.
- [polish](./polish/README.md): llama-server cleanup benchmarks.
- `cleanup/cases.md`: markdown table of raw speech and expected output (self-correction cases); data only, read by the `eval_cleanup` example and `polish/eval_s1.py` flow.

## Invariants

- `stt-run.cmd` expects the speech-engine DLLs in `target\debug\native` (see `../copy-speech-engine.ps1`).

## Side Effects

- Spawns local processes; reads models under `%LOCALAPPDATA%\ProjectOASIS`.

## Verification

- Run: `scripts\bench\stt-run.cmd FF "%LOCALAPPDATA%\ProjectOASIS\Models\stt\parakeet-tdt-0.6b-v2" 4 3`
