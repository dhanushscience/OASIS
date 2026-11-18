# oasis-dpapi

## Overview
Windows DPAPI protect and unprotect with current-user scope and optional entropy. Output is byte-for-byte what .NET `ProtectedData` wrote, so the C# app's `secrets/*` files still open. With `oasis-win`, one of the crates meant for `unsafe` (here only the Windows `os` module in `src/lib.rs`; `oasis-voice` also allows it for its speech engine binding). Used by `oasis-platform` (`windows/dpapi.rs`).

## Interface
- `protect(plain, entropy) -> Result<Vec<u8>, DpapiError>` and `unprotect(blob, entropy)`.
- `DpapiError`: `Unsupported` (not Windows), `TooLarge` (over 4 GiB), `Os(u32)` (Win32 error code).

## Invariants
- The same entropy must be supplied to decrypt.
- Only the same Windows user can decrypt.
- Buffers returned by Windows are copied and freed inside `os`.
- On non-Windows, calls return `DpapiError::Unsupported`.

## Side Effects
- None beyond the Win32 calls; no disk or network.

## Verification
- Run: `cargo test -p oasis-dpapi`
