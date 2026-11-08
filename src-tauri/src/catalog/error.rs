//! Typed rejections of release metadata. Each rejection class has its own variant so
//! callers and tests can tell them apart; [`LauncherError`] carries them over IPC.

use std::fmt;

use crate::contract::{ErrorCode, InventoryError, LauncherError, PathError, ProductId};

use super::SCHEMA_VERSION;

/// Why a feed or release manifest was rejected. Every variant fails closed: the
/// metadata is not used and nothing is installed or marked ready from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogError {
    /// Not JSON, wrong types, a missing non-verification field or an unknown field.
    Malformed(String),
    /// `schema_version` is not [`SCHEMA_VERSION`].
    UnsupportedSchema { found: u64 },
    /// `min_launcher_version` is newer than this launcher.
    LauncherTooOld { required: String },
    /// A key that looks like a command, script, hook or argument list. Metadata is
    /// declarative only.
    ExecutableMetadata { key: String },
    /// A size or SHA-256 is missing, null, empty, zero or the all-zero placeholder.
    PlaceholderVerification { field: String },
    /// A URL is not `https://` with a plain host.
    NotHttps { field: String },
    /// A path is absolute, escapes its root or is unsafe on Windows.
    UnsafePath { field: String, error: PathError },
    /// The file list has duplicate or Windows-conflicting paths.
    Inventory(InventoryError),
    /// The archive's declared contents disagree with the file list.
    ArchiveInconsistent { field: String },
    /// An upcoming product carries a release pointer.
    UpcomingHasArtifacts { product_id: ProductId },
    /// The manifest is not the release the feed points to.
    ManifestMismatch { field: &'static str },
    /// A known release ID now has different metadata (immutability violation).
    ReleaseChanged { release_id: String },
    /// Any other field that is present but unusable.
    InvalidField { field: String, reason: &'static str },
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(m) => write!(f, "malformed release metadata: {m}"),
            Self::UnsupportedSchema { found } if *found > u64::from(SCHEMA_VERSION) => write!(
                f,
                "release metadata schema {found} is newer than this launcher supports \
                 ({SCHEMA_VERSION}); a launcher update is required"
            ),
            Self::UnsupportedSchema { found } => write!(
                f,
                "release metadata schema {found} is no longer supported (expected {SCHEMA_VERSION})"
            ),
            Self::LauncherTooOld { required } => write!(
                f,
                "this release needs launcher {required} or newer; a launcher update is required"
            ),
            Self::ExecutableMetadata { key } => {
                write!(f, "metadata must be declarative; `{key}` is not allowed")
            }
            Self::PlaceholderVerification { field } => {
                write!(f, "{field} is missing or a placeholder")
            }
            Self::NotHttps { field } => write!(f, "{field} must be an https:// URL"),
            Self::UnsafePath { field, error } => write!(f, "{field} is an unsafe path: {error}"),
            Self::Inventory(e) => write!(f, "file list: {e}"),
            Self::ArchiveInconsistent { field } => {
                write!(f, "{field} does not match the file list")
            }
            Self::UpcomingHasArtifacts { product_id } => {
                write!(f, "upcoming product {} carries a release", product_id.0)
            }
            Self::ManifestMismatch { field } => {
                write!(f, "manifest {field} does not match the feed")
            }
            Self::ReleaseChanged { release_id } => write!(
                f,
                "release {release_id} was published with different contents; releases are immutable"
            ),
            Self::InvalidField { field, reason } => write!(f, "{field}: {reason}"),
        }
    }
}

impl std::error::Error for CatalogError {}

impl From<CatalogError> for LauncherError {
    fn from(e: CatalogError) -> Self {
        let code = match &e {
            CatalogError::UnsupportedSchema { found } if *found > u64::from(SCHEMA_VERSION) => {
                ErrorCode::LauncherUpdateRequired
            }
            CatalogError::LauncherTooOld { .. } => ErrorCode::LauncherUpdateRequired,
            CatalogError::ReleaseChanged { .. } => ErrorCode::ReleaseChanged,
            _ => ErrorCode::MetadataInvalid,
        };
        LauncherError::new(code, e.to_string())
    }
}
