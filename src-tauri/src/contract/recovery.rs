//! How recovery decides one planned entry of an interrupted step: from disk, never
//! from the journal's per-entry progress alone, which may lag disk by one entry
//! ([`super::FaultPoint::AfterEntry`]).

use std::path::{Path, PathBuf};

/// What disk says about one planned entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryOnDisk {
    /// The entry's operation has not happened: undoing it is a no-op, redoing it is due.
    NotDone,
    /// The entry's operation has happened (journaled or not): rollback must reverse it.
    Done,
    /// Disk matches neither state (both or neither side present). Recovery stops with
    /// [`super::ErrorCode::RecoveryRequired`] and changes nothing further.
    Conflict,
}

fn present(p: &Path) -> bool {
    // symlink_metadata: a link counts as present without being followed.
    std::fs::symlink_metadata(p).is_ok()
}

/// One rename the journal planned for a multi-entry step (file, directory or a
/// whole root, such as `L` -> `K`). The journal writes the complete planned list at
/// [`super::FaultPoint::BeforeStep`], before the first entry is done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedRename {
    pub from: PathBuf,
    pub to: PathBuf,
}

impl PlannedRename {
    /// `from` present and `to` absent: not done. `from` absent and `to` present: done.
    pub fn on_disk(&self) -> EntryOnDisk {
        match (present(&self.from), present(&self.to)) {
            (true, false) => EntryOnDisk::NotDone,
            (false, true) => EntryOnDisk::Done,
            _ => EntryOnDisk::Conflict,
        }
    }

    /// The rename that undoes this one.
    pub fn reversed(&self) -> Self {
        Self {
            from: self.to.clone(),
            to: self.from.clone(),
        }
    }
}

/// One file copy the journal planned (the sentinel `L` -> `S`, player files carried
/// forward `K` -> new `L`). The copy is written to `temp`, a sibling of `to`, flushed,
/// then renamed over `to`, so `to` is never half-written.
///
/// The destination may already exist before the copy (a shipped default at the
/// path of the player's edited file), so presence proves nothing: the copy is done
/// only when `to` has the same bytes as `from`.
///
/// Copies are never reversed one by one: their destinations live in `S` or the new
/// `L`, which rollback discards as a whole. A crash between writing `temp` and the
/// rename leaves `temp` behind; recovery and `CleanedUp` delete any planned `temp`
/// that is present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedCopy {
    pub from: PathBuf,
    pub to: PathBuf,
    pub temp: PathBuf,
}

/// Byte-for-byte equality of two regular files (stronger than comparing hashes).
fn same_file_contents(a: &Path, b: &Path) -> std::io::Result<bool> {
    use std::io::Read;
    let (ma, mb) = (std::fs::symlink_metadata(a)?, std::fs::symlink_metadata(b)?);
    if !ma.is_file() || !mb.is_file() || ma.len() != mb.len() {
        return Ok(false);
    }
    let (mut fa, mut fb) = (std::fs::File::open(a)?, std::fs::File::open(b)?);
    let (mut ba, mut bb) = (vec![0u8; 64 * 1024], vec![0u8; 64 * 1024]);
    loop {
        let n = fa.read(&mut ba)?;
        if n == 0 {
            return Ok(true);
        }
        fb.read_exact(&mut bb[..n])?;
        if ba[..n] != bb[..n] {
            return Ok(false);
        }
    }
}

impl PlannedCopy {
    /// `to` has exactly `from`'s bytes: done. Otherwise (absent, or holding other
    /// content such as a shipped default): not done, and performing it replaces
    /// `to`. `from` missing, or an I/O error while comparing: conflict.
    pub fn on_disk(&self) -> EntryOnDisk {
        if !present(&self.from) {
            return EntryOnDisk::Conflict;
        }
        if !present(&self.to) {
            return EntryOnDisk::NotDone;
        }
        match same_file_contents(&self.from, &self.to) {
            Ok(true) => EntryOnDisk::Done,
            Ok(false) => EntryOnDisk::NotDone,
            Err(_) => EntryOnDisk::Conflict,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Crash at `LiveMovedToBackup` `AfterEntry(1)`: entries 0 and 1 are in `B` but
    /// the journal records only entry 0. Deciding from disk finds both.
    #[test]
    fn rollback_after_journal_lag_finds_the_unjournaled_entry_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let (live, backup) = (dir.path().join("L"), dir.path().join("B"));
        std::fs::create_dir_all(&live).unwrap();
        std::fs::create_dir_all(&backup).unwrap();
        let planned: Vec<PlannedRename> = ["a.txt", "b.txt", "c.txt"]
            .iter()
            .map(|n| {
                std::fs::write(live.join(n), n).unwrap();
                PlannedRename {
                    from: live.join(n),
                    to: backup.join(n),
                }
            })
            .collect();
        for entry in &planned[..2] {
            std::fs::rename(&entry.from, &entry.to).unwrap();
        }
        let journaled_done = 1; // lags disk by one

        let states: Vec<_> = planned.iter().map(PlannedRename::on_disk).collect();
        assert_eq!(
            states,
            [EntryOnDisk::Done, EntryOnDisk::Done, EntryOnDisk::NotDone]
        );
        assert!(states.iter().filter(|s| **s == EntryOnDisk::Done).count() > journaled_done);

        // BackupRestoredToLive: reverse every Done entry; running it twice is harmless.
        for _ in 0..2 {
            for entry in planned.iter().filter(|e| e.on_disk() == EntryOnDisk::Done) {
                let undo = entry.reversed();
                std::fs::rename(&undo.from, &undo.to).unwrap();
            }
        }
        for n in ["a.txt", "b.txt", "c.txt"] {
            assert_eq!(std::fs::read_to_string(live.join(n)).unwrap(), n);
        }
    }

    /// Crash at `CleanInstallBackupMade` `AfterEntry(1)`: `L` was renamed to `K` but
    /// the journal does not know. The whole-root rename is still seen as done.
    #[test]
    fn clean_install_root_rename_is_seen_as_done_without_journal_record() {
        let dir = tempfile::tempdir().unwrap();
        let (live, keep) = (dir.path().join("PD2R"), dir.path().join("PD2R.backup"));
        std::fs::create_dir(&live).unwrap();
        let entry = PlannedRename {
            from: live.clone(),
            to: keep.clone(),
        };
        assert_eq!(entry.on_disk(), EntryOnDisk::NotDone);
        std::fs::rename(&live, &keep).unwrap();
        assert_eq!(entry.on_disk(), EntryOnDisk::Done);
        assert_eq!(entry.reversed().on_disk(), EntryOnDisk::NotDone);
    }

    #[test]
    fn rename_with_both_or_neither_side_present_is_a_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        let rename = PlannedRename {
            from: a.clone(),
            to: b.clone(),
        };
        assert_eq!(rename.on_disk(), EntryOnDisk::Conflict);
        std::fs::write(&a, b"").unwrap();
        std::fs::write(&b, b"").unwrap();
        assert_eq!(rename.on_disk(), EntryOnDisk::Conflict);
    }

