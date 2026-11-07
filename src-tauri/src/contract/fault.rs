//! Controlled failure injection at transaction boundaries. Production wires
//! [`NoFaults`]; tests (#36, #39) inject [`FailAt`] or their own injector.

use std::fmt;

use super::Boundary;

/// Where around a [`Boundary`]'s step the transaction consults the injector.
/// Together the points cover every crash window of write-ahead journaling,
/// including a crash after `k` of `n` entries of a multi-entry step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FaultPoint {
    /// The journal durably records the intent to perform this boundary's step; no part
    /// of the step has been done yet.
    BeforeStep,
    /// Multi-entry steps only ([`Boundary::is_multi_entry`]): the journal records
    /// entries `0..i` as done; entry `i` (0-based) has not been started.
    BeforeEntry(usize),
    /// Multi-entry steps only: entry `i` has been done and flushed, but the journal
    /// does not yet record it (journal lags disk by one entry; recovery decides entries from disk, see [`super::PlannedRename::on_disk`]).
    AfterEntry(usize),
    /// The step's filesystem effect is complete and flushed, but the journal does not
    /// yet record the boundary as reached (journal lags disk).
    AfterStep,
}

impl FaultPoint {
    /// Every point the transaction consults for `boundary` when its step processes
    /// `entries` entries, in order. #39 iterates this to cover every crash window.
    pub fn all(boundary: Boundary, entries: usize) -> Vec<FaultPoint> {
        let mut points = vec![FaultPoint::BeforeStep];
        if boundary.is_multi_entry() {
            for i in 0..entries {
                points.extend([FaultPoint::BeforeEntry(i), FaultPoint::AfterEntry(i)]);
            }
        }
        points.push(FaultPoint::AfterStep);
        points
    }
}

/// A simulated crash. The transaction must return it immediately without cleanup or
/// rollback, as if the process died; the test then runs startup recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InjectedFault {
    pub boundary: Boundary,
    pub point: FaultPoint,
}

impl fmt::Display for InjectedFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "injected fault {:?} {:?}", self.point, self.boundary)
    }
}

impl std::error::Error for InjectedFault {}

/// Consulted by the transaction at every [`FaultPoint::all`] point of every boundary
/// it crosses, forward ([`Boundary::sequence`]) and rollback ([`Boundary::ROLLBACK`]),
/// including during startup recovery.
pub trait FaultInjector: Send + Sync {
    fn check(&self, boundary: Boundary, point: FaultPoint) -> Result<(), InjectedFault>;
}

/// Production injector: never fails.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoFaults;

impl FaultInjector for NoFaults {
    fn check(&self, _: Boundary, _: FaultPoint) -> Result<(), InjectedFault> {
        Ok(())
    }
}

/// Test injector: fails each time the transaction reaches exactly this boundary
/// and point. Stateless, so a recovery run with the same injector fails at the same
/// point again; pass [`NoFaults`] to the recovery run unless testing that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FailAt {
    pub boundary: Boundary,
    pub point: FaultPoint,
}

impl FaultInjector for FailAt {
    fn check(&self, boundary: Boundary, point: FaultPoint) -> Result<(), InjectedFault> {
        if (boundary, point) == (self.boundary, self.point) {
            Err(InjectedFault { boundary, point })
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fault_points_cover_each_entry_of_multi_entry_steps() {
        use FaultPoint::*;
        assert_eq!(
            FaultPoint::all(Boundary::StagedMovedToLive, 2),
            [
                BeforeStep,
                BeforeEntry(0),
                AfterEntry(0),
                BeforeEntry(1),
                AfterEntry(1),
                AfterStep
            ]
        );
        assert_eq!(
            FaultPoint::all(Boundary::LiveVerified, 5),
            [BeforeStep, AfterStep]
        );
        assert_eq!(
            FaultPoint::all(Boundary::BackupRestoredToLive, 0),
            [BeforeStep, AfterStep]
        );
    }

    #[test]
    fn no_faults_never_fails_and_fail_at_fails_only_its_point() {
        let fail = FailAt {
            boundary: Boundary::LiveMovedToBackup,
            point: FaultPoint::AfterEntry(1),
        };
        let boundaries = Boundary::sequence(crate::contract::OperationKind::Update)
            .iter()
            .chain(Boundary::ROLLBACK);
        for &b in boundaries {
            for p in FaultPoint::all(b, 3) {
                assert!(NoFaults.check(b, p).is_ok());
                assert_eq!(
                    fail.check(b, p),
                    if (b, p) == (fail.boundary, fail.point) {
                        Err(InjectedFault {
                            boundary: b,
                            point: p,
                        })
                    } else {
                        Ok(())
                    }
                );
            }
        }
    }
}
