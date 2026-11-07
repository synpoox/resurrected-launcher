//! Controlled failure injection at transaction boundaries. Production wires
//! [`NoFaults`]; tests (#36, #39) inject [`FailAt`] or their own injector.

use std::fmt;

use super::Boundary;

/// Where around a [`Boundary`] the transaction consults the injector. Together the
/// two points cover the crash windows of write-ahead journaling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FaultPoint {
    /// The journal durably records the intent to perform this boundary's step; no part
    /// of the step has been done yet.
    BeforeStep,
    /// The step's filesystem effect is complete and flushed, but the journal does not
    /// yet record the boundary as reached (journal lags disk).
    AfterStep,
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

/// Consulted by the transaction at both [`FaultPoint`]s of every boundary in
/// [`Boundary::sequence`].
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

/// Test injector: fails once the transaction reaches exactly this boundary and point.
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
    fn no_faults_never_fails_and_fail_at_fails_only_its_point() {
        let fail = FailAt {
            boundary: Boundary::StagedMovedToLive,
            point: FaultPoint::AfterStep,
        };
        for &b in Boundary::sequence(crate::contract::OperationKind::Update) {
            for p in [FaultPoint::BeforeStep, FaultPoint::AfterStep] {
                assert!(NoFaults.check(b, p).is_ok());
                assert_eq!(
                    fail.check(b, p).is_err(),
                    b == fail.boundary && p == fail.point
                );
            }
        }
    }
}
