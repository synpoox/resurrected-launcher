//! Frozen shared interfaces (#32). Every later ticket codes against these types.
//!
//! Change policy: additive changes (a new DTO, a new [`ErrorCode`]) go through a
//! reviewed PR. Renaming, removing or re-meaning anything that crosses the IPC
//! boundary or is persisted (wire names, [`Boundary`] names) bumps
//! [`CONTRACT_VERSION`].

mod boundary;
mod dto;
mod error;
mod fault;
mod guard;
mod inventory;

pub use boundary::{Boundary, OperationKind};
pub use dto::{
    CancelRequest, OperationOutcome, OperationRequest, Phase, PingResponse, ProductId,
    ProgressEvent, Versioned,
};
pub use error::{ErrorCode, LauncherError};
pub use fault::{FailAt, FaultInjector, FaultPoint, InjectedFault, NoFaults};
pub use guard::{WriteGuard, DISPOSABLE_SENTINEL};
pub use inventory::{ExpectedFile, ExpectedInventory, RelativePath, Sha256Digest};

/// Version of the command/event/error contract carried by every [`Versioned`] payload.
pub const CONTRACT_VERSION: u32 = 1;

/// The provisional umbrella display name. This is the only place it lives; swap it
/// here (see GLOSSARY.md, web ADR 0004). Never "PD2Resurrect".
pub const UMBRELLA_DISPLAY_NAME: &str = "Project Resurrected";
