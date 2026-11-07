//! The durable transaction journal (#36): one JSON file beside the product root,
//! replaced atomically (write temp, flush, write-through rename), plus the planned
//! entries of every multi-entry step and the filesystem primitives they use.
//!
//! Layout, all siblings of the product root `L` (same volume, see
//! [`crate::contract::Boundary`]): `<name>.resurrected-journal.json`, its per-entry
//! progress hint `<name>.resurrected-journal.progress`, staging
//! `<name>.resurrected-staging`, backup `<name>.resurrected-backup`, the archive
//! `<name>.resurrected-archive`, the lock `<name>.resurrected-lock`, and for clean
//! install the labelled backup `<name>.clean-install-backup-<unix seconds>`.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::transaction::Plan;
use crate::contract::{
    Boundary, EntryOnDisk, ErrorCode, LauncherError, OperationKind, PlannedCopy, PlannedRename,
};

/// Bumped when the journal format changes; an unknown version is never guessed at.
pub const JOURNAL_VERSION: u32 = 1;

/// Maps an I/O failure to the contract error a player can act on.
pub(crate) fn io_error(what: &str, path: &Path, e: io::Error) -> LauncherError {
    let code = match e.kind() {
        io::ErrorKind::PermissionDenied => ErrorCode::AccessDenied,
        io::ErrorKind::StorageFull => ErrorCode::InsufficientSpace,
        _ => ErrorCode::Internal,
    };
    LauncherError::new(code, format!("{what} {}: {e}", path.display()))
}

/// The product root with its parent canonicalized, spelled like
/// [`crate::contract::WriteGuard::root`], so every path derived from it matches.
pub(crate) fn canonical_root(root: &Path) -> Result<PathBuf, LauncherError> {
    let invalid = || {
        LauncherError::new(
            ErrorCode::InvalidRequest,
            format!("bad product root {}", root.display()),
        )
    };
    let (Some(parent), Some(name)) = (root.parent(), root.file_name()) else {
        return Err(invalid());
    };
    if !root.is_absolute() {
        return Err(invalid());
    }
    let parent = fs::canonicalize(parent).map_err(|e| io_error("reading", parent, e))?;
    Ok(parent.join(name))
}

/// `<parent>/<root name><suffix>`.
pub(crate) fn sibling(root: &Path, suffix: &str) -> PathBuf {
    let mut name = root.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    root.with_file_name(name)
}

/// Every path a transaction touches outside the player's files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paths {
    /// Live root `L`, as the [`crate::contract::WriteGuard`] spells it.
    pub root: PathBuf,
    /// Staging `S`; extraction (#35) fills it.
    pub staging: PathBuf,
    /// Backup `B`, the recovery copy of replaced or removed managed files.
    pub backup: PathBuf,
    /// Where the download (#35) writes the archive.
    pub archive: PathBuf,
    /// Clean install only: the labelled backup `K`, never silently deleted.
    pub clean_backup: Option<PathBuf>,
}

impl Paths {
    pub(crate) fn for_root(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            staging: sibling(root, ".resurrected-staging"),
            backup: sibling(root, ".resurrected-backup"),
            archive: sibling(root, ".resurrected-archive"),
            clean_backup: None,
        }
    }

    pub(crate) fn journal(&self) -> PathBuf {
        journal_path(&self.root)
    }

    /// Scratch the transaction owns outright and may delete whole.
    fn is_scratch(&self, p: &Path) -> bool {
        [&self.staging, &self.backup, &self.archive].contains(&&p.to_path_buf())
    }
}

pub(crate) fn journal_path(root: &Path) -> PathBuf {
    sibling(root, ".resurrected-journal.json")
}

fn progress_path(root: &Path) -> PathBuf {
    sibling(root, ".resurrected-journal.progress")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Forward,
    Rollback,
}

/// One planned entry of a multi-entry step, decided from disk by [`Entry::on_disk`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum Entry {
    Rename(PlannedRename),
    Copy(PlannedCopy),
    /// A live-root dir created for an incoming entry; rollback removes it if empty.
    MakeDir {
        path: PathBuf,
    },
    /// Removed only while empty; a non-empty dir is left alone (and counts as done).
    RemoveDir {
        path: PathBuf,
    },
    /// Scratch (`S`, `B`, the archive) or a reserved copy temp, deleted whole.
    Delete {
        path: PathBuf,
    },
}

