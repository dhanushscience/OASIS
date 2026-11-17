# oasis-dpapi

## Overview
Windows DPAPI protect and unprotect with current-user scope and optional entropy. Output is byte-for-byte what .NET `ProtectedData` wrote, so the C# app's `secrets/*` files still open. With `oasis-win`, one of the two crates allowed to use `unsafe` (the `os` module in `src/lib.rs`).

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