    fn copy_in(dir: &Path, from: &str, to: &str) -> PlannedCopy {
        PlannedCopy {
            from: dir.join(from),
            to: dir.join(to),
            temp: dir.join(format!("{to}.resurrected-tmp")),
        }
    }

    /// Clean install carries the player's edited filter from `K` into the new `L`,
    /// where the shipped default already sits at the same path. The default must not
    /// read as a finished copy.
    #[test]
    fn carried_forward_player_file_over_shipped_default_is_not_done_until_copied() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("edited.filter"), b"player edit").unwrap();
        std::fs::write(dir.path().join("live.filter"), b"shipped default").unwrap();
        let copy = copy_in(dir.path(), "edited.filter", "live.filter");
        assert_eq!(copy.on_disk(), EntryOnDisk::NotDone);

        // Same length, different bytes: still not done.
        std::fs::write(dir.path().join("live.filter"), b"player edjt").unwrap();
        assert_eq!(copy.on_disk(), EntryOnDisk::NotDone);

        std::fs::copy(&copy.from, &copy.temp).unwrap();
        std::fs::rename(&copy.temp, &copy.to).unwrap();
        assert_eq!(copy.on_disk(), EntryOnDisk::Done);
    }

    /// The sentinel copy `L` -> `S` follows the same rule.
    #[test]
    fn sentinel_copy_is_done_only_with_matching_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let copy = copy_in(dir.path(), "L.sentinel", "S.sentinel");
        assert_eq!(copy.on_disk(), EntryOnDisk::Conflict, "source missing");
        std::fs::write(&copy.from, b"").unwrap();
        assert_eq!(copy.on_disk(), EntryOnDisk::NotDone);
        std::fs::write(&copy.to, b"x").unwrap();
        assert_eq!(copy.on_disk(), EntryOnDisk::NotDone);
        std::fs::write(&copy.to, b"").unwrap();
        assert_eq!(copy.on_disk(), EntryOnDisk::Done);
    }

    /// Update replacing `x`: `L/x -> B/x`, then `S/x -> L/x`. Restoring `B/x` reads
    /// `Conflict` while the new `L/x` is in place, which is why
    /// `NewEntriesRemovedFromLive` must run before `BackupRestoredToLive`.
    #[test]
    fn backup_restore_of_a_replaced_file_needs_new_entries_removed_first() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        for sub in ["L", "B", "S"] {
            std::fs::create_dir(d.join(sub)).unwrap();
        }
        std::fs::write(d.join("L/x"), b"old").unwrap();
        std::fs::write(d.join("S/x"), b"new").unwrap();
        let to_backup = PlannedRename {
            from: d.join("L/x"),
            to: d.join("B/x"),
        };
        let to_live = PlannedRename {
            from: d.join("S/x"),
            to: d.join("L/x"),
        };
        std::fs::rename(&to_backup.from, &to_backup.to).unwrap();
        std::fs::rename(&to_live.from, &to_live.to).unwrap();

        assert_eq!(to_backup.on_disk(), EntryOnDisk::Conflict);
        // NewEntriesRemovedFromLive
        assert_eq!(to_live.on_disk(), EntryOnDisk::Done);
        let undo = to_live.reversed();
        std::fs::rename(&undo.from, &undo.to).unwrap();
        // BackupRestoredToLive
        assert_eq!(to_backup.on_disk(), EntryOnDisk::Done);
        let undo = to_backup.reversed();
        std::fs::rename(&undo.from, &undo.to).unwrap();
        assert_eq!(std::fs::read(d.join("L/x")).unwrap(), b"old");
    }
}
