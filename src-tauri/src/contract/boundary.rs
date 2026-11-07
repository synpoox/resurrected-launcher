//! Mutation boundaries of the journaled update transaction (#8 pipeline:
//! inspect/plan -> download/verify -> confined staging -> preserved-file and
//! compatibility checks -> journaled activation -> verify/commit -> safe cleanup).
//!
//! # Activation model
//!
//! Live root `L` is the product folder. Staging `S`, backup `B` and the downloaded
//! archive live outside `L` on the same filesystem. Activation moves entries by
//! rename, never by copying over live files:
//!
//! 1. every live managed entry the operation replaces or removes is renamed from `L`
//!    into `B` ([`Boundary::LiveMovedToBackup`]); `B` is the recovery copy of the
//!    previous installation, so the old bytes are kept without needing re-download;
//! 2. every new entry is renamed from `S` into `L` ([`Boundary::StagedMovedToLive`]).
//!
//! Player files, edited configs and unknown/plugin files are not in either set: they
//! stay in place and are never moved, renamed or deleted (clean install excepted,
//! where the whole old folder becomes a labelled backup that is never deleted).
//! A step that renames many entries is several Windows renames, not one atomic act;
//! the journal records per-entry progress inside a step so rollback is exact.
//!
//! # Recovery rule
//!
//! Before `LiveMovedToBackup` (or `CleanInstallBackupMade`) is reached, the live
//! root is untouched: recovery discards staging/archive. From then until
//! `LiveVerified`, recovery restores the previous installation from `B`. From
//! `LiveVerified` onward, recovery rolls forward (commit the record, clean up).
//!
//! Every operation reports every boundary of [`Boundary::sequence`] in order, even
//! when a step has nothing to do (a fresh install moves zero entries into `B`), so
//! the failure matrix (#39) can inject a fault at each one.

use serde::{Deserialize, Serialize};

/// A journaled operation on one product installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    /// First installation of a release into an empty (or absent-content) product root.
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
/// [`super::CONTRACT_VERSION`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Boundary {
    /// Inspect/plan and preflight (space, permissions, running processes, lock,
    /// compatibility, save-breaking acknowledgement, mandatory-setting conflicts)
    /// passed, and the journal now exists on disk with the plan. Nothing outside the
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
    /// mutation. Last point where cancel is free.
    StagedInventoryVerified,
    /// Activation step 1: each live managed entry being replaced or removed has been
    /// renamed out of the live root into backup `B` (the recovery copy). From here
    /// recovery must restore from `B` until [`Boundary::LiveVerified`].
    LiveMovedToBackup,
    /// Clean install only, replacing step 1: the whole existing product folder has
    /// been renamed to a labelled clean-install backup that is never silently deleted.
    CleanInstallBackupMade,
    /// Activation step 2: each new entry has been renamed from staging into the live
    /// root.
    StagedMovedToLive,
    /// Clean install only: recognised player files have been copied from the
    /// clean-install backup into the new live root (the backup keeps its copies).
    PlayerFilesCarriedForward,
    /// The live root has been re-read and matches the release inventory, with
    /// preserved player, edited-config and unknown files still present (for
    /// uninstall: managed files absent, everything else intact). From here recovery
    /// rolls forward instead of back.
    LiveVerified,
    /// The installed-release record (State, #34) now names the new release, or is
    /// removed for uninstall. Only now is the result installed or playable.
    InstalledRecordCommitted,
    /// Staging, backup `B` and the archive were removed. Never removes a clean-install
    /// backup or the sole copy of any player or unknown file. The journal is closed
    /// after this.
    CleanedUp,
}

impl Boundary {
    /// The ordered boundaries every run of `kind` reports.
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

    #[test]
    fn every_sequence_opens_first_commits_after_verification_and_cleans_up_last() {
        for kind in ALL_KINDS {
            let seq = Boundary::sequence(kind);
            assert_eq!(seq.first(), Some(&Boundary::TransactionOpened), "{kind:?}");
            assert_eq!(seq.last(), Some(&Boundary::CleanedUp), "{kind:?}");
            let pos = |b| seq.iter().position(|x| *x == b).unwrap();
            assert!(pos(Boundary::LiveVerified) < pos(Boundary::InstalledRecordCommitted));
            // Declaration order is pipeline order.
            assert!(seq.windows(2).all(|w| w[0] < w[1]), "{kind:?}");
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
            let verified = seq
                .iter()
                .position(|b| *b == Boundary::StagedInventoryVerified)
                .unwrap();
            let first_live = seq
                .iter()
                .position(|b| {
                    matches!(
                        b,
                        Boundary::LiveMovedToBackup | Boundary::CleanInstallBackupMade
                    )
                })
                .unwrap();
            assert_eq!(first_live, verified + 1, "{kind:?}");
        }
    }

    #[test]
    fn boundary_journal_names_are_snake_case() {
        assert_eq!(
            serde_json::to_string(&Boundary::LiveMovedToBackup).unwrap(),
            "\"live_moved_to_backup\""
        );
        let b: Boundary = serde_json::from_str("\"installed_record_committed\"").unwrap();
        assert_eq!(b, Boundary::InstalledRecordCommitted);
    }
}
