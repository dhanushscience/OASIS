# Contributing to OASIS

Thanks for wanting to help. OASIS is a three-button pad plus a free Windows app. The app is a Rust workspace in `SW/` with a Tauri + Svelte shell in `SW/apps/oasis-desktop`.

## Build from source

You need Windows 10 or 11, [Rust](https://rustup.rs) (1.82 or newer) and [Node.js](https://nodejs.org) 20 or newer.

```
cd SW/apps/oasis-desktop/web
npm ci
npm run build

cd ../../..
cargo build --workspace
```

Build the web part first, because the desktop crate embeds its output. The speech engine is loaded at run time, so you do not need extra DLLs to build or test. To hear voice working you need the models that come with the installed app; see [beta/INSTALL.md](beta/INSTALL.md).

## Run the tests

```
cd SW
cargo test --workspace

cd apps/oasis-desktop/web
npm test
```

CI runs the same commands on every pull request. `cargo fmt --all` and `cargo clippy --workspace --all-targets` should be clean too.

## Propose a change

1. For anything bigger than a small fix, open an issue first so we can agree on the idea.
2. Fork, make a branch, and keep the change to one thing.
3. Open a pull request against `master` and fill in the template.

Found a bug? See [beta/FEEDBACK.md](beta/FEEDBACK.md).

## Code style

Keep it small and match the surroundings: same naming, same structure, same level of comments. Put logic in the crates under `SW/crates/`, not in the app shells. Do not add a dependency for a few lines of code. Each folder has a `README.md`; update it if you change what the folder does.

By contributing, you agree your work is licensed under the project's Apache-2.0 license.