fn present(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok()
}

fn is_real_dir(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| m.is_dir())
}

impl Entry {
    pub fn on_disk(&self) -> EntryOnDisk {
        match self {
            Entry::Rename(r) => r.on_disk(),
            Entry::Copy(c) => c.on_disk(),
            Entry::MakeDir { path } if is_real_dir(path) => EntryOnDisk::Done,
            Entry::MakeDir { path } if present(path) => EntryOnDisk::Conflict,
            Entry::MakeDir { .. } => EntryOnDisk::NotDone,
            Entry::RemoveDir { path } => {
                let empty =
                    is_real_dir(path) && fs::read_dir(path).is_ok_and(|mut d| d.next().is_none());
                if empty {
                    EntryOnDisk::NotDone
                } else {
                    EntryOnDisk::Done
                }
            }
            Entry::Delete { path } if present(path) => EntryOnDisk::NotDone,
            Entry::Delete { .. } => EntryOnDisk::Done,
        }
    }

    /// The entry that undoes this one during rollback. Copies have none: their
    /// destinations are in `S` or the new `L`, which rollback discards whole.
    pub fn reversed(&self) -> Option<Entry> {
        match self {
            Entry::Rename(r) => Some(Entry::Rename(r.reversed())),
            Entry::MakeDir { path } => Some(Entry::RemoveDir { path: path.clone() }),
            _ => None,
        }
    }

    /// Performs a `NotDone` entry. `paths` confines deletion to the transaction's
    /// own scratch and reserved temps.
    pub fn apply(&self, paths: &Paths) -> Result<(), LauncherError> {
        match self {
            Entry::Rename(r) => {
                if let Some(parent) = r.to.parent() {
                    fs::create_dir_all(parent).map_err(|e| io_error("creating", parent, e))?;
                }
                durable_rename(&r.from, &r.to, false).map_err(|e| io_error("moving", &r.from, e))
            }
            Entry::Copy(c) => {
                let copy = || -> io::Result<()> {
                    if let Some(parent) = c.to.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::copy(&c.from, &c.temp)?;
                    flush(&File::open(&c.temp)?)?;
                    durable_rename(&c.temp, &c.to, true)
                };
                copy().map_err(|e| io_error("copying", &c.from, e))
            }
            Entry::MakeDir { path } => {
                fs::create_dir(path).map_err(|e| io_error("creating", path, e))
            }
            Entry::RemoveDir { path } => {
                fs::remove_dir(path).map_err(|e| io_error("removing", path, e))
            }
            Entry::Delete { path } => {
                let temp = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(crate::contract::COPY_TEMP_SUFFIX));
                if !paths.is_scratch(path) && !temp {
                    return Err(LauncherError::new(
                        ErrorCode::Internal,
                        format!("refusing to delete non-scratch {}", path.display()),
                    ));
                }
                let r = if is_real_dir(path) {
                    fs::remove_dir_all(path)
                } else {
                    fs::remove_file(path)
                };
                r.map_err(|e| io_error("deleting", path, e))
            }
        }
    }
}

/// A multi-entry step's complete planned entry list, written at `BeforeStep`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepRecord {
    pub boundary: Boundary,
    pub entries: Vec<Entry>,
}

/// The durable record of one transaction. Recovery reads only this and disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Journal {
    pub version: u32,
    pub kind: OperationKind,
    pub paths: Paths,
    pub plan: Plan,
    pub direction: Direction,
    /// The last boundary reached in `direction` (a forward boundary right after the
    /// switch to rollback, until `RollbackStarted` is reached).
    pub reached: Option<Boundary>,
    pub rollback_reason: Option<String>,
    /// Planned entries of every multi-entry step started, latest last.
    pub steps: Vec<StepRecord>,
}

