//! Crash at every mutation boundary, then restart recovery. Each test crashes the
//! operation at every [`FaultPoint`] its boundary consults (before the step,
//! around each entry of a multi-entry step, after the step), each time on a fresh
//! disposable installation, then runs startup recovery. The root must then hold
//! the previous installation (crash before `LiveVerified` was journaled) or the
//! new one (after), verified on disk.

use super::*;
use resurrected_launcher_lib::contract::{ErrorCode, FailAt};

fn crash_then_recover(kind: OperationKind, b: Boundary) {
    for p in forward_points(kind, b) {
        let case = format!("{kind:?} crash at {b:?} {p:?}");
        let s = Scenario::new(kind);
        let err = s
            .run(&FailAt {
                boundary: b,
                point: p,
            })
            .unwrap_err();
        assert!(err.message.contains("injected fault"), "{case}: {err}");
        // Mid clean install the root may be gone (it is `K`), so the guard
        // refuses before the journal is seen; either way nothing is changed.
        let refused = s.run(&NoFaults).unwrap_err().code;
        let root_gone = !s.root.exists();
        assert!(
            refused == ErrorCode::RecoveryRequired
                || (root_gone && refused == ErrorCode::ProductRootNotDisposable),
            "{case}: another operation before recovery: {refused:?}"
        );

        let recovered = s
            .rig
            .recover(&NoFaults)
            .unwrap_or_else(|e| panic!("{case}: {e}"));
        let new = past_live_verified(kind, b);
        let want = if new {
            Recovery::RolledForward
        } else {
            Recovery::RolledBack
        };
        assert_eq!(recovered, want, "{case}");
        s.assert_settled(new, &case);
        assert_eq!(
            s.rig.recover(&NoFaults).unwrap(),
            Recovery::Nothing,
            "{case}"
        );
    }
}

mod install {
    use super::*;
    boundary_tests!(crash_then_recover, Install, Boundary::sequence(Install);
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
    boundary_tests!(crash_then_recover, Update, Boundary::sequence(Update);
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
    boundary_tests!(crash_then_recover, Repair, Boundary::sequence(Repair);
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
    boundary_tests!(crash_then_recover, Uninstall, Boundary::sequence(Uninstall);
        transaction_opened => TransactionOpened,
        live_moved_to_backup => LiveMovedToBackup,
        live_verified => LiveVerified,
        installed_record_committed => InstalledRecordCommitted,
        cleaned_up => CleanedUp,
    );
}

mod clean_install {
    use super::*;
    boundary_tests!(crash_then_recover, CleanInstall, Boundary::sequence(CleanInstall);
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
