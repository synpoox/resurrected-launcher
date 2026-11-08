//! Self-tests of the fixture harness in `support/` (#38).

mod support;

use std::fs;
use std::io::Read;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use resurrected_launcher_lib::catalog::{Feed, Ownership};
use resurrected_launcher_lib::contract::{ErrorCode, ProductId, RelativePath};
use resurrected_launcher_lib::install::extract::{extract, verify_staging};
use resurrected_launcher_lib::install::paths::TransactionPaths;
use support::*;

fn not_cancelled() -> AtomicBool {
    AtomicBool::new(false)
}

#[test]
fn fixture_release_archive_extracts_to_exactly_its_manifest_inventory() {
    for name in ALL_RELEASES {
        let release = release(name);
        let game = DisposableGame::new();
        let guard = WriteGuard::check(&game.product_root(&release)).unwrap();
        let paths = TransactionPaths::of(&guard);
        fs::write(paths.archive(), &release.archive).unwrap();
        let inventory = &release.manifest.inventory;
        extract(&seven_zip(), &release.manifest, &paths, &not_cancelled())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        // The archive's top folder is stripped: staging mirrors the product root.
        assert!(!paths.staging().join(release.manifest.mod_folder()).exists());
        verify_staging(&paths, inventory, &not_cancelled()).unwrap();
        let listed: Vec<&str> = inventory.files().iter().map(|f| f.path.as_str()).collect();
        assert_eq!(listed, release.payload.keys().collect::<Vec<_>>(), "{name}");
    }
}

#[test]
fn fixture_feed_recommends_each_release_manifest() {
    let releases: Vec<_> = [PD2R_2, FOREVER_1].map(release).into();
    let feed = Feed::parse(&feed(&releases.iter().collect::<Vec<_>>())).unwrap();
    for r in &releases {
        let entry = feed.product(&r.manifest.product_id).unwrap();
        entry.check_manifest(&r.manifest).unwrap();
    }
    assert!(feed.product(&ProductId("project-09".into())).is_none());
}

#[test]
fn fixture_releases_carry_shipped_defaults_updates_and_a_save_breaking_release() {
    let (r1, r2, r3) = (release(PD2R_1), release(PD2R_2), release(PD2R_3));
    let filter = RelativePath::new(LOOT_FILTER).unwrap();
    for r in [&r1, &r2, &r3] {
        assert_eq!(r.manifest.ownership_of(&filter), Ownership::ShippedDefault);
        assert_eq!(
            r.manifest
                .ownership_of(&RelativePath::new(UNKNOWN_PLUGIN).unwrap()),
            Ownership::Unknown
        );
    }
    assert_ne!(r1.payload[LOOT_FILTER], r2.payload[LOOT_FILTER]);
    assert_eq!(
        r1.payload["d2rloader/config.ini"],
        r2.payload["d2rloader/config.ini"]
    );
    assert!(r1
        .payload
        .contains_key("PD2R.mpq/data/global/excel/retired.txt"));
    assert!(!r2
        .payload
        .contains_key("PD2R.mpq/data/global/excel/retired.txt"));
    assert!(!r1.manifest.is_save_breaking() && !r2.manifest.is_save_breaking());
    assert!(r3.manifest.is_save_breaking());
    assert_eq!(r1.manifest.inventory.empty_dirs().len(), 1);
    assert_eq!(release(FOREVER_1).manifest.mod_folder(), "Forever");
}

#[test]
fn malicious_fixture_archives_are_refused_before_extraction() {
    for malice in Malice::ALL {
        let release = malicious(malice);
        let game = DisposableGame::new();
        let guard = WriteGuard::check(&game.product_root(&release)).unwrap();
        let paths = TransactionPaths::of(&guard);
        fs::write(paths.archive(), &release.archive).unwrap();
        let err = extract(&seven_zip(), &release.manifest, &paths, &not_cancelled())
            .expect_err(release.release_id());
        // Each fixture must trip its own rule, not an incidental one.
        let why = match malice {
            Malice::Traversal => "DotComponent",
            Malice::Absolute => "Colon",
            Malice::CaseConflict => "differs only in case",
            Malice::Unlisted | Malice::Stray | Malice::Flat => "not in the manifest",
            Malice::Junction => "reparse point",
        };
        assert_eq!(err.code, ErrorCode::UnsafeArchiveEntry, "{malice:?}: {err}");
        assert!(err.message.contains(why), "{malice:?}: {err}");
        assert!(!paths.staging().exists(), "{malice:?}");
    }
}

