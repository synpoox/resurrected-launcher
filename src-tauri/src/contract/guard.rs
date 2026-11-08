//! Write guard: the launcher writes only a product folder under a real D2R
//! folder's `mods` folder, or a disposable test installation.

use std::path::{Path, PathBuf};

use super::{ErrorCode, LauncherError, RelativePath};

/// File that marks a product root as a disposable test installation.
pub const DISPOSABLE_SENTINEL: &str = ".resurrected-disposable";

/// Proof that a product root may be written. Install code takes a `&WriteGuard` for
/// every mutation of the product root, so a write without a passed check does not
/// compile. Install derives its staging/backup paths as siblings of
/// [`WriteGuard::root`]; the guard itself proves only the root, not those paths.
///
/// A root passes if it is a **mod folder** or holds the sentinel:
/// - A mod folder is `<D2R>\mods\<folder>` (phase B of the slice 3 gate, #61):
///   `<D2R>\D2R.exe` is a regular file, and neither the root nor any folder above
///   it is a link or junction. The root itself may be absent (a fresh install).
///   So the launcher writes a real D2R installation only inside its `mods`
///   folder, and never through a link.
/// - A root holding a regular [`DISPOSABLE_SENTINEL`] file is a disposable test
///   installation; fixtures create the root with it. The sentinel is an unknown
///   file to every operation, so it stays in place; clean install moves it as
///   specified at [`super::Boundary::CleanInstallBackupMade`], and recovery
///   re-guards with [`WriteGuard::check_with_backup`].
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

/// A Windows symlink, junction or other reparse point (or a Unix symlink).
fn is_link(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    meta.file_type().is_symlink()
}

