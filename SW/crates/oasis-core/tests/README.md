# `oasis-core/tests/` — characterization tests

**What:** integration tests that pin `oasis-core`'s behaviour from the outside, using only its
public API.

**Why:** "behaviour is the contract". These tests are ports of the C# characterization tests,
**same names, same scenarios, same expected results**, so the Rust rules provably match the
shipped app. If one fails after a change, behaviour changed — decide whether you meant it.

**When:** `cargo test -p oasis-core` locally, and in CI on Windows, macOS and Linux.

**How:** each reducer test builds a state, steps the reducer with timed events (`at(ms)`), and asserts
on the next state and the effects. The pad tests call `reduce` with `default_claims()`; the voice tests call `voice::reduce`.

| File | Tests | Ported from |
|---|---|---|
| `pad_reducer.rs` | 41 | `SW/tests/Oasis.Architecture.Tests/PadReducerTests.cs` (the gesture rules) |
| `voice_reducer.rs` | 23 | `SW/tests/Oasis.Architecture.Tests/VoiceReducerTests.cs` |
| `quality.rs` | 1 | Not a port. Runs the dictation rules over the takes in `fixtures/quality/` and pins which lines of the target text they reach ([`fixtures/quality/README.md`](./fixtures/quality/README.md)) |

Tests here use only the public API. Small unit tests sit next to the code they test (`#[cfg(test)] mod tests` at the bottom of each
file), e.g. the ported `EditDistanceTests`, `BrowserProcessesTests`, `YouTubeHostsTests`,
`BrowserBridgePolicyTests`, `MediaDuckPolicyTests`, `LabelPolicyTests`.
