//! What the PC has, for choosing its voice model set (`docs/VOICE_MODEL_SETUP.md` §2.2): processor,
//! AVX2, cores, memory, free disk, and graphics cards with the memory of their own.
//!
//! Graphics memory is read from the display drivers' registry entries
//! (`HardwareInformation.qwMemorySize`): the usual WMI value stops at 4 GB, which is exactly the
//! line that matters here. Nothing is sent anywhere.

#![allow(unsafe_code)]

use std::path::Path;
use std::ptr;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows_sys::Win32::System::Registry::{RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ};
use windows_sys::Win32::System::SystemInformation::{
    GetLogicalProcessorInformation, GlobalMemoryStatusEx, RelationProcessorCore, MEMORYSTATUSEX, SYSTEM_LOGICAL_PROCESSOR_INFORMATION,
};

/// The display adapters' device class.
const DISPLAY_CLASS: &str = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
const PROCESSOR_KEY: &str = r"HARDWARE\DESCRIPTION\System\CentralProcessor\0";
/// Adapter entries are numbered 0000, 0001, …; real PCs have a handful.
const MAX_ADAPTERS: u32 = 16;

/// What [`read`] found. Zero where Windows would not say.
#[derive(Debug, Clone, Default)]
pub struct Hardware {
    pub processor: String,
    pub avx2: bool,
    pub cores: u32,
    pub threads: u32,
    pub memory_bytes: u64,
    pub free_disk_bytes: u64,
    /// (name, memory of its own in bytes), one per display adapter, without software adapters.
    pub gpus: Vec<(String, u64)>,
}

/// Read the PC's facts; free disk is for the drive holding `models_folder` (or its nearest
/// existing parent).
pub fn read(models_folder: &Path) -> Hardware {
    Hardware {
        processor: registry_bytes(PROCESSOR_KEY, "ProcessorNameString").map(|b| utf16(&b)).unwrap_or_default().trim().to_string(),
        avx2: avx2(),
        cores: physical_cores(),
        threads: std::thread::available_parallelism().map_or(0, |n| u32::try_from(n.get()).unwrap_or(u32::MAX)),
        memory_bytes: memory_bytes(),
        free_disk_bytes: free_disk_bytes(models_folder),
        gpus: gpus(),
    }
}

fn avx2() -> bool {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        std::arch::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        false
    }
}

fn physical_cores() -> u32 {
    let mut bytes = 0u32;
    // SAFETY: a null buffer asks for the size needed.
    unsafe { GetLogicalProcessorInformation(ptr::null_mut(), &mut bytes) };
    let size = std::mem::size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION>();
    if bytes == 0 {
        return 0;
    }
    // SAFETY: the struct is plain data, so zeroed entries are valid.
    let mut entries: Vec<SYSTEM_LOGICAL_PROCESSOR_INFORMATION> = vec![unsafe { std::mem::zeroed() }; (bytes as usize).div_ceil(size)];
    let mut bytes = u32::try_from(entries.len() * size).unwrap_or(u32::MAX);
    // SAFETY: the buffer holds `bytes` bytes of entries.
    if unsafe { GetLogicalProcessorInformation(entries.as_mut_ptr(), &mut bytes) } == 0 {
        return 0;
    }
    let filled = bytes as usize / size;
    u32::try_from(entries[..filled].iter().filter(|e| e.Relationship == RelationProcessorCore).count()).unwrap_or(0)
}

fn memory_bytes() -> u64 {
    // SAFETY: plain data; dwLength is set before the call as the API requires.
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = u32::try_from(std::mem::size_of::<MEMORYSTATUSEX>()).unwrap_or(0);
    // SAFETY: `status` is a valid, sized MEMORYSTATUSEX.
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 { 0 } else { status.ullTotalPhys }
}

