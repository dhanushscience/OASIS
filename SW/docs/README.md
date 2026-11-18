# `docs/` — design documents

**What:** a pointer. This folder holds no design documents in git.

**Why:** the long design notes (status, porting map, decisions, voice design and benchmarks,
update flow, architecture) are kept on the owner's machine, not in the repository. Many crate
READMEs still cite them by name, for example `DECISIONS D13` or `docs/VOICE_DECISIONS.md V38`.
Read those as references to the private notes.

**What to read in git instead:**

| Question | Read |
|---|---|
| What is in the app and where | [`../README.md`](../README.md) |
| Which crate owns what, and the layer rules | [`../crates/README.md`](../crates/README.md) |
| How the pad and voice pipelines run | [`../crates/oasis-engine/README.md`](../crates/oasis-engine/README.md), [`../crates/oasis-core/README.md`](../crates/oasis-core/README.md) |
| What the programs do | [`../apps/README.md`](../apps/README.md) |
| Packaging, releases, model publishing, benchmarks | [`../scripts/README.md`](../scripts/README.md) |
| CI | [`../ci/README.md`](../ci/README.md) |

**When to update:** when a crate README cites a decision that matters to a contributor, put the
reason in that README so it does not depend on the private notes.

The BLE wire format and product behaviour specs stay authoritative in the C# repo for now:
`OASIS/docs/PROTOCOLS.md`, `OASIS/SW/docs/FEATURES.md`.
