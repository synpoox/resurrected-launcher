//! Game process tracking (#58): every process whose image is under the selected
//! D2R root, including games started outside the launcher.

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::GameHost;
use crate::contract::{ErrorCode, LauncherError, RunningGame};

/// The processes from [`GameHost::processes`] whose image path is inside `root`,
/// compared by path components as Windows does (case-insensitive; `C:\D2R2` is not
/// under `C:\D2R`). Empty when nothing runs there. Also used after a launcher
/// restart to rediscover a running game.
///
/// Errors: [`crate::contract::ErrorCode::Internal`] if enumeration fails.
pub fn running_under(root: &Path, host: &dyn GameHost) -> Result<Vec<RunningGame>, LauncherError> {
    let root = fold(root);
    let all = host
        .processes()
        .map_err(|e| LauncherError::new(ErrorCode::Internal, format!("listing processes: {e}")))?;
    Ok(all
        .into_iter()
        .filter(|p| fold(&p.image).starts_with(&root))
        .collect())
}

/// Canonical, `\\?\`-free, upper-cased form for a case-insensitive component compare.
/// A path that cannot be canonicalized (gone, access denied) is compared as given.
fn fold(path: &Path) -> PathBuf {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = path.to_string_lossy();
    PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text).to_uppercase())
}

/// Exit watcher: blocks, checking [`running_under`] every `poll`, until nothing runs
/// under `root`. The caller then reassesses readiness (back to ready).
pub fn wait_until_exited(
    root: &Path,
    host: &dyn GameHost,
    poll: Duration,
) -> Result<(), LauncherError> {
    while !running_under(root, host)?.is_empty() {
        std::thread::sleep(poll);
    }
    Ok(())
}

/// See [`GameHost::processes`]: Toolhelp32 snapshot plus
/// `QueryFullProcessImageNameW` and the process creation time.
#[cfg(windows)]
pub(super) fn system_processes() -> io::Result<Vec<RunningGame>> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    type Handle = isize;
    const TH32CS_SNAPPROCESS: u32 = 0x2;
    const INVALID_HANDLE_VALUE: Handle = -1;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    /// Seconds from the FILETIME epoch (1601-01-01) to the Unix epoch.
    const UNIX_EPOCH_FILETIME_SECS: u64 = 11_644_473_600;

    #[repr(C)]
    struct ProcessEntry32W {
        size: u32,
        usage: u32,
        process_id: u32,
        default_heap_id: usize,
        module_id: u32,
        threads: u32,
        parent_process_id: u32,
        pri_class_base: i32,
        flags: u32,
        exe_file: [u16; 260],
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> Handle;
        fn Process32FirstW(snapshot: Handle, entry: *mut ProcessEntry32W) -> i32;
        fn Process32NextW(snapshot: Handle, entry: *mut ProcessEntry32W) -> i32;
        fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> Handle;
        fn QueryFullProcessImageNameW(
            process: Handle,
            flags: u32,
            name: *mut u16,
            size: *mut u32,
        ) -> i32;
        // FILETIME is two little-endian u32 halves; a u64 has the same layout.
        fn GetProcessTimes(
            process: Handle,
            creation: *mut u64,
            exit: *mut u64,
            kernel: *mut u64,
            user: *mut u64,
        ) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }

    // SAFETY: plain Win32 calls; every out pointer is valid for the call and every
    // opened handle is closed before return.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let mut entry: ProcessEntry32W = std::mem::zeroed();
        entry.size = std::mem::size_of::<ProcessEntry32W>() as u32;
        let mut name = vec![0u16; 32_768];
        let mut found = Vec::new();
        let mut more = Process32FirstW(snapshot, &mut entry) != 0;
        while more {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.process_id);
            if process != 0 {
                let mut len = name.len() as u32;
                let (mut created, mut ignored) = (0u64, [0u64; 3]);
                let ok = QueryFullProcessImageNameW(process, 0, name.as_mut_ptr(), &mut len) != 0
                    && GetProcessTimes(
                        process,
                        &mut created,
                        &mut ignored[0],
                        &mut ignored[1],
                        &mut ignored[2],
                    ) != 0;
                CloseHandle(process);
                if ok {
                    found.push(RunningGame {
                        pid: entry.process_id,
                        image: OsString::from_wide(&name[..len as usize]).into(),
                        since: (created / 10_000_000).saturating_sub(UNIX_EPOCH_FILETIME_SECS),
                    });
                }
            }
            more = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
        Ok(found)
    }
}

#[cfg(not(windows))]
pub(super) fn system_processes() -> io::Result<Vec<RunningGame>> {
    // shortcut: the product targets Windows only; non-Windows dev builds see no games.
    Ok(Vec::new())
}
