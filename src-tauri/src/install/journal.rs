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
    RelativePath,
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

/// A real directory: not a symlink, junction or any other reparse point.
pub(crate) fn real_dir(m: &fs::Metadata) -> bool {
    #[cfg(windows)]
    let reparse = {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    };
    #[cfg(not(windows))]
    let reparse = false;
    m.is_dir() && !m.file_type().is_symlink() && !reparse
}

/// `base` joined with `rel`, refusing when `base` or any existing dir between it
/// and the entry is not a real directory, so a junction or link inside the root
/// can never carry a planned move outside the guarded tree. Missing dirs are fine
/// (the step creates them).
pub(crate) fn confined(base: &Path, rel: &RelativePath) -> Result<PathBuf, LauncherError> {
    let parts: Vec<_> = rel.as_str().split('/').collect();
    // Ok(false): missing, so nothing below it exists either.
    let real = |p: &Path| match fs::symlink_metadata(p) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Ok(m) if real_dir(&m) => Ok(true),
        _ => Err(LauncherError::new(
            ErrorCode::ProductRootNotDisposable,
            format!(
                "refusing to write through {}: not a real directory",
                p.display()
            ),
        )),
    };
    let mut path = base.to_path_buf();
    if real(&path)? {
        for dir in &parts[..parts.len() - 1] {
            path.push(dir);
            if !real(&path)? {
                break;
            }
        }
    }
    Ok(parts.iter().fold(base.to_path_buf(), |p, c| p.join(c)))
}

const CLEAN_BACKUP_INFIX: &str = ".clean-install-backup-";

/// The labelled clean-install backup `K` for `root`, stamped with `secs`.
pub(crate) fn clean_backup_path(root: &Path, secs: u64) -> PathBuf {
    sibling(root, &format!("{CLEAN_BACKUP_INFIX}{secs}"))
}

/// Whether `keep` is `<root name>.clean-install-backup-<digits>` beside `root`.
fn is_clean_backup_of(root: &Path, keep: &Option<PathBuf>) -> bool {
    let Some(keep) = keep else { return false };
    let prefix = sibling(root, CLEAN_BACKUP_INFIX);
    let (Some(prefix), Some(name)) = (
        prefix.file_name().and_then(|n| n.to_str()),
        keep.file_name().and_then(|n| n.to_str()),
    ) else {
        return false;
    };
    keep.parent() == root.parent()
        && name
            .strip_prefix(prefix)
            .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
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
                    flush(&OpenOptions::new().write(true).open(&c.temp)?)?;
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
        // Every path recovery acts on must be the one this root derives, so an
        // edited journal cannot point deletes or renames elsewhere.
        let mut want = Paths::for_root(root);
        want.clean_backup = journal.paths.clean_backup.clone();
        let clean = journal.kind == OperationKind::CleanInstall;
        if journal.paths != want || clean != is_clean_backup_of(root, &want.clean_backup) {
            return Err(unreadable(
                "its paths do not belong to this product root".into(),
            ));
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

thread_local! {
    /// Unit tests simulate crashes with the fault injector, never by power loss, so
    /// they skip the (slow) flushes unless a test turns them on for its thread;
    /// every release build flushes.
    static FLUSH: std::cell::Cell<bool> = const { std::cell::Cell::new(!cfg!(test)) };
}

fn flushing() -> bool {
    FLUSH.with(|f| f.get())
}

/// Test-only: runs the real flush path on this thread.
#[cfg(test)]
pub(crate) fn flush_on_this_thread() {
    FLUSH.with(|f| f.set(true));
}

fn flush(f: &File) -> io::Result<()> {
    if flushing() {
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
    let through = if flushing() {
        MOVEFILE_WRITE_THROUGH
    } else {
        0
    };
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

    /// The release path: real flushes, write-through renames and a flushed copy.
    #[test]
    fn durable_writes_copies_and_renames_flush_for_real() {
        flush_on_this_thread();
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::for_root(&dir.path().join("PD2R"));
        let journal = dir.path().join("j.json");
        write_atomic(&journal, b"one").unwrap();
        write_atomic(&journal, b"two").unwrap();
        assert_eq!(fs::read(&journal).unwrap(), b"two");

        let (from, to) = (
            dir.path().join("edited.filter"),
            dir.path().join("x/live.filter"),
        );
        fs::write(&from, b"player edit").unwrap();
        let copy = Entry::Copy(PlannedCopy::plan(from, to.clone()).unwrap());
        copy.apply(&paths).unwrap();
        assert_eq!(copy.on_disk(), EntryOnDisk::Done);

        let moved = dir.path().join("moved.filter");
        durable_rename(&to, &moved, false).unwrap();
        assert_eq!(fs::read(moved).unwrap(), b"player edit");
    }

    #[test]
    fn confined_refuses_a_non_directory_between_root_and_entry() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("data")).unwrap();
        fs::write(dir.path().join("file"), b"").unwrap();
        let rel = |s: &str| RelativePath::new(s).unwrap();
        assert!(confined(dir.path(), &rel("data/missing/a.txt")).is_ok());
        let err = confined(dir.path(), &rel("file/a.txt")).unwrap_err();
        assert_eq!(err.code, ErrorCode::ProductRootNotDisposable);
    }

    #[test]
    fn journal_naming_paths_of_another_root_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("PD2R");
        let mut paths = Paths::for_root(&root);
        let journal = |paths: Paths, kind| Journal {
            version: JOURNAL_VERSION,
            kind,
            paths,
            plan: serde_json::from_value(serde_json::json!({
                "kind": "update", "to_backup": [], "to_live": [], "carry_forward": [],
                "prune_if_empty": [], "record": "remove",
                "space": {"download": 0, "staging": 0, "recovery_copy": 0}
            }))
            .unwrap(),
            direction: Direction::Forward,
            reached: None,
            rollback_reason: None,
            steps: vec![],
        };
        journal(paths.clone(), OperationKind::Update)
            .save()
            .unwrap();
        assert!(Journal::load(&root).unwrap().is_some());

        paths.staging = dir.path().join("saves");
        journal(paths.clone(), OperationKind::Update)
            .save()
            .unwrap();
        assert_eq!(
            Journal::load(&root).unwrap_err().code,
            ErrorCode::RecoveryRequired
        );

        let mut clean = Paths::for_root(&root);
        clean.clean_backup = Some(dir.path().join("saves"));
        journal(clean.clone(), OperationKind::CleanInstall)
            .save()
            .unwrap();
        assert_eq!(
            Journal::load(&root).unwrap_err().code,
            ErrorCode::RecoveryRequired
        );
        clean.clean_backup = Some(clean_backup_path(&root, 42));
        journal(clean, OperationKind::CleanInstall).save().unwrap();
        assert!(Journal::load(&root).unwrap().is_some());
    }

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
