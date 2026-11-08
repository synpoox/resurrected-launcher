//! Catalog refresh: download the release feed and the manifests it recommends,
//! validate everything, and only then replace State's metadata cache.
//!
//! All or nothing: offline, a failed download, invalid metadata or a changed
//! release leaves the cache exactly as it was, so the last valid metadata stays in
//! use and nothing unverified is reported as ready.

use std::sync::atomic::AtomicBool;

use super::check;
use super::{Availability, CatalogError, Feed, ReleaseManifest};
use crate::contract::{ErrorCode, LauncherError, ProductId, Sha256Digest};
use crate::install::download::{Downloader, Expect};
use crate::state::StateStore;

/// The publisher's mutable feed: GitHub redirects `latest/download` to the newest
/// release's asset, then to its object storage (#62).
pub const DEFAULT_FEED_URL: &str =
    "https://github.com/synpoox/pd2-resurrected/releases/latest/download/feed.json";

/// Cache identity of the feed.
pub const FEED_IDENTITY: &str = "feed";

/// Largest feed the launcher downloads; its size and hash are unknown up front.
pub const MAX_FEED_SIZE: u64 = 1024 * 1024;

/// Cache identity of a release manifest.
pub fn manifest_identity(product: &ProductId, release_id: &str) -> String {
    format!("manifest/{}/{release_id}", product.0)
}

/// Refreshes the cached feed and recommended manifests from `feed_url` (HTTPS, a
/// public host). Returns the new feed. [`fetch`] then [`apply`], holding `state`
/// throughout; callers sharing State behind a lock use those two directly.
///
/// Errors: [`crate::contract::ErrorCode::ReleaseChanged`] when a known
/// `release_id` now points at different manifest bytes, `metadata_invalid` for
/// invalid metadata, and the download errors of [`Downloader::fetch_bytes`]. On
/// any error the cache is unchanged.
pub fn refresh(
    downloader: &Downloader,
    state: &mut StateStore,
    feed_url: &str,
) -> Result<Feed, LauncherError> {
    let cancel = AtomicBool::new(false);
    let fetched = fetch(
        feed_url,
        &mut |url, expect| downloader.fetch_bytes(url, expect, &cancel),
        &mut |identity| state.cached(identity).map(|c| c.sha256),
    )?;
    apply(state, fetched)
}

/// Download function of [`fetch`]: [`Downloader::fetch_bytes`] semantics.
pub type Fetch<'a> =
    dyn FnMut(&str, Expect<'_>) -> Result<(Vec<u8>, Sha256Digest), LauncherError> + 'a;

/// SHA-256 of the cached metadata with this cache identity, if any.
pub type CachedSha<'a> = dyn FnMut(&str) -> Option<Sha256Digest> + 'a;

/// A downloaded feed and the manifests it recommends that were not already cached,
/// each validated on its own. From [`fetch`]; nothing is cached until [`apply`].
#[derive(Debug)]
pub struct Fetched {
    feed_bytes: Vec<u8>,
    feed: Feed,
    /// `(cache identity, bytes)`.
    manifests: Vec<(String, Vec<u8>)>,
}

/// First phase of a refresh, without State: downloads the feed from `feed_url`
/// (HTTPS, a public host) and every recommended manifest whose identity `cached`
/// does not already hold with the pinned SHA-256, and validates each. `download`
/// is [`Downloader::fetch_bytes`] in production; tests route URLs through it.
///
/// Errors: `metadata_invalid` for invalid metadata and the errors of `download`.
pub fn fetch(
    feed_url: &str,
    download: &mut Fetch<'_>,
    cached: &mut CachedSha<'_>,
) -> Result<Fetched, LauncherError> {
    let feed_url = check::https("feed_url", feed_url.to_owned())?;
    let (feed_bytes, _) = download(&feed_url, Expect::AtMost(MAX_FEED_SIZE))?;
    let feed = Feed::parse(&feed_bytes)?;
    let mut manifests = Vec::new();
    for product in &feed.products {
        let Availability::Available(pointer) = &product.availability else {
            continue;
        };
        let identity = manifest_identity(&product.product_id, &pointer.release_id);
        if cached(&identity).is_some_and(|sha| sha == pointer.manifest_sha256) {
            continue;
        }
        // manifest_size is capped by Feed::parse (MAX_MANIFEST_SIZE).
        let expect = Expect::Exact {
            size: pointer.manifest_size,
            sha256: &pointer.manifest_sha256,
        };
        let (bytes, _) = download(&pointer.manifest_url, expect)?;
        product.check_manifest(&ReleaseManifest::parse(&bytes)?)?;
        manifests.push((identity, bytes));
    }
    Ok(Fetched {
        feed_bytes,
        feed,
        manifests,
    })
}

/// Second phase of a refresh: checks `fetched` against the cached feed and
/// manifests (releases are immutable), then caches it. Returns the new feed.
///
/// Errors: `release_changed` when a known `release_id` now points at different
/// manifest bytes, and `internal` when a manifest [`fetch`] found cached is gone.
/// On any error the cache is unchanged.
pub fn apply(state: &mut StateStore, fetched: Fetched) -> Result<Feed, LauncherError> {
    let Fetched {
        feed_bytes,
        feed,
        manifests,
    } = fetched;
    let cached_feed = state
        .cached(FEED_IDENTITY)
        .and_then(|c| Feed::parse(&c.bytes).ok());

    let mut new = Vec::new();
    for product in &feed.products {
        let Availability::Available(pointer) = &product.availability else {
            continue;
        };
        let known = cached_feed
            .as_ref()
            .and_then(|f| f.product(&product.product_id));
        if let Some(Availability::Available(old)) = known.map(|p| &p.availability) {
            pointer.check_unchanged(old)?;
        }
        let identity = manifest_identity(&product.product_id, &pointer.release_id);
        match state.cached(&identity) {
            Some(c) if c.sha256 == pointer.manifest_sha256 => continue,
            Some(_) => {
                return Err(CatalogError::ReleaseChanged {
                    release_id: pointer.release_id.clone(),
                }
                .into())
            }
            None => {}
        }
        let Some((_, bytes)) = manifests.iter().find(|(i, _)| *i == identity) else {
            return Err(LauncherError::new(
                ErrorCode::Internal,
                format!("cached manifest {identity} disappeared during the refresh"),
            ));
        };
        new.push((identity, bytes));
    }

    // Manifests before the feed that points at them.
    for (identity, bytes) in new {
        state.put_cached(&identity, bytes)?;
    }
    state.put_cached(FEED_IDENTITY, &feed_bytes)?;
    Ok(feed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Live check that GitHub's `latest/download` redirects (to the tag, then to
    /// object storage) download over HTTPS. Network; run with `--ignored`.
    #[test]
    #[ignore = "network: github.com"]
    fn default_feed_host_redirects_reach_object_storage() {
        let url = DEFAULT_FEED_URL.replace("feed.json", "SHA256SUMS");
        let (bytes, _) = Downloader::new()
            .fetch_bytes(&url, Expect::AtMost(MAX_FEED_SIZE), &AtomicBool::new(false))
            .unwrap();
        assert!(String::from_utf8(bytes).unwrap().contains(".7z"));
    }
}
