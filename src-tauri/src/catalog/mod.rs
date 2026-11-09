//! Catalog module: read and validate the release feed and release manifests,
//! returning typed product and release descriptions (including
//! [`crate::contract::ExpectedInventory`]). Wire format: `docs/specs/release-manifest.md`.
//!
//! Parsing is plain over bytes: no network, no filesystem. [`refresh`] is the
//! networked entry point; it downloads, validates and caches the feed and manifests,
//! in two phases ([`fetch`], then [`apply`]) that callers can run apart.
//! Missing, placeholder or unsupported data is a [`CatalogError`] and never
//! fabricates readiness. Path and hash rules come from [`crate::contract`]; this
//! module does not redefine them.
//!
//! [`news`] reads the separate, untrusted `news.json` (#80); it shares only the
//! public-HTTPS rule.

mod check;
mod error;
mod feed;
mod manifest;
pub mod news;
mod sync;

pub(crate) use check::https as public_https;
pub use error::CatalogError;
pub use feed::{Availability, Feed, FeedProduct, ReleasePointer, MAX_MANIFEST_SIZE};
pub use manifest::{
    Archive, ArchiveFormat, Compatibility, GameBuild, LoaderBuild, Ownership, Platform,
    ReleaseManifest, ReleaseStatus, Saves, Setting,
};
pub use sync::{
    apply, fetch, manifest_identity, refresh, CachedSha, Fetch, Fetched, DEFAULT_FEED_URL,
    FEED_IDENTITY, MAX_FEED_SIZE,
};

/// Schema version of the feed and release manifest this launcher reads. Schema 2 is
/// the toolkit packager's current output and is incomplete for the safe-update
/// contract (#6); a newer schema means a launcher update is required.
pub const SCHEMA_VERSION: u32 = 3;

#[cfg(test)]
mod tests;
