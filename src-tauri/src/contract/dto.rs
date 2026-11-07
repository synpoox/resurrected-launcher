//! Command and event DTOs crossing the Tauri IPC boundary.
//!
//! The frontend sends user intents (product ID + operation kind + explicit consents),
//! never paths, URLs, tools or arguments. Wire names are snake_case.

use serde::{Deserialize, Serialize};

use super::{Boundary, ErrorCode, LauncherError, OperationKind, CONTRACT_VERSION};

/// Envelope carrying [`CONTRACT_VERSION`] with every request, response and event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Versioned<T> {
    pub contract_version: u32,
    pub data: T,
}

impl<T> Versioned<T> {
    /// Wraps `data` in the current contract version.
    pub fn new(data: T) -> Self {
        Self {
            contract_version: CONTRACT_VERSION,
            data,
        }
    }

    /// Unwraps an incoming payload, refusing any other contract version.
    pub fn into_current(self) -> Result<T, LauncherError> {
        if self.contract_version == CONTRACT_VERSION {
            Ok(self.data)
        } else {
            Err(LauncherError::new(
                ErrorCode::UnsupportedContractVersion,
                format!(
                    "contract version {} is not supported (expected {CONTRACT_VERSION})",
                    self.contract_version
                ),
            ))
        }
    }
}

/// Stable machine identity of a product, distinct from its display name.
/// Opaque here; the Catalog validates that it names a known product.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProductId(pub String);

/// Reply to the `ping` command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PingResponse {
    pub app_version: String,
    pub display_name: String,
}

/// A user intent to run a journaled operation on one product.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationRequest {
    pub product_id: ProductId,
    pub kind: OperationKind,
    /// The player acknowledged the save-incompatibility warning (ADR 0001).
    /// Install and update of a save-breaking release are refused without it.
    #[serde(default)]
    pub acknowledge_save_breaking: bool,
    /// Explicit consent for [`OperationKind::CleanInstall`]; refused without it.
    #[serde(default)]
    pub consent_clean_install: bool,
}

/// A user intent to cancel the running operation on one product. During final
/// activation this finishes or rolls back safely before stopping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelRequest {
    pub product_id: ProductId,
}

/// Coarse, user-facing progress phase of an operation (the UI's view of the
/// pipeline; [`Boundary`] is the finer durable view).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Planning,
    Downloading,
    Extracting,
    Verifying,
    Activating,
    Committing,
    CleaningUp,
    /// Resolving an interrupted transaction found at startup.
    Recovering,
}

/// Progress event emitted while an operation runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub product_id: ProductId,
    pub kind: OperationKind,
    pub phase: Phase,
    /// Last boundary durably reached, if any.
    pub boundary: Option<Boundary>,
    pub bytes_done: Option<u64>,
    pub bytes_total: Option<u64>,
}

/// Final result event of an operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum OperationOutcome {
    /// Committed: the installed-release record reflects the new state.
    Succeeded {
        product_id: ProductId,
        kind: OperationKind,
    },
    /// Cancelled; the previous complete installation is in place.
    Cancelled {
        product_id: ProductId,
        kind: OperationKind,
    },
    /// Failed; `error` explains why. The previous installation was restored unless
    /// `error.code` is [`ErrorCode::RecoveryRequired`].
    Failed {
        product_id: ProductId,
        kind: OperationKind,
        error: LauncherError,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn operation_request_wire_shape_is_snake_case_with_default_consents() {
        let wire = json!({
            "contract_version": CONTRACT_VERSION,
            "data": { "product_id": "pd2r", "kind": "clean_install" }
        });
        let req: Versioned<OperationRequest> = serde_json::from_value(wire).unwrap();
        let req = req.into_current().unwrap();
        assert_eq!(req.product_id, ProductId("pd2r".into()));
        assert_eq!(req.kind, OperationKind::CleanInstall);
        assert!(!req.acknowledge_save_breaking);
        assert!(!req.consent_clean_install);
    }

    #[test]
    fn other_contract_version_is_refused() {
        let old = Versioned {
            contract_version: CONTRACT_VERSION + 1,
            data: (),
        };
        let err = old.into_current().unwrap_err();
        assert_eq!(err.code, ErrorCode::UnsupportedContractVersion);
    }

    #[test]
    fn failed_outcome_serializes_with_status_tag() {
        let outcome = OperationOutcome::Failed {
            product_id: ProductId("pd2r".into()),
            kind: OperationKind::Update,
            error: LauncherError::new(ErrorCode::InsufficientSpace, "need 2 GB"),
        };
        let v = serde_json::to_value(Versioned::new(outcome)).unwrap();
        assert_eq!(v["data"]["status"], "failed");
        assert_eq!(v["data"]["error"]["code"], "insufficient_space");
    }
}
