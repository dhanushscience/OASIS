# polish

## Overview

Benchmarks the local cleanup LLM (S1-mini, or SpeakoFlow-Mini as comparison) behind a `llama-server` on 127.0.0.1. Python scripts use the standard library only; run them with `python -I`.

## Interface

- `run.ps1 <s1|speako> <cpu|vulkan> <label> [affinity hex] [threads] [gpu layers]`: starts `llama-server` on port 8091, waits for `/health`, runs `bench_polish.py`, stops the server. Server log: `server-<model>-<label>.log` beside the script.
- `bench_polish.py <s1|speako> <label> [runs] [port]`: times each case in `cases.json` over several runs; checks every `keep` term survives and every `gone` term is removed; prints a JSON summary last.
- `cases.json`: cases with `id`, `size`, `text`, `keep`, `gone`.
- `eval_s1.py <rules.jsonl> [results.json]`: scores S1-mini on a cases file four ways (rules, S1 raw, S1 sent, rules+S1). Starts its own server on port 18731. Input comes from `cargo run -q -p oasis-core --example eval_cleanup -- <cases.md> target/m.txt target/rules.jsonl`.
- `try_s1.py`: interactive; type a sentence, see S1-mini's cleaned output and timing. Port 18731.

## Invariants

- Servers bind to `127.0.0.1` only.
- `eval_s1.py` mirrors the app's rule that a sentence growing by more than 2 words is dropped; it does not repeat the app's meaning check.
- Models and llama.cpp builds are expected under `%LOCALAPPDATA%\ProjectOASIS` (`Models\llm`, `bench\llama-cpu` and `bench\llama-vulkan`, `Models\runtime\llama-vulkan`).

## Side Effects

- Starts and kills `llama-server` processes; writes server logs and result files here.

## Verification

- Run: `powershell scripts\bench\polish\run.ps1 s1 vulkan test`
