# `oasis-core/src/input/reducer/` — the gesture reducer

## Overview
**What:** the `reduce()` function itself, split across four files by the kind of event it
handles.

**Why split:** it is about 780 lines, past the 500-line limit the C# project enforced. The C#
original is split the same way (`PadReducer.cs` + `PadReducer.Transitions.cs`). Each file
answers one question, so a bug report like "the tap runs on release, not press" points straight
at one file.

**When:** every pad event goes through `mod.rs::reduce`, which hands it to one of the three
files below.

**How:** each handler takes the current `PadState` and returns a `Transition`. Shared checks
(`allows` = "does the highest-priority claim with this id permit it?", `is_hidden_volume`,
`arms_show_desktop`, `due`) live in `mod.rs`.

## Interface
`reduce()` in `mod.rs` plus the handlers below.

| File | Handles | Answers |
|---|---|---|
| `mod.rs` | entry point, disconnect, shared helpers, timer names | "Which handler, and is this claim allowed?" |
| `press.rs` | `PadEvent::Down` | "What can this press still become, and does anything run right now?" |
| `release.rs` | `PadEvent::Up` | "Was it a tap, the end of a hold, or the first half of a double-click?" |
| `tick.rs` | `PadEvent::Tick` | "Which held gesture's deadline just passed, and does it win?" |

## Invariants
- Every event passes through `mod.rs::reduce`; handlers return a `Transition` and never mutate in place.
- A claim acts only if the highest-priority claim with that id permits it (`allows`).

## Side Effects
- None.

## Verification
- Run: `cargo test -p oasis-core`
