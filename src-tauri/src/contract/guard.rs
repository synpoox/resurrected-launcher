//! Write guard keeping slice 2 off real D2R installations.

use std::path::{Path, PathBuf};

use super::{ErrorCode, LauncherError};

/// File that marks a product root as a disposable test installation.
pub const DISPOSABLE_SENTINEL: &str = ".resurrected-disposable";

/// Proof that a product root may be written. Install code takes a `&WriteGuard` for
/// every mutation of the product root, so a write without a passed check does not
/// compile. Install derives its staging/backup paths as siblings of
/// [`WriteGuard::root`]; the guard itself proves only the root, not those paths.
///
/// Until the slice 3 gate ticket removes this check, only roots containing a regular
/// [`DISPOSABLE_SENTINEL`] file pass, so a real D2R installation (which has none) is
/// never written, and neither are its siblings. Fixtures create the root with the
/// sentinel. The sentinel is an unknown file to every operation, so it stays in
/// place; clean install moves it as specified at
/// [`super::Boundary::CleanInstallBackupMade`], and recovery re-guards with
/// [`WriteGuard::check_with_backup`].
#[derive(Debug)]
pub struct WriteGuard {
    root: PathBuf,
}

fn refuse(code: ErrorCode, root: &Path, why: &str) -> LauncherError {
    LauncherError::new(code, format!("refusing to write {}: {why}", root.display()))
}

fn has_sentinel(dir: &Path) -> bool {
    // symlink_metadata: a link named like the sentinel does not count.
    std::fs::symlink_metadata(dir.join(DISPOSABLE_SENTINEL)).is_ok_and(|m| m.is_file())
}

/// Absolute, with the parent canonicalized once (on Windows a `\\?\` path). The
/// root itself may be absent; if present it must be a real directory, not a link.
fn canonical_root(root: &Path) -> Result<PathBuf, LauncherError> {
    let invalid = |why| refuse(ErrorCode::InvalidRequest, root, why);
    if !root.is_absolute() {
        return Err(invalid("product root must be absolute"));
    }
    let (Some(parent), Some(name)) = (root.parent(), root.file_name()) else {
        return Err(invalid("product root has no parent or name"));
    };
    let parent = std::fs::canonicalize(parent).map_err(|e| {
        refuse(
            ErrorCode::ProductRootNotDisposable,
            root,
            &format!("parent unreadable: {e}"),
        )
    })?;
    let root = parent.join(name);
    if let Ok(meta) = std::fs::symlink_metadata(&root) {
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(refuse(
                ErrorCode::ProductRootNotDisposable,
                &root,
                "not a real directory",
            ));
        }
    }
    Ok(root)
}

impl WriteGuard {
    /// Issues a guard for an existing `product_root` holding the sentinel.
    ///
    /// Errors: [`ErrorCode::InvalidRequest`] for a relative path;
    /// [`ErrorCode::ProductRootNotDisposable`] when the root is missing, is a link
    /// or lacks a regular sentinel file.
    pub fn check(product_root: &Path) -> Result<Self, LauncherError> {
        let root = canonical_root(product_root)?;
        if has_sentinel(&root) {
            Ok(Self { root })
        } else {
            Err(refuse(
                ErrorCode::ProductRootNotDisposable,
                &root,
                "no disposable sentinel",
            ))
        }
    }

    /// For recovery of a clean install, where the root may be absent between
    /// `CleanInstallBackupMade` and `StagedMovedToLive`: passes if the root holds
    /// the sentinel, or if the root is absent and `backup` (the journal-recorded
    /// clean-install backup, a sibling of the root) holds it. The guard's root is
    /// still `product_root`.
    pub fn check_with_backup(product_root: &Path, backup: &Path) -> Result<Self, LauncherError> {
        let root = canonical_root(product_root)?;
        let backup = canonical_root(backup)?;
        if backup.parent() != root.parent() {
            return Err(refuse(
                ErrorCode::InvalidRequest,
                &backup,
                "backup is not a sibling of the product root",
            ));
        }
        if has_sentinel(&root) || (!root.exists() && has_sentinel(&backup)) {
            Ok(Self { root })
        } else {
            Err(refuse(
                ErrorCode::ProductRootNotDisposable,
                &root,
                "no disposable sentinel in root or backup",
            ))
        }
    }

