//! Catalog module: read and validate the release feed and release manifests,
//! returning typed product and release descriptions (including
//! [`crate::contract::ExpectedInventory`]). Wire format: `docs/specs/release-manifest.md`.
//!
//! Parsing is plain over bytes: no network, no filesystem. [`refresh`] is the one
//! networked entry point; it downloads, validates and caches the feed and manifests.
//! Missing, placeholder or unsupported data is a [`CatalogError`] and never
//! fabricates readiness. Path and hash rules come from [`crate::contract`]; this
//! module does not redefine them.

mod check;
mod error;
mod feed;
mod manifest;
mod sync;

pub use error::CatalogError;
pub use feed::{Availability, Feed, FeedProduct, ReleasePointer, MAX_MANIFEST_SIZE};
pub use manifest::{
    Archive, ArchiveFormat, Compatibility, GameBuild, Ownership, Platform, ReleaseManifest,
    ReleaseStatus, Saves, Setting,
};
pub use sync::{
    manifest_identity, refresh, refresh_with, DEFAULT_FEED_URL, FEED_IDENTITY, MAX_FEED_SIZE,
};

/// Schema version of the feed and release manifest this launcher reads. Schema 2 is
/// the toolkit packager's current output and is incomplete for the safe-update
/// contract (#6); a newer schema means a launcher update is required.
pub const SCHEMA_VERSION: u32 = 3;

#[cfg(test)]
mod tests;
