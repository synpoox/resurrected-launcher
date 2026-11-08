//! Catalog refresh (#56) over the local TLS fixture server, with State in a real
//! temp dir. Feeds carry public-looking URLs (the catalog rejects local hosts), so
//! the downloads are routed to the fixture server; every byte still goes through
//! `Downloader::fetch_bytes` with its HTTPS, redirect, size and hash checks.

mod support;

use resurrected_launcher_lib::catalog::{
    apply, fetch, manifest_identity, refresh, Feed, Fetch, FEED_IDENTITY,
};
use resurrected_launcher_lib::contract::{ErrorCode, LauncherError, Sha256Digest};
use resurrected_launcher_lib::install::download::{Downloader, Expect};
use resurrected_launcher_lib::state::StateStore;
use std::sync::atomic::AtomicBool;
use support::*;

const HOST: &str = "https://fixtures.example.org";
const FEED_URL: &str = "https://fixtures.example.org/feed.json";

struct Rig {
    server: FixtureServer,
    downloader: Downloader,
    _dir: tempfile::TempDir,
    state: StateStore,
}

impl Rig {
    fn new() -> Self {
        let server = FixtureServer::start();
        let dir = tempfile::tempdir().unwrap();
        Self {
            downloader: Downloader::with_tls(server.tls_config()),
            server,
            state: StateStore::open(dir.path()),
            _dir: dir,
        }
    }

    /// Serves `feed` at `/feed.json` and each release's manifest.
    fn publish(&self, feed: Vec<u8>, releases: &[&FixtureRelease]) {
        self.server.route("/feed.json", Reply::Body(feed));
        for r in releases {
            self.server
                .route(&r.manifest_path(), Reply::Body(r.manifest_json.clone()));
        }
    }

    fn refresh(&mut self) -> Result<Feed, LauncherError> {
        let (server, downloader) = (&self.server, &self.downloader);
        let cancel = AtomicBool::new(false);
        let mut fetch = |url: &str, expect: Expect<'_>| {
            let local = server.url(url.strip_prefix(HOST).expect("fixture host"));
            downloader.fetch_bytes(&local, expect, &cancel)
        };
        refresh_with(&mut self.state, FEED_URL, &mut fetch)
    }

    fn cached(&mut self, identity: &str) -> Option<Vec<u8>> {
        self.state.cached(identity).map(|c| c.bytes)
    }
}

/// `refresh`'s two phases with the downloads done by `download`.
fn refresh_with(
    state: &mut StateStore,
    feed_url: &str,
    download: &mut Fetch<'_>,
) -> Result<Feed, LauncherError> {
    let fetched = fetch(feed_url, download, &mut |identity| {
        state.cached(identity).map(|c| c.sha256)
    })?;
    apply(state, fetched)
}

fn identity(r: &FixtureRelease) -> String {
    manifest_identity(&r.manifest.product_id, r.release_id())
}

fn code(r: Result<Feed, LauncherError>) -> ErrorCode {
    r.expect_err("refresh must fail").code
}

/// `release`'s manifest re-published in place with different bytes.
fn republished(release: &FixtureRelease) -> FixtureRelease {
    let mut r = release.clone();
    r.manifest_json.extend_from_slice(b"\n");
    r
}

#[test]
fn refresh_caches_the_validated_feed_and_recommended_manifests_following_redirects() {
    let mut rig = Rig::new();
    let (pd2r, forever) = (release(PD2R_REAL_LAYOUT), release(FOREVER_1));
    let feed = support::feed(&[&pd2r, &forever]);
    rig.publish(feed.clone(), &[&forever]);
    // GitHub answers latest/download and the asset URL with redirects.
    rig.server.route(
        &pd2r.manifest_path(),
        Reply::Redirect("/storage/pd2r".into()),
    );
    rig.server
        .route("/storage/pd2r", Reply::Body(pd2r.manifest_json.clone()));

    let got = rig.refresh().unwrap();
    assert_eq!(got, Feed::parse(&feed).unwrap());
    assert_eq!(rig.cached(FEED_IDENTITY), Some(feed));
    assert_eq!(
        rig.cached(&identity(&pd2r)),
        Some(pd2r.manifest_json.clone())
    );
    assert_eq!(rig.cached(&identity(&forever)), Some(forever.manifest_json));
    assert!(rig.server.hits().contains(&"/storage/pd2r".to_owned()));

    // Unchanged: cached manifests are not downloaded again.
    let before = rig.server.hits().len();
    rig.refresh().unwrap();
    assert_eq!(rig.server.hits()[before..], ["/feed.json".to_owned()]);
}

