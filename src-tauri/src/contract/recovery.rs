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

/// One copy the journal planned (sentinel into staging, player files carried
/// forward). Copies are written to a temporary sibling and renamed into place, so a
/// present destination is a complete copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedCopy {
    pub from: PathBuf,
    pub to: PathBuf,
}

impl PlannedCopy {
    /// Destination present: done (undo deletes it). Destination absent and source
    /// present: not done. Both absent: conflict.
    pub fn on_disk(&self) -> EntryOnDisk {
        match (present(&self.from), present(&self.to)) {
            (_, true) => EntryOnDisk::Done,
            (true, false) => EntryOnDisk::NotDone,
            (false, false) => EntryOnDisk::Conflict,
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
    fn both_or_neither_side_present_is_a_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        let rename = PlannedRename {
            from: a.clone(),
            to: b.clone(),
        };
        let copy = PlannedCopy {
            from: a.clone(),
            to: b.clone(),
        };
        assert_eq!(rename.on_disk(), EntryOnDisk::Conflict);
        assert_eq!(copy.on_disk(), EntryOnDisk::Conflict);
        std::fs::write(&a, b"").unwrap();
        assert_eq!(copy.on_disk(), EntryOnDisk::NotDone);
        std::fs::write(&b, b"").unwrap();
        assert_eq!(rename.on_disk(), EntryOnDisk::Conflict);
        assert_eq!(copy.on_disk(), EntryOnDisk::Done);
    }
}
