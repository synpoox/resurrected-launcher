//! A crash inside restart recovery itself, then recovery again. Rollback tests
//! crash the operation partway through activation (the first entry moved into the
//! live root, or for uninstall out of it), then crash recovery at every fault point
//! of one [`Boundary::ROLLBACK`] boundary; the next recovery must still restore the
//! previous installation. Roll-forward tests crash just before the record commit,
//! then crash recovery at every point of a remaining forward boundary; the next
//! recovery must still commit the new installation.

use super::*;
use resurrected_launcher_lib::contract::FailAt;

/// Where the operation crashes before recovery: mid-activation, or past
/// `LiveVerified`.
fn first_crash(kind: OperationKind, roll_forward: bool) -> FailAt {
    let (boundary, point) = match (roll_forward, kind) {
        (true, _) => (Boundary::InstalledRecordCommitted, FaultPoint::BeforeStep),
        (false, Uninstall) => (Boundary::LiveMovedToBackup, FaultPoint::AfterEntry(0)),
        (false, _) => (Boundary::StagedMovedToLive, FaultPoint::AfterEntry(0)),
    };
    FailAt { boundary, point }
}

fn crashed(kind: OperationKind, roll_forward: bool) -> Scenario {
    let s = Scenario::new(kind);
    s.run(&first_crash(kind, roll_forward)).unwrap_err();
    s
}

fn crash_recovery_then_recover(kind: OperationKind, b: Boundary, roll_forward: bool) {
    let reference = crashed(kind, roll_forward);
    let seen = Recorder::default();
    reference.rig.recover(&seen).unwrap();
    reference.assert_settled(roll_forward, &format!("{kind:?} recovery without faults"));
    let points = seen.points_of(b);
    assert!(!points.is_empty(), "{kind:?}: recovery never reached {b:?}");

    for p in points {
        let case = format!("{kind:?} recovery crash at {b:?} {p:?}");
        let s = crashed(kind, roll_forward);
        let err = s
            .rig
            .recover(&FailAt {
                boundary: b,
                point: p,
            })
            .unwrap_err();
        assert!(err.message.contains("injected fault"), "{case}: {err}");
        let want = if roll_forward {
            Recovery::RolledForward
        } else {
            Recovery::RolledBack
        };
        let again = s
            .rig
            .recover(&NoFaults)
            .unwrap_or_else(|e| panic!("{case}: {e}"));
        assert_eq!(again, want, "{case}");
        s.assert_settled(roll_forward, &case);
    }
}

fn rolling_back(kind: OperationKind, b: Boundary) {
    crash_recovery_then_recover(kind, b, false);
}

fn rolling_forward(kind: OperationKind, b: Boundary) {
    crash_recovery_then_recover(kind, b, true);
}

macro_rules! recovery_tests {
    ($kind:ident) => {
        mod rolling_back {
            use super::*;
            boundary_tests!(rolling_back, $kind, Boundary::ROLLBACK;
                rollback_started => RollbackStarted,
                new_entries_removed_from_live => NewEntriesRemovedFromLive,
                backup_restored_to_live => BackupRestoredToLive,
                rollback_verified => RollbackVerified,
                cleaned_up => CleanedUp,
            );
        }

        mod rolling_forward {
            use super::*;
            boundary_tests!(rolling_forward, $kind, &Boundary::sequence($kind)[Boundary::sequence($kind).len() - 2..];
                installed_record_committed => InstalledRecordCommitted,
                cleaned_up => CleanedUp,
            );
        }
    };
}

mod install {
    use super::*;
    recovery_tests!(Install);
}

mod update {
    use super::*;
    recovery_tests!(Update);
}

mod repair {
    use super::*;
    recovery_tests!(Repair);
}

mod uninstall {
    use super::*;
    recovery_tests!(Uninstall);
}

mod clean_install {
    use super::*;
    recovery_tests!(CleanInstall);
}
