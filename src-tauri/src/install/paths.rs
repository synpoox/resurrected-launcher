//! Paths Install derives from a guarded product root.
//!
//! Staging, backup, clean-install backup and the downloaded archive are siblings of
//! the product root (see [`crate::contract::Boundary`]), so they sit on the root's
//! filesystem and activation can rename instead of copy. They exist only through a
//! [`WriteGuard`], so nothing here can point next to a real D2R installation.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::contract::{RelativePath, WriteGuard};

/// The transaction's sibling paths of one guarded product root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionPaths {
    staging: PathBuf,
    backup: PathBuf,
    clean_install_backup: PathBuf,
    archive: PathBuf,
    download_partial: PathBuf,
}

impl TransactionPaths {
    pub fn of(guard: &WriteGuard) -> Self {
        let root = guard.root();
        let sibling = |suffix: &str| {
            let mut name: OsString = root
                .file_name()
                .expect("a guarded root always has a name")
                .to_owned();
            name.push(suffix);
            root.with_file_name(name)
        };
        Self {
            staging: sibling(".resurrected-staging"),
            backup: sibling(".resurrected-backup"),
            clean_install_backup: sibling(".resurrected-clean-install-backup"),
            archive: sibling(".resurrected-download"),
            download_partial: sibling(".resurrected-download.partial"),
        }
    }

    /// Isolated staging `S`, where the archive is extracted.
    pub fn staging(&self) -> &Path {
        &self.staging
    }

    /// Recovery copy `B` of the replaced live entries.
    pub fn backup(&self) -> &Path {
        &self.backup
    }

    /// Labelled clean-install backup `K`; never silently deleted.
    pub fn clean_install_backup(&self) -> &Path {
        &self.clean_install_backup
    }

    /// The complete, verified release archive.
    pub fn archive(&self) -> &Path {
        &self.archive
    }

    /// An archive still being downloaded. Never trusted; a retry restarts it.
    pub fn download_partial(&self) -> &Path {
        &self.download_partial
    }
}

/// `rel` under `root`, with `/` turned into path components.
pub fn under(root: &Path, rel: &RelativePath) -> PathBuf {
    rel.as_str()
        .split('/')
        .fold(root.to_path_buf(), |p, c| p.join(c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::DISPOSABLE_SENTINEL;

    #[test]
    fn transaction_paths_are_siblings_of_the_product_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Product");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join(DISPOSABLE_SENTINEL), b"").unwrap();
        let guard = WriteGuard::check(&root).unwrap();
        let paths = TransactionPaths::of(&guard);
        for p in [
            paths.staging(),
            paths.backup(),
            paths.clean_install_backup(),
            paths.archive(),
            paths.download_partial(),
        ] {
            assert_eq!(p.parent(), guard.root().parent());
            assert_ne!(p, guard.root());
        }
        let rel = RelativePath::new("data/global/x.txt").unwrap();
        assert_eq!(
            under(guard.root(), &rel),
            guard.root().join("data").join("global").join("x.txt")
        );
    }
}
