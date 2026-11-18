# `oasis-core/examples/` — cleanup benchmark

## Overview
A developer tool, not part of the library. `eval_cleanup.rs` scores the rule-based dictation cleanup
(`oasis_core::speech::pipeline`) against a markdown table of spoken inputs and expected text.

## Interface
- `eval_cleanup.rs` — `cargo run -p oasis-core --example eval_cleanup -- <cases.md> [misses.txt [rules.jsonl]]`.
  Defaults: `scripts/bench/cleanup/cases.md`, `target/cleanup-misses.txt`. Cases are `| n | raw | expected |`
  rows, one table per section. Prints per-section "exact" and "close" scores (close ignores capitals,
  punctuation and spacing) and writes every miss to the misses file; the optional third file gets one
  JSON line per case (section, raw, expected, rules) for other tools to run their own step on the rule text.
  It also prints a "spoken-only" score (cases whose expected text uses only words already said) and a Minimal-Edit Score
  (words wrongly deleted or added, kept apart from the exact score). Cases run as dictation with formatting on (`CleanupContext::dictate(true)`).

## Invariants
- Reads only through `speech::pipeline` with `CleanupContext` / `CleanupRequest`; it never changes library behaviour.
- It panics (`expect`) if the cases file or the output files cannot be read or written; it is a developer tool, not run in CI beyond building.

## Side Effects
- Reads the cases file; writes the misses file and the optional JSONL file (`std::fs`).

## Verification
- Run: `cargo build -p oasis-core --examples`
