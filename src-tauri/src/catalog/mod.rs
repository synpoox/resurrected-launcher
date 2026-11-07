//! Catalog module: read and validate the release feed and release manifests,
//! returning typed product and release descriptions (including
//! [`crate::contract::ExpectedInventory`]). Wire format: `docs/specs/release-manifest.md`.
//!
//! Plain parsing over bytes: no network, no filesystem. Missing, placeholder or
//! unsupported data is a [`CatalogError`] and never fabricates readiness. Path and
//! hash rules come from [`crate::contract`]; this module does not redefine them.

mod check;
mod error;
mod feed;
mod manifest;

pub use error::CatalogError;
pub use feed::{Availability, Feed, FeedProduct, ReleasePointer};
pub use manifest::{
    Archive, ArchiveFormat, Compatibility, GameBuild, LoaderPin, Ownership, Platform,
    ReleaseManifest, ReleaseStatus, Saves, Setting,
};

/// Schema version of the feed and release manifest this launcher reads. Schema 2 is
/// the toolkit packager's current output and is incomplete for the safe-update
/// contract (#6); a newer schema means a launcher update is required.
pub const SCHEMA_VERSION: u32 = 3;

#[cfg(test)]
mod tests;
