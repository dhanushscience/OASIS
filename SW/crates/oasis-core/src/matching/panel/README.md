# `oasis-core/src/matching/panel/` — which part of a program has focus

## Overview
Decides which panel of a program holds the keyboard focus (an IDE's terminal, editor, AI chat, …)
so a profile's extra button set for that panel can be chosen. It is the pure half of the C#
`Services/Context/*`: the catalog data, the scoring, and the stabiliser. Reading the accessibility
tree is the OS layer's job (`oasis-win`), which hands over a `UiSnapshot`.

## Interface
- `mod.rs` — declares `catalog`, `classify`, `state`; re-exports `classify`, `merge_workbench`, `should_probe_workbench`, `workbench_panel`, `PanelResult`, `UiNode`, `UiSnapshot`, `WorkbenchNode` and `PanelState`. Everything in `catalog` is reached by path.
- `catalog.rs` — `Scope`, `Rule`, `Exclusion`, `PanelDef`, `Family`; `FAMILIES` (4 families: Electron IDE,
  browser, Visual Studio, Office); `resolve_family(process_name, process_path)`; `Family::matches_process`.
  Data of the C# `AppCatalog.json` (version 2) written as Rust so no JSON reader is needed.
- `classify.rs` — `UiNode`, `UiSnapshot` (`is_weak_focus`, `is_shell_chrome_focus`), `PanelResult`,
  `classify(process_name, process_path, title, url, ui)`, `WorkbenchNode`, `workbench_panel`,
  `should_probe_workbench`, `merge_workbench`.
- `state.rs` — `PanelState::stabilize(incoming, force_fresh, now)`; `now` is a `std::time::Instant` the caller supplies. A medium or high reading, or one for the same panel, is taken at once; a weaker, different one is ignored for 500 ms after a medium or high reading.

## Invariants
- No OS access: input is only a snapshot passed in by the caller (`classify.rs` module doc).
- A weak reading that disagrees with a strong one a moment ago is not believed, so tree flicker does not
  flip buttons (`state.rs:PanelState::stabilize`).
- Time comes in as `now` (`std::time::Instant`); nothing here reads a clock.

## Side Effects
- None. `PanelState` mutates only itself.

## Verification
- Run: `cargo test -p oasis-core`