#[test]
fn disposable_install_holds_the_release_and_player_files() {
    let release = release(PD2R_1);
    let game = DisposableGame::new();
    let root = game.installed(&release);
    assert!(root.starts_with(game.d2r()));
    WriteGuard::check(&root).unwrap();
    for (rel, bytes) in &release.payload {
        assert_eq!(&fs::read(root.join(rel)).unwrap(), bytes, "{rel}");
    }
    assert!(root.join("d2rloader/patches").is_dir());
    let edited = edit_loot_filter(&root);
    assert_ne!(edited, release.payload[LOOT_FILTER]);
    assert_eq!(
        add_unknown_plugin(&root),
        fs::read(root.join(UNKNOWN_PLUGIN)).unwrap()
    );
    assert!(
        WriteGuard::check(game.d2r()).is_err(),
        "only product roots carry the sentinel"
    );
}

// ---- fixture server ----

fn client(server: &FixtureServer) -> ureq::Agent {
    ureq::Agent::config_builder()
        .max_redirects(0)
        .http_status_as_error(false)
        .tls_config(server.tls_config())
        .build()
        .into()
}

/// Status, Location and the body read result.
fn get(server: &FixtureServer, path: &str) -> (u16, Option<String>, std::io::Result<Vec<u8>>) {
    let response = client(server).get(&server.url(path)).call().unwrap();
    let status = response.status().as_u16();
    let location = response
        .headers()
        .get("location")
        .map(|v| v.to_str().unwrap().to_owned());
    let mut body = Vec::new();
    let read = response
        .into_body()
        .into_reader()
        .read_to_end(&mut body)
        .map(|_| body);
    (status, location, read)
}

#[test]
fn fixture_server_serves_a_release_archive() {
    let release = release(PD2R_1);
    let server = FixtureServer::start();
    let archive = server.serve(&release);
    assert!(archive.url.starts_with("https://127.0.0.1:"));
    assert_eq!(archive.sha256, release.manifest.archive.sha256);
    let (status, _, body) = get(&server, &release.archive_path());
    assert_eq!(status, 200);
    assert_eq!(sha256(&body.unwrap()), archive.sha256);
    let (_, _, manifest) = get(&server, &release.manifest_path());
    assert_eq!(manifest.unwrap(), release.manifest_json);
    assert_eq!(get(&server, "/missing").0, 404);
    assert_eq!(server.hits().len(), 3);
}

#[test]
fn fixture_server_injects_failures() {
    let body: Vec<u8> = (0..200_000u32).map(|i| i as u8).collect();
    let server = FixtureServer::start();
    let route = |path: &str, reply| server.route(path, reply);
    route(
        "/truncated",
        Reply::Truncated {
            body: body.clone(),
            after: 1000,
        },
    );
    route(
        "/reset",
        Reply::Reset {
            body: body.clone(),
            after: 1000,
        },
    );
    route("/wrong-hash", Reply::WrongHash(body.clone()));
    route("/http", Reply::RedirectToHttp("/plain".into()));
    route("/moved", Reply::Redirect("/ok".into()));
    route("/down", Reply::Status(503));
    route(
        "/slow",
        Reply::Delayed(Duration::from_millis(300), Box::new(Reply::Status(500))),
    );
    route("/ok", Reply::Body(body.clone()));

    assert!(get(&server, "/truncated").2.is_err(), "truncation");
    assert!(get(&server, "/reset").2.is_err(), "connection reset");
    let wrong = get(&server, "/wrong-hash").2.unwrap();
    assert_eq!(wrong.len(), body.len());
    assert_ne!(sha256(&wrong), sha256(&body));
    let (status, location, _) = get(&server, "/http");
    assert_eq!(status, 302);
    assert!(location.unwrap().starts_with("http://127.0.0.1:"));
    let (status, location, _) = get(&server, "/moved");
    assert_eq!((status, location), (302, Some(server.url("/ok"))));
    assert_eq!(get(&server, "/down").0, 503);
    let started = Instant::now();
    assert_eq!(get(&server, "/slow").0, 500);
    assert!(started.elapsed() >= Duration::from_millis(300), "delay");

    server.once("/ok", Reply::Status(500));
    assert_eq!(get(&server, "/ok").0, 500, "one-shot failure first");
    assert_eq!(
        get(&server, "/ok").2.unwrap(),
        body,
        "then the standing reply"
    );
}
