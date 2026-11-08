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
mod game;
mod guard;
mod inventory;
mod recovery;

pub use boundary::{Boundary, OperationKind};
pub use dto::{
    CancelRequest, OperationOutcome, OperationRequest, OutcomeReport, Phase, PingResponse,
    ProductId, ProgressEvent, TrayAction, TrayRequest, Versioned,
};
pub use error::{ErrorCode, LauncherError};
pub use fault::{FailAt, FaultInjector, FaultPoint, InjectedFault, NoFaults};
pub use game::{
    GameLocation, GameStore, LoaderGuidance, Readiness, RunningGame, LOADER_DOWNLOAD_PAGE,
};
pub use guard::{WriteGuard, DISPOSABLE_SENTINEL};
pub use inventory::{
    ExpectedFile, ExpectedInventory, InventoryError, ListingMismatch, PathError, RelativePath,
    Sha256Digest,
};
pub use recovery::{EntryOnDisk, PlannedCopy, PlannedRename, COPY_TEMP_SUFFIX};

/// Version of the command/event/error contract carried by every [`Versioned`] payload.
/// 2: the succeeded outcome carries a report, progress counts files (#74).
pub const CONTRACT_VERSION: u32 = 2;

/// The provisional umbrella display name. This is the only place it lives; swap it
/// here (see GLOSSARY.md, web ADR 0004). Never "PD2Resurrect".
pub const UMBRELLA_DISPLAY_NAME: &str = "Project Resurrected";