#[test]
fn a_known_release_with_different_manifest_bytes_is_release_changed_and_the_cache_is_kept() {
    let mut rig = Rig::new();
    let (r1, r2) = (release(PD2R_1), release(PD2R_2));
    let first = support::feed(&[&r1]);
    rig.publish(first.clone(), &[&r1]);
    rig.refresh().unwrap();

    // Same release_id, new manifest bytes, while the cached feed still points at it.
    let changed = republished(&r1);
    rig.publish(support::feed(&[&changed]), &[&changed]);
    assert_eq!(code(rig.refresh()), ErrorCode::ReleaseChanged);
    assert_eq!(rig.cached(FEED_IDENTITY), Some(first));
    assert_eq!(rig.cached(&identity(&r1)), Some(r1.manifest_json.clone()));

    // The feed moved on to 2, then back to a republished 1: the cached manifest
    // still identifies it.
    let second = support::feed(&[&r2]);
    rig.publish(second.clone(), &[&r2]);
    rig.refresh().unwrap();
    rig.publish(support::feed(&[&changed]), &[&changed]);
    assert_eq!(code(rig.refresh()), ErrorCode::ReleaseChanged);
    assert_eq!(rig.cached(FEED_IDENTITY), Some(second));
}

#[test]
fn offline_or_unverified_downloads_keep_the_last_valid_metadata() {
    let mut rig = Rig::new();
    let (r1, r2) = (release(PD2R_1), release(PD2R_2));
    let first = support::feed(&[&r1]);
    rig.publish(first.clone(), &[&r1]);
    rig.refresh().unwrap();

    rig.server.route("/feed.json", Reply::Status(503));
    assert_eq!(code(rig.refresh()), ErrorCode::DownloadFailed);

    // The new feed is fine but its manifest bytes are not what it pins.
    rig.server
        .route("/feed.json", Reply::Body(support::feed(&[&r2])));
    rig.server.route(
        &r2.manifest_path(),
        Reply::WrongHash(r2.manifest_json.clone()),
    );
    assert_eq!(code(rig.refresh()), ErrorCode::VerificationFailed);

    // A manifest that is not the recommended release.
    rig.server
        .route(&r2.manifest_path(), Reply::Body(r1.manifest_json.clone()));
    let mut wrong = support::feed(&[&r2]);
    let (old, new) = (
        format!("\"manifest_size\": {}", r2.manifest_json.len()),
        format!("\"manifest_size\": {}", r1.manifest_json.len()),
    );
    let text = String::from_utf8(wrong)
        .unwrap()
        .replace(&old, &new)
        .replace(&hex(&r2.manifest_json), &hex(&r1.manifest_json));
    wrong = text.into_bytes();
    rig.server.route("/feed.json", Reply::Body(wrong));
    assert_eq!(code(rig.refresh()), ErrorCode::MetadataInvalid);

    rig.server
        .route("/feed.json", Reply::Body(b"not json".to_vec()));
    assert_eq!(code(rig.refresh()), ErrorCode::MetadataInvalid);

    assert_eq!(rig.cached(FEED_IDENTITY), Some(first));
    assert_eq!(rig.cached(&identity(&r2)), None);
}

#[test]
fn refresh_accepts_only_an_https_feed_on_a_public_host() {
    let mut rig = Rig::new();
    let mut never = |_: &str, _: Expect<'_>| -> Result<(Vec<u8>, Sha256Digest), LauncherError> {
        panic!("nothing may be downloaded")
    };
    for url in [
        "http://fixtures.example.org/feed.json",
        "https://127.0.0.1/feed.json",
    ] {
        let got = refresh_with(&mut rig.state, url, &mut never);
        assert_eq!(code(got), ErrorCode::MetadataInvalid, "{url}");
    }
    // The real downloader refuses a redirect off HTTPS.
    let feed = rig.server.url("/feed.json");
    rig.server
        .route("/feed.json", Reply::RedirectToHttp("/plain".into()));
    let cancel = AtomicBool::new(false);
    let got = rig
        .downloader
        .fetch_bytes(&feed, Expect::AtMost(1024), &cancel);
    assert_eq!(got.unwrap_err().code, ErrorCode::DownloadFailed);
    // `refresh` itself goes through the same checks.
    assert_eq!(
        code(refresh(
            &rig.downloader,
            &mut rig.state,
            "http://example.org/feed.json"
        )),
        ErrorCode::MetadataInvalid
    );
    assert!(rig.server.hits().iter().all(|h| h == "/feed.json"));
    assert_eq!(rig.cached(FEED_IDENTITY), None);
}

fn hex(bytes: &[u8]) -> String {
    sha256(bytes).to_string()
}
