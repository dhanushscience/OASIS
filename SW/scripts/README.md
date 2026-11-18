# scripts

## Overview

PowerShell tooling for packaging and releasing the OASIS beta, plus benchmark helpers under `./bench/`.
All scripts run from `SW/` on Windows.

## Interface

- `copy-speech-engine.ps1 -From <folder> [-To <dir>]`: copies `sherpa-onnx-c-api.dll` and `onnxruntime.dll` (found in `-From` or `-From\runtimes\win-x64\native`) to `target\debug\native`.
- `package-beta.ps1 [-SpeechEngine] [-Notes] [-Publish] [-ReleaseRepo] [-SigningKey]`: stages `dist\stage\native`, runs `npm run build` in `apps\oasis-desktop\web`, builds the signed NSIS installer with the Tauri CLI (`tauri.beta.conf.json`), writes `latest.json`, and with `-Publish` creates the GitHub release via `gh`. Default repo `dhanushscience/OASIS`; key `%USERPROFILE%\.tauri\oasis-updater.key`.
- `publish-models.ps1 -LlamaZip -LlamaLicense [-Tag] [-Repo] [-Stage] [-Publish]`: stages the voice model files from `%LOCALAPPDATA%\ProjectOASIS\Models`, hashes them (SHA-256), regenerates `crates\oasis-core\src\voice\model_files.rs`, and with `-Publish` uploads them to a pre-release (`--latest=false`).
- `models/` holds `parakeet.NOTICE.txt` and `s1-mini-oasis.NOTICE`, the licence notices shipped beside those models (no contract file).
- [bench](./bench/README.md): speech and polish benchmarks.

## Invariants

- `package-beta.ps1` needs the two speech-engine DLLs and an updater signing key, and throws without them; no model or Groq key is packed.
- The models release is always a pre-release so it never becomes "Latest" for the app updater.
- `model_files.rs` is generated; do not edit it by hand.
- `-ReleaseRepo` must match the updater endpoint in `tauri.conf.json`.

## Side Effects

- Deletes and recreates `dist\stage`; builds under `target\release\bundle\nsis\`.
- Sets `TAURI_SIGNING_PRIVATE_KEY` in the process environment.
- Overwrites `model_files.rs`; with `-Publish`, creates GitHub releases and uploads assets (network).

## Verification

- No tests. Dry run: `.\scripts\publish-models.ps1 -LlamaZip <zip> -LlamaLicense <file>` (prepares without uploading).
