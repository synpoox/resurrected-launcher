//! Cancellation at every mutation boundary, then restart recovery. The player's
//! cancel arrives just before and just after each boundary's step. Activation is
//! never interrupted mid-step: a cancel before `LiveVerified` was journaled rolls
//! the transaction back to the previous installation in process and returns
//! `Cancelled`; a later one lets the verified new installation commit. Either way
//! restart recovery then finds nothing to do.

use std::sync::atomic::Ordering;

use super::*;
use resurrected_launcher_lib::contract::ErrorCode;

fn cancel_then_recover(kind: OperationKind, b: Boundary) {
    for p in [FaultPoint::BeforeStep, FaultPoint::AfterStep] {
        let case = format!("{kind:?} cancel at {b:?} {p:?}");
        let s = Scenario::new(kind);
        let cancel = Hook(|x, q| {
            if (x, q) == (b, p) {
                s.rig.cancel.store(true, Ordering::Relaxed);
            }
        });
        let result = s.run(&cancel);
        assert!(
            s.rig.cancel.load(Ordering::Relaxed),
            "{case}: point reached"
        );
        let new = past_live_verified(kind, b)
            || (b == Boundary::LiveVerified && p == FaultPoint::AfterStep);
        if new {
            result.unwrap_or_else(|e| panic!("{case}: {e}"));
        } else {
            assert_eq!(result.unwrap_err().code, ErrorCode::Cancelled, "{case}");
        }
        assert_eq!(
            s.rig.recover(&NoFaults).unwrap(),
            Recovery::Nothing,
            "{case}"
        );
        s.assert_settled(new, &case);
    }
}

mod install {
    use super::*;
    boundary_tests!(cancel_then_recover, Install, Boundary::sequence(Install);
        transaction_opened => TransactionOpened,
        download_complete => DownloadComplete,
        staged => Staged,
        staged_inventory_verified => StagedInventoryVerified,
        live_moved_to_backup => LiveMovedToBackup,
        staged_moved_to_live => StagedMovedToLive,
        live_verified => LiveVerified,
        installed_record_committed => InstalledRecordCommitted,
        cleaned_up => CleanedUp,
    );
}

mod update {
    use super::*;
    boundary_tests!(cancel_then_recover, Update, Boundary::sequence(Update);
        transaction_opened => TransactionOpened,
        download_complete => DownloadComplete,
        staged => Staged,
        staged_inventory_verified => StagedInventoryVerified,
        live_moved_to_backup => LiveMovedToBackup,
        staged_moved_to_live => StagedMovedToLive,
        live_verified => LiveVerified,
        installed_record_committed => InstalledRecordCommitted,
        cleaned_up => CleanedUp,
    );
}

mod repair {
    use super::*;
    boundary_tests!(cancel_then_recover, Repair, Boundary::sequence(Repair);
        transaction_opened => TransactionOpened,
        download_complete => DownloadComplete,
        staged => Staged,
        staged_inventory_verified => StagedInventoryVerified,
        live_moved_to_backup => LiveMovedToBackup,
        staged_moved_to_live => StagedMovedToLive,
        live_verified => LiveVerified,
        installed_record_committed => InstalledRecordCommitted,
        cleaned_up => CleanedUp,
    );
}

mod uninstall {
    use super::*;
    boundary_tests!(cancel_then_recover, Uninstall, Boundary::sequence(Uninstall);
        transaction_opened => TransactionOpened,
        live_moved_to_backup => LiveMovedToBackup,
        live_verified => LiveVerified,
        installed_record_committed => InstalledRecordCommitted,
        cleaned_up => CleanedUp,
    );
}

mod clean_install {
    use super::*;
    boundary_tests!(cancel_then_recover, CleanInstall, Boundary::sequence(CleanInstall);
        transaction_opened => TransactionOpened,
        download_complete => DownloadComplete,
        staged => Staged,
        staged_inventory_verified => StagedInventoryVerified,
        clean_install_backup_made => CleanInstallBackupMade,
        staged_moved_to_live => StagedMovedToLive,
        player_files_carried_forward => PlayerFilesCarriedForward,
        live_verified => LiveVerified,
        installed_record_committed => InstalledRecordCommitted,
        cleaned_up => CleanedUp,
    );
}
