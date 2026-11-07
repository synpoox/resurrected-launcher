//! Preflight: running processes, write permission and free space, checked before a
//! transaction opens and rechecked immediately before the first live mutation. A
//! failure here changes nothing in the installation.

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::journal::{io_error, sibling, Paths};
use crate::contract::{ErrorCode, LauncherError};

/// Bytes an operation needs on the product root's volume (#6 step 1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpaceNeeds {
    pub download: u64,
    pub staging: u64,
    /// The recovery copy of the previous installation's replaced files.
    pub recovery_copy: u64,
}

impl SpaceNeeds {
    pub fn total(&self) -> u64 {
        self.download
            .saturating_add(self.staging)
            .saturating_add(self.recovery_copy)
    }
}

/// The machine facts preflight reads. Tests inject their own.
pub trait Host: Send + Sync {
    /// Bytes the current user may still write on `dir`'s volume.
    fn free_space(&self, dir: &Path) -> io::Result<u64>;
    /// Descriptions of processes holding any of `files` (managed files the
    /// operation will move) open; empty when none.
    fn processes_using(&self, files: &[PathBuf]) -> Vec<String>;
}

/// The real machine.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemHost;

impl Host for SystemHost {
    #[cfg(windows)]
    fn free_space(&self, dir: &Path) -> io::Result<u64> {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        extern "system" {
            fn GetDiskFreeSpaceExW(
                dir: *const u16,
                free_to_caller: *mut u64,
                total: *mut u64,
                total_free: *mut u64,
            ) -> i32;
        }
        let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut free = 0u64;
        // SAFETY: NUL-terminated path; out-pointers are valid or null as allowed.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(free)
    }

    #[cfg(not(windows))]
    fn free_space(&self, _dir: &Path) -> io::Result<u64> {
        // shortcut: the product targets Windows only; non-Windows dev builds skip the space check.
        Ok(u64::MAX)
    }

    /// A file a running game holds open cannot be opened exclusively (Windows
    /// sharing violation). shortcut: this sees processes through the files the
    /// operation moves; whole-process tracking belongs to the Game module.
    fn processes_using(&self, files: &[PathBuf]) -> Vec<String> {
        files
            .iter()
            .filter(|f| in_use(f))
            .map(|f| format!("a process has {} open", f.display()))
            .collect()
    }
}

#[cfg(windows)]
fn in_use(file: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    const ERROR_LOCK_VIOLATION: i32 = 33;
    let opened = OpenOptions::new().read(true).share_mode(0).open(file);
    matches!(
        opened.map_err(|e| e.raw_os_error()),
        Err(Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION))
    )
}

#[cfg(not(windows))]
fn in_use(_: &Path) -> bool {
    false
}

/// Creates and removes a probe file in `dir`, proving the normal user can write
/// there. A pre-existing file of that name is opened, never deleted.
fn probe_write(probe: &Path) -> Result<(), LauncherError> {
    let existed = fs::symlink_metadata(probe).is_ok();
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(probe)
        .map_err(|e| io_error("cannot write", probe, e))?;
    if !existed {
        fs::remove_file(probe).map_err(|e| io_error("cannot remove", probe, e))?;
    }
    Ok(())
}

/// Refuses with [`ErrorCode::GameRunning`], [`ErrorCode::AccessDenied`] or
/// [`ErrorCode::InsufficientSpace`]. Writes nothing that outlives the call.
pub fn check(
    paths: &Paths,
    bytes_needed: u64,
    moved_files: &[PathBuf],
    host: &dyn Host,
) -> Result<(), LauncherError> {
    let running = host.processes_using(moved_files);
    if !running.is_empty() {
        return Err(LauncherError::new(
            ErrorCode::GameRunning,
            running.join("; "),
        ));
    }
    probe_write(&paths.root.join(".resurrected-write-probe"))?;
    probe_write(&sibling(&paths.root, ".resurrected-write-probe"))?;
    let parent = paths.root.parent().unwrap_or(&paths.root);
    let free = host
        .free_space(parent)
        .map_err(|e| io_error("reading free space of", parent, e))?;
    if free < bytes_needed {
        return Err(LauncherError::new(
            ErrorCode::InsufficientSpace,
            format!(
                "{bytes_needed} bytes needed, {free} free on {}",
                parent.display()
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_host_reports_free_space_of_a_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(SystemHost.free_space(dir.path()).unwrap() > 0);
    }

    #[cfg(windows)]
    #[test]
    fn system_host_sees_a_process_holding_a_managed_file_open() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pd2r.mpq");
        fs::write(&file, b"x").unwrap();
        assert!(SystemHost
            .processes_using(std::slice::from_ref(&file))
            .is_empty());
        let _held = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&file)
            .unwrap();
        assert_eq!(SystemHost.processes_using(&[file]).len(), 1);
    }
}
