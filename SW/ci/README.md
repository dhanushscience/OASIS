# ci: continuous integration

## Overview

**What:** `rust.yml`, a GitHub Actions workflow (job `test`) that formats, lints (zero warnings,
`RUSTFLAGS: -D warnings`) and tests this workspace on `windows-latest`, `macos-latest` and
`ubuntu-latest`, then runs `oasis status` on each.

**Why:** the Rust app targets three OSes but day-to-day work happens on one. CI is what proves
the other two still build.

**When:** on every push or pull request that touches `SW/**` or `.github/workflows/rust.yml`.

**How:** GitHub only reads workflows from `.github/workflows/` at the repository root, so copy
`rust.yml` to `OASIS RLS/.github/workflows/rust.yml`. It runs with `working-directory: SW`.

## Interface

Steps, in order: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
`cargo test --workspace`, `cargo run -p oasis-cli -- status`. The matrix uses `fail-fast: false`.

## Invariants

- Any compiler or clippy warning fails the build.
- The copy at the repository root must match `ci/rust.yml`; this folder is the source.

## Side Effects

- None locally. On GitHub it consumes runner minutes on three OSes.

## Verification

- Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo test --workspace` (from `SW/`).
