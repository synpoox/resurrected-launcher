//! Mutation boundaries of the journaled update transaction (#8 pipeline:
//! inspect/plan -> download/verify -> confined staging -> preserved-file and
//! compatibility checks -> journaled activation -> verify/commit -> safe cleanup).
//!
//! # Activation model
//!
//! Live root `L` is the product folder. Staging `S`, backup `B`, the clean-install
//! backup `K` and the downloaded archive are siblings of `L` (same parent, so the
//! same filesystem), derived by Install from [`super::WriteGuard::root`]. Activation
//! moves entries by rename, never by copying over live files:
//!
//! 1. every live managed entry the operation replaces or removes is renamed from `L`
//!    into `B` ([`Boundary::LiveMovedToBackup`]); `B` is the recovery copy of the
//!    previous installation, so the old bytes are kept without needing re-download;
//! 2. every new entry is renamed from `S` into `L` ([`Boundary::StagedMovedToLive`]).
//!
//! Player files, edited configs and unknown/plugin files (including the
//! [`super::DISPOSABLE_SENTINEL`]) are in neither set: they stay in place and are
//! never moved, renamed or deleted. Clean install is the exception: with consent,
//! the whole old folder becomes `K`, a labelled backup that is never silently deleted.
//!
//! # Multi-entry steps
//!
//! A step that renames, copies or deletes several entries is several Windows
//! operations, not one atomic act. The journal writes the step's complete planned
//! entry list ([`super::PlannedRename`], [`super::PlannedCopy`]) at `BeforeStep`,
//! processes entries in that order, records per-entry progress, and the injector
//! is consulted around every entry ([`super::FaultPoint::BeforeEntry`],
//! [`super::FaultPoint::AfterEntry`]). Per-entry progress may lag disk by one
//! entry, so it is a hint, never the source of truth. Multi-entry steps: `LiveMovedToBackup`,
//! `CleanInstallBackupMade`, `StagedMovedToLive`, `PlayerFilesCarriedForward`,
//! `NewEntriesRemovedFromLive`, `BackupRestoredToLive`, `CleanedUp`. Every other
//! boundary is a single step and is only consulted at `BeforeStep`/`AfterStep`.
//!
//! # Direction and recovery rule
//!
//! The journal records a direction: forward ([`Boundary::sequence`]) or rollback
//! ([`Boundary::ROLLBACK`]). Startup recovery, and cancel or failure during a
//! transaction, decide as follows:
//!
//! * forward, `LiveVerified` reached: roll forward through the remaining forward
//!   boundaries (commit the record, clean up);
//! * forward, `LiveVerified` not reached: switch the journal to rollback and run
//!   [`Boundary::ROLLBACK`];
//! * rollback: resume [`Boundary::ROLLBACK`] after the last boundary reached.
//!
//! **Per-entry decisions come from disk.** Whenever recovery undoes, redoes or
//! finishes a multi-entry step (including the step that was in progress at the
//! crash, whose boundary the journal has not recorded), it examines *every* entry
//! the journal planned for that step, not only those recorded as done, and decides
//! each one with [`super::PlannedRename::on_disk`] / [`super::PlannedCopy::on_disk`]:
//! `Done` renames are reversed by rollback (or skipped by roll-forward), `NotDone`
//! entries are left (or performed), `Conflict` stops recovery with
//! [`super::ErrorCode::RecoveryRequired`] without further changes. A copy is `Done`
//! only when the destination has the source's exact bytes, because the destination
//! may have existed before the copy (a shipped default under the player's edited
//! file). Copies are never reversed singly; their destinations are inside `S` or
//! the new `L`, which rollback discards whole. Examples:
//!
//! * crash at `LiveMovedToBackup` `AfterEntry(k)`: entry `k` is in `B` but
//!   unjournaled; `BackupRestoredToLive` still finds it `Done` and moves it back;
//! * crash at `CleanInstallBackupMade` `AfterEntry(1)`: `L` is already `K`;
//!   the `L -> K` entry reads `Done`, so `BackupRestoredToLive` renames `K` back;
//! * crash at `StagedMovedToLive` `AfterEntry(k)`: `NewEntriesRemovedFromLive`
//!   finds entry `k` in `L` and returns it to `S`.
//!
//! Rollback steps plan their entries as the reverses of the forward step's planned
//! entries, so they are covered by the same rule and are idempotent: re-running a
//! rollback step after a crash finds finished entries `NotDone` (already reversed).
//! Their order matters: `NewEntriesRemovedFromLive` must finish before
//! `BackupRestoredToLive`, because a replaced file's backup entry reads `Conflict`
//! while the new file still occupies its live path.
//! Deletes in `CleanedUp` are decided the same way (target absent = done), and
//! recovery and `CleanedUp` delete any planned copy `temp` file left by a crash
//! between writing it and renaming it into place. A `temp` is always `to` plus the
//! reserved [`super::COPY_TEMP_SUFFIX`] and must be absent when planned
//! ([`super::PlannedCopy::plan`]), so that deletion never removes a player file. A
//! copy whose source is not a regular file (a link or dir) reads `Conflict`.
//!
//! Recovery is itself journaled, so a crash during recovery is resolved by the next
//! recovery. It never touches player or unknown files or dirs: it only acts on
//! entries the journal planned. The installed-release record is never changed
//! during rollback, because it is only written after `LiveVerified`.
//!
//! Every operation reports every boundary of its sequence in order, even when a
//! step has nothing to do (a fresh install moves zero entries into `B`), so the
//! failure matrix (#39) can inject a fault at each one.