fn free_disk_bytes(folder: &Path) -> u64 {
    // The models folder may not exist yet: ask about the nearest folder that does.
    let existing = folder.ancestors().find(|p| p.exists()).unwrap_or(folder);
    let wide: Vec<u16> = existing.as_os_str().encode_wide_z();
    let mut free = 0u64;
    // SAFETY: zero-terminated path; the other out-pointers may be null.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, ptr::null_mut(), ptr::null_mut()) };
    if ok == 0 { 0 } else { free }
}

fn gpus() -> Vec<(String, u64)> {
    (0..MAX_ADAPTERS)
        .filter_map(|n| {
            let key = format!(r"{DISPLAY_CLASS}\{n:04}");
            let name = utf16(&registry_bytes(&key, "DriverDesc")?).trim().to_string();
            // The software adapters have no memory of their own and cannot run the model.
            if name.is_empty() || name.contains("Basic Display") || name.contains("Basic Render") {
                return None;
            }
            let memory = registry_bytes(&key, "HardwareInformation.qwMemorySize")
                .or_else(|| registry_bytes(&key, "HardwareInformation.MemorySize"))
                .map_or(0, |b| little_endian(&b));
            // Virtual displays (projectors, remote desktops) have no memory of their own.
            (memory > 0).then_some((name, memory))
        })
        .collect()
}

/// A registry value's raw bytes under HKEY_LOCAL_MACHINE.
fn registry_bytes(path: &str, name: &str) -> Option<Vec<u8>> {
    let path = wide(path);
    let mut key: HKEY = ptr::null_mut();
    // SAFETY: zero-terminated path, valid out-pointer.
    if unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, path.as_ptr(), 0, KEY_READ, &mut key) } != ERROR_SUCCESS {
        return None;
    }
    let name = wide(name);
    let mut size = 0u32;
    // SAFETY: a null data pointer asks for the size.
    let probe = unsafe { RegQueryValueExW(key, name.as_ptr(), ptr::null(), ptr::null_mut(), ptr::null_mut(), &mut size) };
    let mut value = None;
    if probe == ERROR_SUCCESS && size > 0 {
        let mut buffer = vec![0u8; size as usize];
        // SAFETY: the buffer holds `size` bytes.
        let read = unsafe { RegQueryValueExW(key, name.as_ptr(), ptr::null(), ptr::null_mut(), buffer.as_mut_ptr(), &mut size) };
        if read == ERROR_SUCCESS {
            buffer.truncate(size as usize);
            value = Some(buffer);
        }
    }
    // SAFETY: `key` was opened above.
    unsafe { RegCloseKey(key) };
    value
}

/// A REG_SZ value's text (UTF-16, maybe zero-terminated).
fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|u| *u != 0).collect();
    String::from_utf16_lossy(&units)
}

/// A REG_DWORD, REG_QWORD or REG_BINARY number, as Windows stores it.
fn little_endian(bytes: &[u8]) -> u64 {
    bytes.iter().take(8).rev().fold(0, |n, b| (n << 8) | u64::from(*b))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

trait EncodeWideZ {
    fn encode_wide_z(&self) -> Vec<u16>;
}

impl EncodeWideZ for std::ffi::OsStr {
    fn encode_wide_z(&self) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        self.encode_wide().chain(std::iter::once(0)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_numbers_and_text_are_read_as_windows_stores_them() {
        assert_eq!(little_endian(&[0, 0, 0, 0, 1, 0, 0, 0]), 4_294_967_296, "a 4 GB QWORD");
        assert_eq!(little_endian(&[0, 0, 0, 0x80]), 2_147_483_648, "a 2 GB DWORD");
        assert_eq!(utf16(&[b'G', 0, b'T', 0, 0, 0, b'x', 0]), "GT");
    }

    #[test]
    fn this_pc_reports_sensible_facts() {
        let hw = read(&std::env::temp_dir());
        assert!(hw.threads >= 1 && hw.cores >= 1 && hw.cores <= hw.threads, "{hw:?}");
        assert!(hw.memory_bytes > 1_000_000_000, "{hw:?}");
        assert!(hw.free_disk_bytes > 0, "{hw:?}");
    }
}
