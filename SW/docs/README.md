# `docs/` — design documents

**What:** the documents you need before a larger change to the Rust app.

**Why:** the code says *how*; these say *why* and *what's left*, which the code cannot.

**When to read:** before starting a new area or making an architectural change; update them in
the same change that makes them wrong.

| File | Read it when |
|---|---|
| `TECHNICAL_OVERVIEW.md` | **Start here for how it works**: pad and voice pipelines, function by function |
| `STATUS.md` | **Start here.** What works, how sure we are, what is missing, in plain words |
| `PORTING.md` | You want to know what is ported, what is next, and which C# file maps where |
| `WINDOWS_BACKEND.md` | You want the Windows work in order and how each piece was checked |
| `DECISIONS.md` | You need the decisions made (UI toolkit, async runtime, model runtimes, …) and why |
| `VOICE_DECISIONS.md` | **Start here for voice**: every decision on voice behaviour, speed and models, and the build order |
| `VOICE_PIPELINE.md` | You need the voice design in full: the analysis, the intent gate, the cleanup tiers, the latency budget |
| `VOICE_BENCH.md` | You need the measured speed, accuracy and memory of each speech and polish model |
| `VOICE_MODEL_SETUP.md` | You need to know which PC runs which model, and how models are downloaded and loaded |
| `VOICE_MODEL_RESEARCH.md`, `VOICE_BASELINE_HARDWARE.md` | You need the research behind the model and hardware choices |
| `UPDATES.md` | You are shipping an update, or moving releases to one repo |
| `ARCHITECTURE.md` | You need the layer rules, the threading model, or where new code goes |
| `PLATFORMS.md` | You are working on Windows / macOS / Linux specifics |

The wire protocol and product behaviour specs stay authoritative in the C# repo for now:
`OASIS/docs/PROTOCOLS.md`, `OASIS/SW/docs/FEATURES.md`.
