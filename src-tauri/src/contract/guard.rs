//! Write guard keeping slice 2 off real D2R installations.

use std::path::{Path, PathBuf};

use super::{ErrorCode, LauncherError};

/// File that marks a product root as a disposable test installation.
pub const DISPOSABLE_SENTINEL: &str = ".resurrected-disposable";

/// Proof that a product root may be written. Install code takes a `&WriteGuard`
/// for every mutation of a product root and its sibling staging/backup paths, so a
/// write without a passed check does not compile.
///
/// Until the slice 3 gate ticket removes this check, only roots containing a regular
/// [`DISPOSABLE_SENTINEL`] file pass, so a real D2R installation is never written.
/// The root must already exist (fixtures create it with the sentinel). The sentinel
/// is an unknown file to every operation: it stays in place, and clean install
/// carries it forward.
#[derive(Debug)]
pub struct WriteGuard {
    root: PathBuf,
}

impl WriteGuard {
    /// Refuses with [`ErrorCode::ProductRootNotDisposable`] unless `product_root`
    /// is a directory containing the sentinel as a regular file (not a link).
    pub fn check(product_root: &Path) -> Result<Self, LauncherError> {
        let sentinel = product_root.join(DISPOSABLE_SENTINEL);
        match std::fs::symlink_metadata(&sentinel) {
            Ok(meta) if meta.is_file() => Ok(Self {
                root: product_root.to_path_buf(),
            }),
            _ => Err(LauncherError::new(
                ErrorCode::ProductRootNotDisposable,
                format!(
                    "refusing to write {}: no {DISPOSABLE_SENTINEL} sentinel",
                    product_root.display()
                ),
            )),
        }
    }

    /// The product root this guard permits writing.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_guard_refuses_product_root_without_sentinel() {
        let dir = tempfile::tempdir().unwrap();
        let err = WriteGuard::check(dir.path()).unwrap_err();
        assert_eq!(err.code, ErrorCode::ProductRootNotDisposable);
    }

    #[test]
    fn write_guard_refuses_missing_root_and_sentinel_directory() {
        let dir = tempfile::tempdir().unwrap();
        assert!(WriteGuard::check(&dir.path().join("absent")).is_err());
        std::fs::create_dir(dir.path().join(DISPOSABLE_SENTINEL)).unwrap();
        assert!(WriteGuard::check(dir.path()).is_err());
    }

    #[test]
    fn write_guard_allows_disposable_product_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(DISPOSABLE_SENTINEL), b"").unwrap();
        let guard = WriteGuard::check(dir.path()).unwrap();
        assert_eq!(guard.root(), dir.path());
    }
}
