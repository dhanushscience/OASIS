# `oasis-core/examples/` — cleanup benchmark

## Overview
A developer tool, not part of the library. `eval_cleanup.rs` scores the rule-based dictation cleanup
(`oasis_core::speech::pipeline`) against a markdown table of spoken inputs and expected text.

## Interface
- `eval_cleanup.rs` — `cargo run -p oasis-core --example eval_cleanup -- <cases.md> [misses.txt [rules.jsonl]]`.
  Defaults: `scripts/bench/cleanup/cases.md`, `target/cleanup-misses.txt`. Cases are `| n | raw | expected |`
  rows, one table per section. Prints per-section "exact" and "close" scores (close ignores capitals,
  punctuation and spacing) and writes every miss to the misses file; the optional third file gets one
  JSON line per case.

## Invariants
- Reads only through `speech::pipeline` with `CleanupContext` / `CleanupRequest`; it never changes library behaviour.

## Side Effects
- Reads the cases file; writes the misses file and the optional JSONL file (`std::fs`).

## Verification
- Run: `cargo build -p oasis-core --examples`
