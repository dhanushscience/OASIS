# `oasis-core/src/matching/panel/` — which part of a program has focus

## Overview
Decides which panel of a program holds the keyboard focus (an IDE's terminal, editor, AI chat, …)
so a profile's extra button set for that panel can be chosen. It is the pure half of the C#
`Services/Context/*`: the catalog data, the scoring, and the stabiliser. Reading the accessibility
tree is the OS layer's job (`oasis-win`), which hands over a `UiSnapshot`.

## Interface
- `mod.rs` — declares `catalog`, `classify`, `state`; re-exports from `classify` and `PanelState`.
- `catalog.rs` — `Scope`, `Rule`, `Exclusion`, `PanelDef`, `Family`; `FAMILIES` (4 families: Electron IDE,
  browser, Visual Studio, Office); `resolve_family(process_name, process_path)`; `Family::matches_process`.
  Data of the C# `AppCatalog.json` (version 2) written as Rust so no JSON reader is needed.
- `classify.rs` — `UiNode`, `UiSnapshot` (`is_weak_focus`, `is_shell_chrome_focus`), `PanelResult`,
  `classify(process_name, process_path, title, url, ui)`, `WorkbenchNode`, `workbench_panel`,
  `should_probe_workbench`, `merge_workbench`.
- `state.rs` — `PanelState::stabilize(incoming, force_fresh, now)`.

## Invariants
- No OS access: input is only a snapshot passed in by the caller (`classify.rs` module doc).
- A weak reading that disagrees with a strong one a moment ago is not believed, so tree flicker does not
  flip buttons (`state.rs:PanelState::stabilize`).
- Time comes in as `now: Instant`; nothing reads a clock.

## Side Effects
- None. `PanelState` mutates only itself.

## Verification
- Run: `cargo test -p oasis-core`
