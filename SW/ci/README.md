# ci: continuous integration

## Overview

**What:** `rust.yml`, a draft GitHub Actions workflow (job `test`) that formats, lints (zero
warnings, `RUSTFLAGS: -D warnings`) and tests this workspace on `windows-latest`, `macos-latest`
and `ubuntu-latest`, then runs `oasis status` on each.

**Not live:** the repository root has no `rust.yml`. The workflow that runs today is
`../../.github/workflows/ci.yml`: Windows only, on pushes and pull requests to `master`. It runs
the web tests and build, then `cargo test --workspace --no-fail-fast` (one Windows-dependent test
skipped). It does not run fmt or clippy. `rust.yml` is the starting point for adding macOS and
Linux once those backends exist.

**Why:** the Rust app targets three OSes but day-to-day work happens on one. A three-OS run is
what would prove the other two still build.

**When (if installed):** on every push or pull request that touches `SW/**` or
`.github/workflows/rust.yml`.

**How:** GitHub only reads workflows from `.github/workflows/` at the repository root, so copy
`rust.yml` there. It runs with `working-directory: SW`. It does not build the web page, so it
would need the Node steps from `ci.yml` before the desktop crate builds from a clean checkout.

## Interface

Steps, in order: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
`cargo test --workspace`, `cargo run -p oasis-cli -- status`. The matrix uses `fail-fast: false`.

## Invariants

- Any compiler or clippy warning fails the build.
- If `rust.yml` is copied to the repository root, the copy must match `ci/rust.yml`; this folder is the source.

## Side Effects

- None locally. On GitHub it consumes runner minutes on three OSes.

## Verification

- Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo test --workspace` (from `SW/`).
