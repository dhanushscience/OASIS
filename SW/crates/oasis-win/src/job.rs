//! Helper programs (the polish server, the text-to-speech voice) that end when OASIS ends.
//!
//! Each is put in one Windows job object that kills everything in it when its last handle closes.
//! The handle lives for the whole process and Windows closes it when OASIS exits for any reason: a
//! normal quit, a crash, an update restart, or Task Manager. Before this, every restart that was not
//! a clean quit left a `llama-server.exe` behind holding about 1.2 GB; one PC had fifteen (20 GB).
//! Programs the user asked to open (apps, web pages) are never put in the job.

use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};

/// The job, made on first use. Stored as an integer so the static is `Sync`.
static JOB: OnceLock<Option<isize>> = OnceLock::new();

#[allow(unsafe_code)]
fn job() -> Option<HANDLE> {
    let made = JOB.get_or_init(|| {
        // SAFETY: plain calls; the handle is kept open (never closed) for the life of the process.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return None;
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let size = u32::try_from(std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()).unwrap_or(0);
            if SetInformationJobObject(job, JobObjectExtendedLimitInformation, std::ptr::from_ref(&limits).cast(), size) == 0 {
                CloseHandle(job);
                return None;
            }
            Some(job as isize)
        }
    });
    made.map(|h| h as HANDLE)
}

/// Ends the process `pid` when OASIS ends. `false` if Windows refused (the helper then runs as before).
#[allow(unsafe_code)]
pub fn end_with_this_process(pid: u32) -> bool {
    let Some(job) = job() else { return false };
    // SAFETY: the process handle is opened, used once and closed here.
    unsafe {
        let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
        if process.is_null() {
            return false;
        }
        let done = AssignProcessToJobObject(job, process) != 0;
        CloseHandle(process);
        done
    }
}

/// Ends every running copy of the program at `exe` (a full path). For helpers left behind by an
/// OASIS that was stopped before the job existed; call it before starting the helper. Returns how many.
#[allow(unsafe_code)]
pub fn end_strays(exe: &std::path::Path) -> usize {
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    use windows_sys::Win32::System::Threading::TerminateProcess;
    let Some(name) = exe.file_name().map(|n| n.to_string_lossy().to_lowercase()) else { return 0 };
    let wanted = exe.to_string_lossy().to_lowercase();
    let mut ended = 0;
    // SAFETY: the snapshot and each process handle are opened, used and closed here; the entry is
    // zeroed and sized as the API requires.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot.is_null() || snapshot as isize == -1 {
            return 0;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = u32::try_from(std::mem::size_of::<PROCESSENTRY32W>()).unwrap_or(0);
        let mut more = Process32FirstW(snapshot, &mut entry) != 0;
        while more {
            let end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let pid = entry.th32ProcessID;
            if String::from_utf16_lossy(&entry.szExeFile[..end]).to_lowercase() == name
                && pid != std::process::id()
                && crate::window::image_path(pid).is_some_and(|p| p.to_lowercase() == wanted)
            {
                let process = OpenProcess(PROCESS_TERMINATE, 0, pid);
                if !process.is_null() {
                    if TerminateProcess(process, 1) != 0 {
                        ended += 1;
                    }
                    CloseHandle(process);
                }
            }
            more = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    ended
}

/// Private memory in bytes and open handles of process `pid` (OASIS itself or a helper), for the
/// periodic memory line in the timing log. `None` if the process is gone or cannot be read.
#[allow(unsafe_code)]
pub fn usage(pid: u32) -> Option<(u64, u32)> {
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
    use windows_sys::Win32::System::Threading::{GetProcessHandleCount, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ};
    // SAFETY: the handle is opened, used and closed here; the counters are zeroed and sized as the API requires.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut counters: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        let size = u32::try_from(std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>()).unwrap_or(0);
        counters.cb = size;
        let mut handles = 0u32;
        let read = K32GetProcessMemoryInfo(process, std::ptr::from_mut(&mut counters).cast::<PROCESS_MEMORY_COUNTERS>(), size) != 0;
        let counted = GetProcessHandleCount(process, &mut handles) != 0;
        CloseHandle(process);
        (read && counted).then_some((counters.PrivateUsage as u64, handles))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_has_memory_and_handles() {
        let (private, handles) = usage(std::process::id()).expect("this process can read itself");
        assert!(private > 1_000_000, "{private} bytes");
        assert!(handles > 0);
    }

    #[test]
    fn a_child_in_the_job_is_accepted() {
        let mut child = std::process::Command::new("cmd").args(["/c", "ping -n 3 127.0.0.1 >nul"]).spawn().unwrap();
        assert!(end_with_this_process(child.id()));
        let _ = child.kill();
        let _ = child.wait();
    }
}
