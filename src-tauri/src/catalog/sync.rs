//! Catalog refresh: download the release feed and the manifests it recommends,
//! validate everything, and only then replace State's metadata cache.
//!
//! All or nothing: offline, a failed download, invalid metadata or a changed
//! release leaves the cache exactly as it was, so the last valid metadata stays in
//! use and nothing unverified is reported as ready.

use std::sync::atomic::AtomicBool;

use super::check;
use super::{Availability, CatalogError, Feed, ReleaseManifest};
use crate::contract::{LauncherError, ProductId, Sha256Digest};
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
/// public host). Returns the new feed.
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
    refresh_with(state, feed_url, &mut |url, expect| {
        downloader.fetch_bytes(url, expect, &cancel)
    })
}

/// Download function of [`refresh_with`]: [`Downloader::fetch_bytes`] semantics.
pub type Fetch<'a> =
    dyn FnMut(&str, Expect<'_>) -> Result<(Vec<u8>, Sha256Digest), LauncherError> + 'a;

/// [`refresh`] with its downloads done by `fetch`. Tests route the public-looking
/// URLs to the local fixture server through it. Not for production use.
#[doc(hidden)]
pub fn refresh_with(
    state: &mut StateStore,
    feed_url: &str,
    fetch: &mut Fetch<'_>,
) -> Result<Feed, LauncherError> {
    let feed_url = check::https("feed_url", feed_url.to_owned())?;
    let (feed_bytes, _) = fetch(&feed_url, Expect::AtMost(MAX_FEED_SIZE))?;
    let feed = Feed::parse(&feed_bytes)?;
    let cached_feed = state
        .cached(FEED_IDENTITY)
        .and_then(|c| Feed::parse(&c.bytes).ok());

    let mut manifests = Vec::new();
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
        // manifest_size is capped by Feed::parse (MAX_MANIFEST_SIZE).
        let expect = Expect::Exact {
            size: pointer.manifest_size,
            sha256: &pointer.manifest_sha256,
        };
        let (bytes, _) = fetch(&pointer.manifest_url, expect)?;
        product.check_manifest(&ReleaseManifest::parse(&bytes)?)?;
        manifests.push((identity, bytes));
    }

    // Manifests before the feed that points at them.
    for (identity, bytes) in &manifests {
        state.put_cached(identity, bytes)?;
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