use serde::{Deserialize, Serialize};

/// A journaled operation on one product installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    /// First installation of a release into an existing product root that has no
    /// managed content.
    Install,
    /// Recognise an existing installation that already matches a release inventory.
    /// Writes only the installed-release record.
    Adopt,
    /// Restore missing or damaged managed files from the full archive.
    Repair,
    /// Replace the installed release with another release.
    Update,
    /// Remove only positively identified managed files; player and unknown files stay.
    Uninstall,
    /// With explicit consent: keep the existing folder as a labelled backup, install
    /// cleanly, carry recognised player files forward.
    CleanInstall,
}

/// A durable point in a transaction. The journal records the last boundary
/// reached; [`super::FaultInjector`] is consulted around each one.
///
/// Wire/journal names are snake_case and must not change without bumping
/// [`super::CONTRACT_VERSION`]. Variants are not ordered; use positions in
/// [`Boundary::sequence`] or [`Boundary::ROLLBACK`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Boundary {
    // ---- forward ----
    /// Inspect/plan and preflight (space, permissions, running processes, lock,
    /// compatibility, save-breaking acknowledgement, mandatory-setting conflicts)
    /// passed, a [`super::WriteGuard`] was issued, and the journal now exists on disk
    /// with the plan, the guarded root and the sibling paths. Nothing outside the
    /// journal has been written.
    TransactionOpened,
    /// The complete archive is on disk outside the live root, flushed, and its size
    /// and SHA-256 match. A complete archive found later is reused only after
    /// re-verification.
    DownloadComplete,
    /// The archive was extracted with the pinned 7-Zip into isolated staging `S`,
    /// after every entry was validated. The live root is untouched.
    Staged,
    /// Staging matches the release [`super::ExpectedInventory`] exactly (no extra,
    /// missing or differing files; required empty dirs present). Preservation and
    /// compatibility checks are rechecked here, immediately before the first live
    /// mutation. Last point where cancel needs no live-root rollback.
    StagedInventoryVerified,
    /// Activation step 1: each live managed entry being replaced or removed has been
    /// renamed out of the live root into backup `B` (the recovery copy). Multi-entry.
    LiveMovedToBackup,
    /// Clean install only, replacing step 1. Entry 0 copies the sentinel from `L`
    /// into `S` (when `L` has one); entry 1 renames the whole of `L` to the labelled
    /// clean-install backup `K`. So the sentinel is always in `L` or `K`, and in `S`
    /// before `S` becomes `L`. `K` is never silently deleted. Multi-entry.
    CleanInstallBackupMade,
    /// Activation step 2: each new entry has been renamed from staging into the live
    /// root (for clean install: `S` itself is renamed to `L`). Multi-entry.
    StagedMovedToLive,
    /// Clean install only: recognised player files have been copied from `K` into
    /// the new live root (`K` keeps its copies). Multi-entry.
    PlayerFilesCarriedForward,
    /// The live root has been re-read and matches the release inventory, with
    /// preserved player, edited-config and unknown files still present (for
    /// uninstall: managed files absent, everything else intact). From here recovery
    /// rolls forward instead of back.
    LiveVerified,
    /// The installed-release record (State, #34) now names the new release, or is
    /// removed for uninstall. Only now is the result installed or playable.
    InstalledRecordCommitted,

    // ---- rollback (cancel during activation, failure, or restart recovery) ----
    /// The journal durably switched direction to rollback, recording why. Nothing
    /// else has been changed by the rollback yet.
    RollbackStarted,
    /// Each planned `S` -> `L` entry that disk shows done has been renamed back out of
    /// `L` into `S` (for clean install: the new `L` as a whole is renamed aside into
    /// `S`; any carried-forward player copies go with it, since `K` keeps the
    /// originals). Multi-entry; zero entries if activation never moved anything in.
    NewEntriesRemovedFromLive,
    /// Each planned `L` -> `B` entry that disk shows done has been renamed back into `L` (for
    /// clean install: `K` is renamed back to `L`). The previous complete
    /// installation is in place again. Multi-entry. Only correct after
    /// `NewEntriesRemovedFromLive` finished: until then a replaced file's entry reads
    /// `Conflict`.
    BackupRestoredToLive,
    /// The live root has been re-read and matches the previous installation's
    /// inventory (or, for a first install, holds no managed content), with player
    /// and unknown files intact. The installed-release record is unchanged.
    RollbackVerified,

    // ---- both directions ----
    /// Transaction scratch has been removed: staging `S`, backup `B`, the archive and
    /// any stray copy `temp` files the journal planned.
    /// After a committed uninstall it also removes managed dirs left empty; after a
    /// committed update, required empty dirs the new release no longer lists if they
    /// are still empty. It never removes `K`, a non-empty dir it did not create, or
    /// any player or unknown file or dir. Multi-entry. The journal is closed after this.
    CleanedUp,
}

