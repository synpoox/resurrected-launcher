//! Per-install operation lock: at most one writer per product root, across
//! processes. An OS file lock (released by the OS if the process dies, so a crash
//! never leaves a stale lock) on `<name>.resurrected-lock` beside the root.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

use super::journal::{io_error, sibling};
use crate::contract::{ErrorCode, LauncherError, WriteGuard};

/// Held for the whole operation, including restart recovery. Dropping it unlocks.
#[derive(Debug)]
pub struct OperationLock {
    root: PathBuf,
    _file: File,
}

impl OperationLock {
    /// Takes the lock of `guard`'s root. A second writer, in this process or
    /// another, gets [`ErrorCode::OperationInProgress`]. The lock file itself is
    /// left in place (it is empty) so that unlocking never races a new writer.
    pub fn acquire(guard: &WriteGuard) -> Result<Self, LauncherError> {
        let path = sibling(guard.root(), ".resurrected-lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|e| io_error("opening lock", &path, e))?;
        match file.try_lock() {
            Ok(()) => Ok(Self {
                root: guard.root().to_path_buf(),
                _file: file,
            }),
            Err(TryLockError::WouldBlock) => Err(LauncherError::new(
                ErrorCode::OperationInProgress,
                format!("another operation holds {}", path.display()),
            )),
            Err(TryLockError::Error(e)) => Err(io_error("locking", &path, e)),
        }
    }

    /// The product root this lock serializes.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::DISPOSABLE_SENTINEL;

    #[test]
    fn second_writer_is_refused_until_the_first_releases_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("PD2R");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join(DISPOSABLE_SENTINEL), b"").unwrap();
        let guard = WriteGuard::check(&root).unwrap();

        let first = OperationLock::acquire(&guard).unwrap();
        let second = OperationLock::acquire(&guard).unwrap_err();
        assert_eq!(second.code, ErrorCode::OperationInProgress);
        drop(first);
        OperationLock::acquire(&guard).unwrap();
    }
}