    /// The absolute product root this guard permits writing.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(r: Result<WriteGuard, LauncherError>) -> ErrorCode {
        r.unwrap_err().code
    }

    fn disposable_root() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("PD2R");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join(DISPOSABLE_SENTINEL), b"").unwrap();
        (dir, root)
    }

    #[test]
    fn write_guard_allows_disposable_product_root_and_stores_canonical_path() {
        let (_dir, root) = disposable_root();
        let guard = WriteGuard::check(&root).unwrap();
        assert_eq!(guard.root(), std::fs::canonicalize(&root).unwrap());
    }

    #[test]
    fn write_guard_refuses_product_root_without_sentinel() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            code(WriteGuard::check(dir.path())),
            ErrorCode::ProductRootNotDisposable
        );
    }

    #[test]
    fn write_guard_refuses_relative_missing_and_sentinel_directory() {
        assert_eq!(
            code(WriteGuard::check(Path::new("mods/PD2R"))),
            ErrorCode::InvalidRequest
        );
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            code(WriteGuard::check(&dir.path().join("absent"))),
            ErrorCode::ProductRootNotDisposable
        );
        std::fs::create_dir(dir.path().join(DISPOSABLE_SENTINEL)).unwrap();
        assert_eq!(
            code(WriteGuard::check(dir.path())),
            ErrorCode::ProductRootNotDisposable
        );
    }

    #[cfg(unix)]
    use std::os::unix::fs::{symlink as symlink_file, symlink as symlink_dir};
    #[cfg(windows)]
    use std::os::windows::fs::{symlink_dir, symlink_file};

    #[test]
    fn write_guard_refuses_symlinked_sentinel() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("elsewhere");
        std::fs::write(&target, b"").unwrap();
        let root = dir.path().join("PD2R");
        std::fs::create_dir(&root).unwrap();
        if symlink_file(&target, root.join(DISPOSABLE_SENTINEL)).is_err() {
            eprintln!("skipped: this account cannot create symlinks");
            return;
        }
        assert_eq!(
            code(WriteGuard::check(&root)),
            ErrorCode::ProductRootNotDisposable
        );
    }

    #[test]
    fn write_guard_refuses_symlinked_product_root() {
        let (dir, real) = disposable_root();
        let link = dir.path().join("link");
        if symlink_dir(&real, &link).is_err() {
            eprintln!("skipped: this account cannot create symlinks");
            return;
        }
        assert_eq!(
            code(WriteGuard::check(&link)),
            ErrorCode::ProductRootNotDisposable
        );
    }

    #[test]
    fn write_guard_recovers_clean_install_from_backup_only_while_root_is_absent() {
        let (dir, root) = disposable_root();
        let backup = dir.path().join("PD2R.clean-install-backup");
        std::fs::rename(&root, &backup).unwrap();
        let guard = WriteGuard::check_with_backup(&root, &backup).unwrap();
        assert_eq!(guard.root().file_name(), root.file_name());

        // A root without the sentinel reappearing does not borrow the backup's.
        std::fs::create_dir(&root).unwrap();
        assert_eq!(
            code(WriteGuard::check_with_backup(&root, &backup)),
            ErrorCode::ProductRootNotDisposable
        );
    }

    #[test]
    fn write_guard_backup_must_be_a_sibling() {
        let (dir, root) = disposable_root();
        let nested = dir.path().join("x");
        std::fs::create_dir(&nested).unwrap();
        assert_eq!(
            code(WriteGuard::check_with_backup(&root, &nested.join("b"))),
            ErrorCode::InvalidRequest
        );
    }
}