impl Journal {
    /// Reads the journal of `root`, if any. A journal that cannot be read or parsed,
    /// or has an unknown version, is [`ErrorCode::RecoveryRequired`]: it is never
    /// guessed at or discarded.
    pub fn load(root: &Path) -> Result<Option<Self>, LauncherError> {
        let path = journal_path(root);
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io_error("reading journal", &path, e)),
        };
        let unreadable = |why: String| {
            LauncherError::new(
                ErrorCode::RecoveryRequired,
                format!("journal {} is unreadable: {why}", path.display()),
            )
        };
        let journal: Journal =
            serde_json::from_slice(&bytes).map_err(|e| unreadable(e.to_string()))?;
        if journal.version != JOURNAL_VERSION {
            return Err(unreadable(format!("version {}", journal.version)));
        }
        if journal.paths.root != root {
            return Err(unreadable("it names another product root".into()));
        }
        Ok(Some(journal))
    }

    pub fn save(&self) -> Result<(), LauncherError> {
        let path = self.paths.journal();
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| LauncherError::new(ErrorCode::Internal, e.to_string()))?;
        write_atomic(&path, &bytes).map_err(|e| io_error("writing journal", &path, e))
    }

    /// Per-entry progress: a hint for diagnostics, never read by recovery.
    pub fn save_progress(&self, boundary: Boundary, done: usize) -> Result<(), LauncherError> {
        let path = progress_path(&self.paths.root);
        let text = format!("{boundary:?} {done}\n");
        write_atomic(&path, text.as_bytes()).map_err(|e| io_error("writing progress", &path, e))
    }

    /// Closes the journal after `CleanedUp`.
    pub fn close(&self) -> Result<(), LauncherError> {
        for path in [progress_path(&self.paths.root), self.paths.journal()] {
            match fs::remove_file(&path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => {
                    return Err(io_error("closing journal", &path, e))
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Unit tests simulate crashes with the fault injector, never by power loss, so
/// they skip the (slow) flushes; every release build flushes.
const FLUSH: bool = !cfg!(test);

fn flush(f: &File) -> io::Result<()> {
    if FLUSH {
        f.sync_all()?;
    }
    Ok(())
}

/// Write `bytes` to a temp beside `path`, flush it to disk, then atomically
/// replace `path` with a write-through rename.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = sibling(path, ".tmp");
    let mut f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temp)?;
    f.write_all(bytes)?;
    flush(&f)?;
    drop(f);
    durable_rename(&temp, path, true)
}

/// Rename that has reached disk when it returns. Without `replace` an existing
/// destination is an error, so a planned rename never clobbers anything.
#[cfg(windows)]
pub(crate) fn durable_rename(from: &Path, to: &Path, replace: bool) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    let wide = |p: &Path| -> Vec<u16> { p.as_os_str().encode_wide().chain(Some(0)).collect() };
    let through = if FLUSH { MOVEFILE_WRITE_THROUGH } else { 0 };
    let flags = through
        | if replace {
            MOVEFILE_REPLACE_EXISTING
        } else {
            0
        };
    // SAFETY: both arguments are NUL-terminated UTF-16 buffers alive for the call.
    if unsafe { MoveFileExW(wide(from).as_ptr(), wide(to).as_ptr(), flags) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
pub(crate) fn durable_rename(from: &Path, to: &Path, replace: bool) -> io::Result<()> {
    // shortcut: check-then-rename races; the product targets Windows only.
    if !replace && present(to) {
        return Err(io::Error::from(io::ErrorKind::AlreadyExists));
    }
    fs::rename(from, to)?;
    if let Some(parent) = to.parent() {
        flush(&File::open(parent)?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_write_replaces_atomically_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("j.json");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn planned_rename_never_clobbers_an_existing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        fs::write(&a, b"a").unwrap();
        fs::write(&b, b"player").unwrap();
        assert!(durable_rename(&a, &b, false).is_err());
        assert_eq!(fs::read(&b).unwrap(), b"player");
    }

    #[test]
    fn delete_entry_refuses_anything_but_scratch_and_reserved_temps() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::for_root(&dir.path().join("PD2R"));
        let player = dir.path().join("saves");
        fs::create_dir(&player).unwrap();
        let err = Entry::Delete {
            path: player.clone(),
        }
        .apply(&paths)
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::Internal);
        assert!(player.exists());
        fs::create_dir(&paths.staging).unwrap();
        Entry::Delete {
            path: paths.staging.clone(),
        }
        .apply(&paths)
        .unwrap();
        assert!(!paths.staging.exists());
    }

    #[test]
    fn remove_dir_entry_leaves_a_non_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().join("d");
        fs::create_dir(&d).unwrap();
        let entry = Entry::RemoveDir { path: d.clone() };
        assert_eq!(entry.on_disk(), EntryOnDisk::NotDone);
        fs::write(d.join("plugin.dll"), b"").unwrap();
        assert_eq!(entry.on_disk(), EntryOnDisk::Done);
    }
}
