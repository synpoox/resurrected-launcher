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
/// A path that cannot be canonicalized whole (gone, access denied) keeps its
/// deepest ancestor that can be, plus the rest as given, so a missing image under
/// an 8.3 short-name folder (`RUNNER~1`) still compares in long form with the root.
fn fold(path: &Path) -> PathBuf {
    let path = canonical_prefix(path);
    let text = path.to_string_lossy();
    PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text).to_uppercase())
}

fn canonical_prefix(path: &Path) -> PathBuf {
    for base in path.ancestors() {
        if base.as_os_str().is_empty() {
            break;
        }
        if let Ok(canonical) = std::fs::canonicalize(base) {
            // `base` is an ancestor of `path`, so the strip cannot fail.
            let rest = path.strip_prefix(base).unwrap_or(Path::new(""));
            return if rest.as_os_str().is_empty() {
                canonical
            } else {
                canonical.join(rest)
            };
        }
    }
    path.to_path_buf()
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

/// Like [`wait_until_exited`], for a Play: the loader may hand off to the game and
/// exit before the game's process shows up under `root` (#200), so once nothing
/// runs, it keeps looking for up to `grace` and watches whatever appears. `seen`
/// gets every listing that found something, so a report can say which images ran.
/// `grace` is asked each time nothing runs, so it can depend on how the loader
/// ended. Returns when nothing ran for the last time, before the grace that
/// confirmed it.
pub fn wait_until_exited_after_handoff(
    root: &Path,
    host: &dyn GameHost,
    poll: Duration,
    grace: &mut dyn FnMut() -> Duration,
    seen: &mut dyn FnMut(&[RunningGame]),
) -> Result<std::time::Instant, LauncherError> {
    loop {
        loop {
            let running = running_under(root, host)?;
            if running.is_empty() {
                break;
            }
            seen(&running);
            std::thread::sleep(poll);
        }
        let emptied = std::time::Instant::now();
        let grace = grace();
        let deadline = emptied + grace;
        let mut appeared = false;
        while std::time::Instant::now() < deadline {
            std::thread::sleep(poll.min(grace));
            if !running_under(root, host)?.is_empty() {
                appeared = true;
                break;
            }
        }
        if !appeared {
            return Ok(emptied);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_folds_onto_its_existing_canonical_folder() {
        let root = tempfile::tempdir().unwrap();
        let sub = root.path().join("Sub");
        std::fs::create_dir(&sub).unwrap();
        // Spelled through `..` and in another case: only canonicalizing the
        // existing folder makes the two agree.
        let roundabout = sub.join("..").join("SUB").join("missing.exe");
        assert_eq!(fold(&roundabout), fold(&sub).join("MISSING.EXE"));
        assert!(fold(&roundabout).starts_with(fold(root.path())));
    }

    #[test]
    fn an_existing_path_folds_to_its_plain_upper_cased_canonical_form() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("D2R.exe");
        std::fs::write(&file, b"").unwrap();
        // Built here, not copied from `fold`: the verbatim prefix is backslash,
        // backslash, question mark, backslash.
        let verbatim: String = ['\\', '\\', '?', '\\'].iter().collect();
        assert_eq!(verbatim.len(), 4);
        let canonical = std::fs::canonicalize(&file).unwrap();
        let text = canonical.to_string_lossy();
        assert!(
            text.starts_with(&verbatim),
            "canonicalize is verbatim: {text}"
        );
        let expected = text[verbatim.len()..].to_uppercase();
        let folded = fold(&file);
        assert_eq!(folded, PathBuf::from(&expected));
        assert!(
            !folded.to_string_lossy().starts_with('\\'),
            "the verbatim prefix is stripped: {}",
            folded.display()
        );
    }
}
