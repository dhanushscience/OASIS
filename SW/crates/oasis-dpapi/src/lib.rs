//! # oasis-dpapi
//!
//! `ProtectedData.Protect` / `Unprotect` from .NET, with `DataProtectionScope.CurrentUser`:
//! the bytes only the same Windows user can decrypt. The blobs are byte-for-byte what the C#
//! app wrote, so existing `secrets/*` files keep working.
//!
//! This is the only crate in the workspace that may use `unsafe`. The unsafe code is the
//! `os` module below: two Win32 calls, with the buffers Windows hands back copied and freed.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DpapiError {
    /// Not Windows.
    Unsupported,
    /// Too large for the Win32 API (over 4 GiB).
    TooLarge,
    /// Windows refused: wrong user, wrong entropy, damaged blob. Carries the Win32 error code.
    Os(u32),
}

impl fmt::Display for DpapiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DpapiError::Unsupported => f.write_str("DPAPI is only available on Windows."),
            DpapiError::TooLarge => f.write_str("The data is too large to protect."),
            DpapiError::Os(code) => write!(
                f,
                "Windows could not decrypt or encrypt this data (error {code}). \
                 It may belong to another Windows user."
            ),
        }
    }
}

impl std::error::Error for DpapiError {}

/// Encrypt `plain` for the current Windows user. `entropy` must be given again to decrypt.
pub fn protect(plain: &[u8], entropy: &[u8]) -> Result<Vec<u8>, DpapiError> {
    os::protect(plain, entropy)
}

/// Decrypt a blob made by [`protect`] (or by .NET `ProtectedData.Protect`, CurrentUser).
pub fn unprotect(blob: &[u8], entropy: &[u8]) -> Result<Vec<u8>, DpapiError> {
    os::unprotect(blob, entropy)
}

#[cfg(not(windows))]
mod os {
    use super::DpapiError;

    pub fn protect(_: &[u8], _: &[u8]) -> Result<Vec<u8>, DpapiError> {
        Err(DpapiError::Unsupported)
    }

    pub fn unprotect(_: &[u8], _: &[u8]) -> Result<Vec<u8>, DpapiError> {
        Err(DpapiError::Unsupported)
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod os {
    use std::ptr;
    use std::slice;

    use windows_sys::Win32::Foundation::{GetLastError, LocalFree};
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    use super::DpapiError;

    /// A view of `data` for a Win32 call. Valid only while `data` is borrowed.
    fn view(data: &[u8]) -> Result<CRYPT_INTEGER_BLOB, DpapiError> {
        let len = u32::try_from(data.len()).map_err(|_| DpapiError::TooLarge)?;
        // Windows only reads through these pointers for input blobs.
        Ok(CRYPT_INTEGER_BLOB { cbData: len, pbData: data.as_ptr().cast_mut() })
    }

    /// Copy out what Windows allocated, wipe it, and free it.
    ///
    /// # Safety
    /// `out` must be a blob filled in by a successful DPAPI call (allocated with `LocalAlloc`)
    /// that has not been freed yet.
    unsafe fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        if out.pbData.is_null() {
            return Vec::new();
        }
        let len = out.cbData as usize;
        let copy = slice::from_raw_parts(out.pbData, len).to_vec();
        ptr::write_bytes(out.pbData, 0, len);
        LocalFree(out.pbData.cast());
        copy
    }

    pub fn protect(plain: &[u8], entropy: &[u8]) -> Result<Vec<u8>, DpapiError> {
        let input = view(plain)?;
        let entropy = view(entropy)?;
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: ptr::null_mut() };
        // SAFETY: `input` and `entropy` point into slices that outlive the call; `out` is a
        // valid, writable blob; the optional pointers are null as the API allows.
        let ok = unsafe {
            CryptProtectData(
                &input,
                ptr::null(),
                &entropy,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            // SAFETY: no preconditions.
            return Err(DpapiError::Os(unsafe { GetLastError() }));
        }
        // SAFETY: the call succeeded, so `out` is a fresh Windows allocation.
        Ok(unsafe { take(out) })
    }

    pub fn unprotect(blob: &[u8], entropy: &[u8]) -> Result<Vec<u8>, DpapiError> {
        let input = view(blob)?;
        let entropy = view(entropy)?;
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: ptr::null_mut() };
        // SAFETY: as in `protect`.
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                ptr::null_mut(),
                &entropy,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            // SAFETY: no preconditions.
            return Err(DpapiError::Os(unsafe { GetLastError() }));
        }
        // SAFETY: the call succeeded, so `out` is a fresh Windows allocation.
        Ok(unsafe { take(out) })
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let blob = protect(b"secret value", b"entropy").unwrap();
        assert_ne!(blob, b"secret value");
        assert_eq!(unprotect(&blob, b"entropy").unwrap(), b"secret value");
    }

    #[test]
    fn empty_plaintext_round_trips() {
        let blob = protect(b"", b"e").unwrap();
        assert_eq!(unprotect(&blob, b"e").unwrap(), b"");
    }

    #[test]
    fn wrong_entropy_is_refused() {
        let blob = protect(b"x", b"right").unwrap();
        assert!(matches!(unprotect(&blob, b"wrong"), Err(DpapiError::Os(_))));
    }

    #[test]
    fn garbage_is_refused_not_a_crash() {
        assert!(matches!(unprotect(b"not a dpapi blob", b"e"), Err(DpapiError::Os(_))));
    }

    /// Opt-in .NET interop check. Set `OASIS_DPAPI_INTEROP_DIR` to a folder holding
    /// `dotnet.blob` (made by `ProtectedData.Protect("interop-ok", "interop", CurrentUser)`);
    /// this writes `rust.blob` there for .NET to open.
    #[test]
    fn interoperates_with_dotnet_when_a_blob_is_supplied() {
        let Ok(dir) = std::env::var("OASIS_DPAPI_INTEROP_DIR") else { return };
        let dir = std::path::Path::new(&dir);
        let from_dotnet = std::fs::read(dir.join("dotnet.blob")).unwrap();
        assert_eq!(unprotect(&from_dotnet, b"interop").unwrap(), b"interop-ok");
        std::fs::write(dir.join("rust.blob"), protect(b"interop-from-rust", b"interop").unwrap())
            .unwrap();
    }

    #[test]
    fn large_data_round_trips() {
        let data = vec![0xAB; 1 << 20];
        assert_eq!(unprotect(&protect(&data, b"e").unwrap(), b"e").unwrap(), data);
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn other_os_says_unsupported() {
        assert_eq!(protect(b"x", b"e"), Err(DpapiError::Unsupported));
    }
}
