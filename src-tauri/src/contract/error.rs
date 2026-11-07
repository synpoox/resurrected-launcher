//! The one error type that crosses module and IPC boundaries.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Stable, machine-readable error codes. The frontend picks copy from the code;
/// `message` is detail for diagnostics, not UI copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Malformed or contradictory request from the frontend.
    InvalidRequest,
    /// Request carried a different [`super::CONTRACT_VERSION`].
    UnsupportedContractVersion,
    /// The product ID is not in the catalog.
    UnknownProduct,
    /// Feed or manifest is missing, malformed or uses placeholder verification data.
    MetadataInvalid,
    /// Metadata needs a newer launcher (unsupported schema or minimum launcher version).
    LauncherUpdateRequired,
    /// The same release ID was seen with different hashes (immutability violation).
    ReleaseChanged,
    /// [`super::WriteGuard`] refused: the product root lacks the disposable sentinel.
    ProductRootNotDisposable,
    /// Another operation already holds this installation's lock.
    OperationInProgress,
    /// A game or product process is running against this installation.
    GameRunning,
    /// Not enough free space for download, staging and recovery copy.
    InsufficientSpace,
    /// The normal user cannot write where the operation needs to. Never elevate.
    AccessDenied,
    /// Network/HTTP failure, or a non-HTTPS URL or redirect.
    DownloadFailed,
    /// Downloaded bytes do not match the expected size or SHA-256.
    VerificationFailed,
    /// An archive entry is unsafe (traversal, absolute, link, reserved name, ...).
    UnsafeArchiveEntry,
    /// Files on disk do not match the [`super::ExpectedInventory`].
    InventoryMismatch,
    /// A save-breaking release was requested without the acknowledgement (ADR 0001).
    SaveBreakingNotAcknowledged,
    /// Clean install was requested without explicit consent.
    CleanInstallConsentRequired,
    /// An edited mandatory setting conflicts with the new release; nothing was changed.
    MandatorySettingConflict,
    /// The operation was cancelled by the user.
    Cancelled,
    /// An interrupted transaction must be resolved before Play or another operation.
    RecoveryRequired,
    /// A bug or unexpected I/O error; see `message`.
    Internal,
}

/// Error with a stable [`ErrorCode`] and a diagnostic message.
/// `message` may contain local paths and user names; State (#34) redacts it before
/// it reaches diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LauncherError {
    pub code: ErrorCode,
    pub message: String,
}

impl LauncherError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for LauncherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for LauncherError {}