impl Boundary {
    /// The ordered forward boundaries every run of `kind` reports.
    pub const fn sequence(kind: OperationKind) -> &'static [Boundary] {
        use Boundary::*;
        match kind {
            OperationKind::Install | OperationKind::Repair | OperationKind::Update => &[
                TransactionOpened,
                DownloadComplete,
                Staged,
                StagedInventoryVerified,
                LiveMovedToBackup,
                StagedMovedToLive,
                LiveVerified,
                InstalledRecordCommitted,
                CleanedUp,
            ],
            OperationKind::CleanInstall => &[
                TransactionOpened,
                DownloadComplete,
                Staged,
                StagedInventoryVerified,
                CleanInstallBackupMade,
                StagedMovedToLive,
                PlayerFilesCarriedForward,
                LiveVerified,
                InstalledRecordCommitted,
                CleanedUp,
            ],
            OperationKind::Uninstall => &[
                TransactionOpened,
                LiveMovedToBackup,
                LiveVerified,
                InstalledRecordCommitted,
                CleanedUp,
            ],
            OperationKind::Adopt => &[
                TransactionOpened,
                LiveVerified,
                InstalledRecordCommitted,
                CleanedUp,
            ],
        }
    }

    /// The ordered rollback boundaries, the same for every [`OperationKind`]. Entered
    /// from any forward boundary before `LiveVerified`.
    pub const ROLLBACK: &'static [Boundary] = &[
        Boundary::RollbackStarted,
        Boundary::NewEntriesRemovedFromLive,
        Boundary::BackupRestoredToLive,
        Boundary::RollbackVerified,
        Boundary::CleanedUp,
    ];

    /// Whether the step behind this boundary processes several entries and so
    /// consults [`super::FaultPoint::BeforeEntry`]/[`super::FaultPoint::AfterEntry`].
    pub const fn is_multi_entry(self) -> bool {
        use Boundary::*;
        matches!(
            self,
            LiveMovedToBackup
                | CleanInstallBackupMade
                | StagedMovedToLive
                | PlayerFilesCarriedForward
                | NewEntriesRemovedFromLive
                | BackupRestoredToLive
                | CleanedUp
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_KINDS: [OperationKind; 6] = [
        OperationKind::Install,
        OperationKind::Adopt,
        OperationKind::Repair,
        OperationKind::Update,
        OperationKind::Uninstall,
        OperationKind::CleanInstall,
    ];

    fn pos(seq: &[Boundary], b: Boundary) -> usize {
        seq.iter().position(|x| *x == b).unwrap()
    }

    #[test]
    fn every_sequence_opens_first_commits_after_verification_and_cleans_up_last() {
        for kind in ALL_KINDS {
            let seq = Boundary::sequence(kind);
            assert_eq!(seq.first(), Some(&Boundary::TransactionOpened), "{kind:?}");
            assert_eq!(seq.last(), Some(&Boundary::CleanedUp), "{kind:?}");
            assert_eq!(
                pos(seq, Boundary::InstalledRecordCommitted),
                pos(seq, Boundary::LiveVerified) + 1
            );
            let unique: std::collections::HashSet<_> = seq.iter().collect();
            assert_eq!(unique.len(), seq.len(), "{kind:?}");
            assert!(!seq.iter().any(|b| Boundary::ROLLBACK[..4].contains(b)));
        }
    }

    #[test]
    fn archive_operations_verify_staging_before_touching_the_live_root() {
        for kind in [
            OperationKind::Install,
            OperationKind::Repair,
            OperationKind::Update,
            OperationKind::CleanInstall,
        ] {
            let seq = Boundary::sequence(kind);
            let first_live = seq
                .iter()
                .position(|b| {
                    matches!(
                        b,
                        Boundary::LiveMovedToBackup | Boundary::CleanInstallBackupMade
                    )
                })
                .unwrap();
            assert_eq!(
                first_live,
                pos(seq, Boundary::StagedInventoryVerified) + 1,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn rollback_restores_before_verifying_and_never_commits() {
        let rb = Boundary::ROLLBACK;
        assert_eq!(rb.first(), Some(&Boundary::RollbackStarted));
        assert_eq!(rb.last(), Some(&Boundary::CleanedUp));
        assert!(
            pos(rb, Boundary::NewEntriesRemovedFromLive) < pos(rb, Boundary::BackupRestoredToLive)
        );
        assert!(!rb.contains(&Boundary::InstalledRecordCommitted));
    }

    #[test]
    fn every_live_moving_step_is_multi_entry() {
        for b in [
            Boundary::LiveMovedToBackup,
            Boundary::StagedMovedToLive,
            Boundary::NewEntriesRemovedFromLive,
            Boundary::BackupRestoredToLive,
        ] {
            assert!(b.is_multi_entry(), "{b:?}");
        }
        assert!(!Boundary::LiveVerified.is_multi_entry());
    }

    #[test]
    fn boundary_journal_names_are_snake_case() {
        assert_eq!(
            serde_json::to_string(&Boundary::LiveMovedToBackup).unwrap(),
            "\"live_moved_to_backup\""
        );
        let b: Boundary = serde_json::from_str("\"backup_restored_to_live\"").unwrap();
        assert_eq!(b, Boundary::BackupRestoredToLive);
    }
}