/// `root` (absolute, as given) is `<D2R>\mods\<folder>`: the parent is named
/// `mods` (any case), `<D2R>\D2R.exe` is a regular file, and no existing folder
/// from the root up to the drive is a link or junction.
fn is_mod_folder(root: &Path) -> bool {
    let Ok(root) = std::path::absolute(root) else {
        return false;
    };
    let (Some(mods), Some(d2r)) = (root.parent(), root.parent().and_then(Path::parent)) else {
        return false;
    };
    if d2r.parent().is_none()
        || !mods
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("mods"))
    {
        return false;
    }
    let exe = std::fs::symlink_metadata(d2r.join("D2R.exe"));
    if !exe.is_ok_and(|m| m.is_file() && !is_link(&m)) {
        return false;
    }
    root.ancestors()
        .all(|dir| !std::fs::symlink_metadata(dir).is_ok_and(|m| is_link(&m)))
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
    // The root's own name obeys the same Windows rules as release paths.
    if name
        .to_str()
        .map(RelativePath::new)
        .is_none_or(|r| r.is_err())
    {
        return Err(invalid("product root name is not a safe Windows name"));
    }
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
    /// Issues a guard for `product_root` if it is a mod folder or holds the
    /// sentinel (see [`WriteGuard`]).
    ///
    /// Errors: [`ErrorCode::InvalidRequest`] for a relative path or an unsafe name;
    /// [`ErrorCode::ProductRootNotDisposable`] when the root is a link, or is
    /// neither a mod folder nor a root holding a regular sentinel file.
    pub fn check(product_root: &Path) -> Result<Self, LauncherError> {
        let root = canonical_root(product_root)?;
        if is_mod_folder(product_root) || has_sentinel(&root) {
            Ok(Self { root })
        } else {
            Err(refuse(
                ErrorCode::ProductRootNotDisposable,
                &root,
                "not a folder in a D2R mods folder, and no disposable sentinel",
            ))
        }
    }

    /// **Recovery only**; ordinary operations use [`WriteGuard::check`].
    /// For recovery of a clean install, where the root may be absent between
    /// `CleanInstallBackupMade` and `StagedMovedToLive`: passes if the root is a
    /// mod folder (present or absent), if it holds the sentinel, or if the root is
    /// absent and `backup` (the journal-recorded clean-install backup, a sibling
    /// of the root) holds it. The guard's root is still `product_root`.
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
        let root_absent = std::fs::symlink_metadata(&root).is_err();
        if is_mod_folder(product_root)
            || has_sentinel(&root)
            || (root_absent && has_sentinel(&backup))
        {
            Ok(Self { root })
        } else {
            Err(refuse(
                ErrorCode::ProductRootNotDisposable,
                &root,
                "not a folder in a D2R mods folder, and no disposable sentinel in root or backup",
            ))
        }
    }

    /// The absolute product root this guard permits writing, with its parent
    /// canonicalized (on Windows a verbatim `\\?\C:\...` path). Derive staging and
    /// backup paths by joining onto `root().parent()`. Never compare it with a path
    /// that did not come from a guard or the same canonicalization, since spelling
    /// differs (`C:\` vs `\\?\C:\`, case, `..`); strip the `\\?\` prefix only for
    /// display.
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

    #[test]
    fn write_guard_refuses_unsafe_root_names() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["CON", "PD2R.", "PD2R ", "aux.txt"] {
            assert_eq!(
                code(WriteGuard::check(&dir.path().join(name))),
                ErrorCode::InvalidRequest,
                "{name:?}"
            );
        }
    }

    /// Skipped tests are loud: visible with `--nocapture` and in CI logs.
    fn skip(why: &str) {
        eprintln!(
            "SKIPPED {}: {why}",
            std::thread::current().name().unwrap_or("?")
        );
    }

    #[cfg(windows)]
    #[test]
    fn write_guard_refuses_junctioned_product_root() {
        let (dir, real) = disposable_root();
        let link = dir.path().join("junction");
        let made = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(&link)
            .arg(&real)
            .output()
            .is_ok_and(|o| o.status.success());
        if !made {
            skip("mklink /J failed");
            return;
        }
        assert_eq!(
            code(WriteGuard::check(&link)),
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
            skip("this account cannot create symlinks");
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
            skip("this account cannot create symlinks");
            return;
        }
        assert_eq!(
            code(WriteGuard::check(&link)),
            ErrorCode::ProductRootNotDisposable
        );
    }

    /// `<tmp>\D2R\mods` with a regular `D2R\D2R.exe`; returns (tmp, D2R folder).
    fn d2r_folder() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let d2r = dir.path().join("D2R");
        std::fs::create_dir_all(d2r.join("mods")).unwrap();
        std::fs::write(d2r.join("D2R.exe"), b"MZ").unwrap();
        (dir, d2r)
    }

    #[test]
    fn write_guard_allows_a_mod_folder_of_a_d2r_folder_present_or_absent() {
        let (_dir, d2r) = d2r_folder();
        let root = d2r.join("mods").join("PD2Resurrected");
        // Absent: a fresh install creates it.
        let guard = WriteGuard::check(&root).unwrap();
        assert_eq!(guard.root().file_name(), root.file_name());
        std::fs::create_dir(&root).unwrap();
        let guard = WriteGuard::check(&root).unwrap();
        assert_eq!(guard.root(), std::fs::canonicalize(&root).unwrap());
        // `mods` in any case, as Windows does.
        std::fs::rename(d2r.join("mods"), d2r.join("MODS")).unwrap();
        WriteGuard::check(&d2r.join("MODS").join("PD2Resurrected")).unwrap();
    }

    #[test]
    fn write_guard_refuses_folders_outside_a_d2r_mods_folder() {
        let (dir, d2r) = d2r_folder();
        for root in [
            d2r.join("PD2Resurrected"),                     // not under mods
            d2r.join("mods").join("PD2R").join("nested"),   // two levels down
            d2r.join("Data").join("PD2Resurrected"),        // another folder
            dir.path().join("mods").join("PD2Resurrected"), // no D2R.exe above
        ] {
            std::fs::create_dir_all(root.parent().unwrap()).unwrap();
            assert_eq!(
                code(WriteGuard::check(&root)),
                ErrorCode::ProductRootNotDisposable,
                "{}",
                root.display()
            );
        }
        // D2R.exe must be a regular file.
        std::fs::remove_file(d2r.join("D2R.exe")).unwrap();
        std::fs::create_dir(d2r.join("D2R.exe")).unwrap();
        assert_eq!(
            code(WriteGuard::check(&d2r.join("mods").join("PD2Resurrected"))),
            ErrorCode::ProductRootNotDisposable
        );
    }

    #[cfg(windows)]
    fn junction(link: &Path, target: &Path) -> bool {
        std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .output()
            .is_ok_and(|o| o.status.success())
    }

    /// A junction anywhere on the path (the mod folder, `mods`, the D2R folder
    /// or above it) refuses the write: it would land somewhere else.
    #[cfg(windows)]
    #[test]
    fn write_guard_refuses_a_mod_folder_reached_through_a_junction() {
        let (dir, d2r) = d2r_folder();
        let real_root = d2r.join("mods").join("Real");
        std::fs::create_dir(&real_root).unwrap();
        let linked_root = d2r.join("mods").join("PD2Resurrected");
        let linked_d2r = dir.path().join("LinkedD2R");
        let other = tempfile::tempdir().unwrap();
        let linked_mods_d2r = other.path().join("D2R");
        std::fs::create_dir(&linked_mods_d2r).unwrap();
        std::fs::write(linked_mods_d2r.join("D2R.exe"), b"MZ").unwrap();
        if !(junction(&linked_root, &real_root)
            && junction(&linked_d2r, &d2r)
            && junction(&linked_mods_d2r.join("mods"), &d2r.join("mods")))
        {
            skip("mklink /J failed");
            return;
        }
        for root in [
            linked_root,
            linked_d2r.join("mods").join("Real"),
            linked_mods_d2r.join("mods").join("Real"),
        ] {
            assert_eq!(
                code(WriteGuard::check(&root)),
                ErrorCode::ProductRootNotDisposable,
                "{}",
                root.display()
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn write_guard_refuses_a_linked_d2r_exe() {
        let (_dir, d2r) = d2r_folder();
        let real = d2r.join("real.exe");
        std::fs::rename(d2r.join("D2R.exe"), &real).unwrap();
        if symlink_file(&real, d2r.join("D2R.exe")).is_err() {
            skip("this account cannot create symlinks");
            return;
        }
        assert_eq!(
            code(WriteGuard::check(&d2r.join("mods").join("PD2Resurrected"))),
            ErrorCode::ProductRootNotDisposable
        );
    }

    #[test]
    fn write_guard_recovers_a_mod_folder_whose_root_is_absent() {
        let (_dir, d2r) = d2r_folder();
        let root = d2r.join("mods").join("PD2Resurrected");
        let backup = d2r.join("mods").join("PD2Resurrected.clean-install-backup");
        std::fs::create_dir(&backup).unwrap();
        let guard = WriteGuard::check_with_backup(&root, &backup).unwrap();
        assert_eq!(guard.root().file_name(), root.file_name());
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
