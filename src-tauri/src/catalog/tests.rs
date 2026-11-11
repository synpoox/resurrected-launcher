//! Fixture tests: one valid feed and release manifest, plus one malformed file per
//! rejection class in `fixtures/catalog/invalid/`, each asserting its exact error.

use std::fs;
use std::path::PathBuf;

use crate::contract::{
    ErrorCode, InventoryError, LauncherError, PathError, ProductId, RelativePath, Sha256Digest,
};

use super::*;

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/catalog")
        .join(name);
    fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn valid_manifest() -> ReleaseManifest {
    ReleaseManifest::parse(&fixture("valid/release-manifest.json")).unwrap()
}

fn valid_feed() -> Feed {
    Feed::parse(&fixture("valid/feed.json")).unwrap()
}

fn path(s: &str) -> RelativePath {
    RelativePath::new(s).unwrap()
}

fn field(f: &str) -> String {
    f.to_owned()
}

fn placeholder(f: &str) -> CatalogError {
    CatalogError::PlaceholderVerification { field: field(f) }
}

fn not_https(f: &str) -> CatalogError {
    CatalogError::NotHttps { field: field(f) }
}

fn unsafe_path(f: &str, error: PathError) -> CatalogError {
    CatalogError::UnsafePath {
        field: field(f),
        error,
    }
}

fn invalid(f: &str, reason: &'static str) -> CatalogError {
    CatalogError::InvalidField {
        field: field(f),
        reason,
    }
}

fn manifest_rejections() -> Vec<(&'static str, CatalogError)> {
    use CatalogError::*;
    vec![
        (
            "url-dot-segment",
            invalid("archive.url", "URL path must not contain dot segments"),
        ),
        ("url-port", not_https("archive.url")),
        ("url-ip-literal", not_https("archive.url")),
        ("url-localhost", not_https("archive.url")),
        (
            "total-size-overflow",
            ArchiveInconsistent {
                field: field("archive.total_size"),
            },
        ),
        (
            "install-root-not-mods",
            invalid("install_root", "must be mods/<folder>"),
        ),
        (
            "install-root-nested",
            invalid("install_root", "must be mods/<folder>"),
        ),
        (
            "install-root-switch",
            invalid("install_root", "folder must not start with -"),
        ),
        (
            "setting-not-shipped-default",
            invalid(
                "compatibility.required_settings[0].file",
                "must be a shipped_default file of this release",
            ),
        ),
        (
            "previous-default-on-managed",
            invalid(
                "files[0].previous_default_sha256",
                "only a shipped_default lists previous hashes",
            ),
        ),
        (
            "previous-default-placeholder",
            placeholder("files[5].previous_default_sha256[1]"),
        ),
        (
            "ownership-unknown",
            invalid(
                "files[0].ownership",
                "listed files are managed or shipped_default",
            ),
        ),
        (
            "notice-without-save-breaking",
            invalid(
                "saves.incompatibility_notice",
                "required exactly when save_breaking is true",
            ),
        ),
        (
            "saves-folder-nested",
            invalid("saves.folder", "must be one folder name"),
        ),
        ("empty-files", invalid("files", "must not be empty")),
        ("zero-hash", placeholder("archive.sha256")),
        ("missing-hash", placeholder("files[1].sha256")),
        ("missing-size", placeholder("archive.size")),
        (
            "zero-size",
            placeholder("compatibility.game_builds[0].size"),
        ),
        ("empty-file-placeholder-hash", placeholder("files[2].size")),
        ("not-https-archive", not_https("archive.url")),
        (
            "absolute-path",
            unsafe_path("files[0].path", PathError::Colon),
        ),
        (
            "escaping-path",
            unsafe_path("files[0].path", PathError::DotComponent),
        ),
        (
            "reserved-name",
            unsafe_path("files[0].path", PathError::ReservedName),
        ),
        (
            "absolute-install-root",
            unsafe_path("install_root", PathError::Absolute),
        ),
        (
            "duplicate-path",
            Inventory(InventoryError::DuplicatePath(path(
                "PD2R.mpq/data/global/excel/armor.txt",
            ))),
        ),
        (
            "case-conflict-path",
            Inventory(InventoryError::DuplicatePath(path(
                "pd2r.mpq/DATA/global/excel/armor.txt",
            ))),
        ),
        (
            "file-under-file",
            Inventory(InventoryError::PathConflict {
                outer: path("PD2R.mpq/modinfo.json"),
                inner: path("PD2R.mpq/modinfo.json/x.txt"),
            }),
        ),
        (
            "archive-file-count",
            ArchiveInconsistent {
                field: field("archive.file_count"),
            },
        ),
        (
            "archive-total-size",
            ArchiveInconsistent {
                field: field("archive.total_size"),
            },
        ),
        ("unsupported-schema", UnsupportedSchema { found: 4 }),
        ("schema-2", UnsupportedSchema { found: 2 }),
        (
            "command",
            ExecutableMetadata {
                key: field("post_install_command"),
            },
        ),
        (
            "nested-script",
            ExecutableMetadata {
                key: field("install_script"),
            },
        ),
        (
            "launcher-too-old",
            LauncherTooOld {
                required: field("999.0.0"),
            },
        ),
        (
            "no-game-builds",
            InvalidField {
                field: field("compatibility.game_builds"),
                reason: "must not be empty",
            },
        ),
        (
            "loader-build-unknown-version",
            invalid(
                "compatibility.loader_builds[3].version",
                "must be one of compatibility.loader_versions",
            ),
        ),
        (
            "loader-build-unpinned-version",
            invalid(
                "compatibility.loader_versions[0]",
                "has no pinned build in compatibility.loader_builds",
            ),
        ),
        (
            "loader-build-zero-hash",
            placeholder("compatibility.loader_builds[1].sha256"),
        ),
        (
            "loader-build-missing-size",
            placeholder("compatibility.loader_builds[1].size"),
        ),
        (
            "loader-build-duplicate-file",
            invalid(
                "compatibility.loader_builds[3].file",
                "duplicate file for this loader version",
            ),
        ),
        (
            "save-breaking-without-notice",
            InvalidField {
                field: field("saves.incompatibility_notice"),
                reason: "required exactly when save_breaking is true",
            },
        ),
    ]
}

fn feed_rejections() -> Vec<(&'static str, CatalogError)> {
    use CatalogError::*;
    vec![
        (
            "available-without-recommended",
            invalid(
                "products[0].recommended",
                "required for an available product",
            ),
        ),
        (
            "duplicate-product",
            invalid("products[3].product_id", "duplicate product"),
        ),
        (
            "not-https",
            not_https("products[0].recommended.manifest_url"),
        ),
        (
            "placeholder-hash",
            placeholder("products[0].recommended.manifest_sha256"),
        ),
        (
            "missing-size",
            placeholder("products[0].recommended.manifest_size"),
        ),
        (
            "manifest-too-large",
            invalid(
                "products[0].recommended.manifest_size",
                "larger than the 8 MiB manifest limit",
            ),
        ),
        (
            "upcoming-with-artifacts",
            UpcomingHasArtifacts {
                product_id: ProductId("project-09".to_owned()),
            },
        ),
        ("unsupported-schema", UnsupportedSchema { found: 4 }),
        (
            "command",
            ExecutableMetadata {
                key: field("refresh_command"),
            },
        ),
    ]
}

#[test]
fn valid_release_manifest_describes_the_release() {
    let m = valid_manifest();
    assert_eq!(m.product_id, ProductId("pd2r".to_owned()));
    assert_eq!(m.release_id, "pd2r-0.4.0-alpha.1");
    assert_eq!(m.status, ReleaseStatus::Alpha);
    assert_eq!(m.archive.format, ArchiveFormat::SevenZip);
    assert_eq!(m.inventory.files().len(), 6);
    assert_eq!(m.inventory.empty_dirs(), [path("d2rloader/patches")]);
    assert_eq!(m.mod_folder(), "PD2R");
    assert!(!m.is_save_breaking());
    assert_eq!(
        m.compatibility.game_builds[0].platform,
        Platform::WindowsX64
    );
    assert_eq!(m.compatibility.required_settings[0].key, "default_mod");
    assert_eq!(m.saves.folder, path("PD2R"));
}

/// #121: the official D2RLoader 1.3.1-beta is pinned by version and bytes.
#[test]
fn release_manifest_pins_loader_builds_by_version_and_sha256() {
    let m = valid_manifest();
    let pinned: Vec<_> = m
        .compatibility
        .loader_builds
        .iter()
        .filter(|b| b.version == "1.3.1-beta")
        .map(|b| (b.file.as_str(), b.size, b.sha256.to_string()))
        .collect();
    assert_eq!(
        pinned,
        [
            (
                "D2RLoader.exe",
                23_331_160,
                "93021dad48533bcfca8a95c69a0cc00cdb9008a3ce6ceba1d04c7aca71985a10".to_owned()
            ),
            (
                "D2RCore.dll",
                18_938_712,
                "2a868d013d2e0830bd2d9e04b918b19e46a73cf726c833e70d089b948fdeb5a2".to_owned()
            ),
        ]
    );
}

/// #121: `loader_builds` is additive; published manifests without it still parse.
#[test]
fn release_manifest_without_loader_builds_still_parses() {
    let m =
        ReleaseManifest::parse(&fixture("real/pd2r-0.1.0-alpha/release-manifest.json")).unwrap();
    assert_eq!(m.compatibility.loader_versions, ["1.3.1-beta"]);
    assert!(m.compatibility.loader_builds.is_empty());
}

#[test]
fn release_manifest_distinguishes_managed_files_shipped_defaults_and_unknown_files() {
    let m = valid_manifest();
    assert_eq!(
        m.ownership_of(&path("PD2R.mpq/modinfo.json")),
        Ownership::Managed
    );
    assert_eq!(
        m.ownership_of(&path("lootfilter/default.filter")),
        Ownership::ShippedDefault
    );
    assert_eq!(
        m.ownership_of(&path("D2RLOADER/Config.ini")),
        Ownership::ShippedDefault
    );
    assert_eq!(
        m.ownership_of(&path("d2rloader/plugins/my-plugin.dll")),
        Ownership::Unknown
    );
}

#[test]
fn each_malformed_release_manifest_fails_closed_with_its_typed_error() {
    for (name, expected) in manifest_rejections() {
        let got = ReleaseManifest::parse(&fixture(&format!("invalid/manifest-{name}.json")));
        assert_eq!(got, Err(expected), "manifest-{name}");
    }
}

#[test]
fn each_malformed_release_feed_fails_closed_with_its_typed_error() {
    for (name, expected) in feed_rejections() {
        let got = Feed::parse(&fixture(&format!("invalid/feed-{name}.json")));
        assert_eq!(got, Err(expected), "feed-{name}");
    }
}

#[test]
fn release_manifest_with_unknown_fields_is_malformed() {
    let got = ReleaseManifest::parse(&fixture("invalid/manifest-unknown-field.json"));
    assert!(
        matches!(&got, Err(CatalogError::Malformed(m)) if m.contains("unknown field `launch`")),
        "{got:?}"
    );
    let got = ReleaseManifest::parse(&fixture("invalid/manifest-duplicate-key.json"));
    assert!(
        matches!(&got, Err(CatalogError::Malformed(m)) if m.contains("duplicate field `release_id`")),
        "{got:?}"
    );
    assert!(matches!(
        ReleaseManifest::parse(b"not json"),
        Err(CatalogError::Malformed(_))
    ));
}

#[test]
fn newer_schema_says_a_launcher_update_is_required() {
    let e: LauncherError =
        ReleaseManifest::parse(&fixture("invalid/manifest-unsupported-schema.json"))
            .unwrap_err()
            .into();
    assert_eq!(e.code, ErrorCode::LauncherUpdateRequired);
    assert!(e.message.contains("a launcher update is required"), "{e}");

    let e: LauncherError = Feed::parse(&fixture("invalid/feed-unsupported-schema.json"))
        .unwrap_err()
        .into();
    assert_eq!(e.code, ErrorCode::LauncherUpdateRequired);

    let e: LauncherError = ReleaseManifest::parse(&fixture("invalid/manifest-schema-2.json"))
        .unwrap_err()
        .into();
    assert_eq!(e.code, ErrorCode::MetadataInvalid);
}

#[test]
fn republished_release_with_changed_hashes_is_rejected_against_cached_metadata() {
    let cached = valid_manifest();
    let republished =
        ReleaseManifest::parse(&fixture("invalid/manifest-changed-hash.json")).unwrap();
    let changed = CatalogError::ReleaseChanged {
        release_id: "pd2r-0.4.0-alpha.1".to_owned(),
    };
    assert_eq!(republished.check_unchanged(&cached), Err(changed.clone()));
    assert_eq!(cached.check_unchanged(&valid_manifest()), Ok(()));
    let e: LauncherError = changed.clone().into();
    assert_eq!(e.code, ErrorCode::ReleaseChanged);

    let pointer = |feed: &Feed| match &feed.products[0].availability {
        Availability::Available(p) => p.clone(),
        Availability::Upcoming => panic!("pd2r is available"),
    };
    let cached = pointer(&valid_feed());
    let republished = pointer(&Feed::parse(&fixture("invalid/feed-changed-pointer.json")).unwrap());
    assert_eq!(republished.check_unchanged(&cached), Err(changed));
    assert_eq!(cached.check_unchanged(&cached), Ok(()));
}

#[test]
fn release_feed_lists_available_and_upcoming_products() {
    let feed = valid_feed();
    let pd2r = feed.product(&ProductId("pd2r".to_owned())).unwrap();
    assert!(matches!(&pd2r.availability, Availability::Available(p) if p.manifest_size == 3072));
    for upcoming in ["project-09", "project-forever"] {
        let p = feed.product(&ProductId(upcoming.to_owned())).unwrap();
        assert_eq!(p.availability, Availability::Upcoming);
    }
    assert_eq!(pd2r.check_manifest(&valid_manifest()), Ok(()));
}

#[test]
fn release_feed_rejects_a_manifest_it_does_not_recommend() {
    let feed = valid_feed();
    let upcoming = feed.product(&ProductId("project-09".to_owned())).unwrap();
    assert_eq!(
        upcoming.check_manifest(&valid_manifest()),
        Err(CatalogError::ManifestMismatch {
            field: "release_id"
        })
    );
    let pd2r = feed.product(&ProductId("pd2r".to_owned())).unwrap();
    let mut other = valid_manifest();
    other.release_id = "pd2r-0.3.0".to_owned();
    assert_eq!(
        pd2r.check_manifest(&other),
        Err(CatalogError::ManifestMismatch {
            field: "release_id"
        })
    );
    other.product_id = ProductId("project-09".to_owned());
    assert_eq!(
        pd2r.check_manifest(&other),
        Err(CatalogError::ManifestMismatch {
            field: "product_id"
        })
    );
}

#[test]
fn every_invalid_fixture_is_asserted() {
    let mut covered: Vec<String> = manifest_rejections()
        .into_iter()
        .map(|(n, _)| format!("manifest-{n}.json"))
        .chain(
            feed_rejections()
                .into_iter()
                .map(|(n, _)| format!("feed-{n}.json")),
        )
        .collect();
    covered.extend(
        [
            "manifest-unknown-field.json",
            "manifest-duplicate-key.json",
            "manifest-changed-hash.json",
            "feed-changed-pointer.json",
        ]
        .map(String::from),
    );
    covered.sort();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/catalog/invalid");
    let mut on_disk: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    on_disk.sort();
    assert_eq!(on_disk, covered);
}

/// #134: a shipped default may list the hashes earlier releases shipped for it;
/// without the field (every other fixture) it lists none.
#[test]
fn release_manifest_lists_previous_shipped_default_hashes_case_insensitively() {
    let m =
        ReleaseManifest::parse(&fixture("valid/release-manifest-previous-defaults.json")).unwrap();
    let want: Sha256Digest = "ab".repeat(32).parse().unwrap();
    assert_eq!(
        m.previous_default_hashes(&path("LOOTFILTER/default.filter")),
        [want]
    );
    assert!(m
        .previous_default_hashes(&path("d2rloader/config.ini"))
        .is_empty());
    assert!(valid_manifest()
        .previous_default_hashes(&path("lootfilter/default.filter"))
        .is_empty());
}

#[test]
fn release_manifest_matches_setting_files_case_insensitively() {
    let m = ReleaseManifest::parse(&fixture("valid/release-manifest-setting-case.json")).unwrap();
    assert_eq!(
        m.compatibility.required_settings[0].file,
        path("D2RLOADER/Config.ini")
    );
}

/// Published PD2R v0.1.0-alpha (#56): the generated schema-3 manifest and feed
/// parse, the feed pins the manifest bytes, and `files` is exactly the archive.
#[test]
fn published_pd2r_0_1_0_alpha_manifest_matches_its_archive_listing() {
    use sha2::{Digest, Sha256};
    let bytes = fixture("real/pd2r-0.1.0-alpha/release-manifest.json");
    let m = ReleaseManifest::parse(&bytes).unwrap();
    let feed = Feed::parse(&fixture("real/pd2r-0.1.0-alpha/feed.json")).unwrap();
    let pd2r = feed.product(&ProductId("pd2r".to_owned())).unwrap();
    pd2r.check_manifest(&m).unwrap();
    let Availability::Available(pointer) = &pd2r.availability else {
        panic!("pd2r is available");
    };
    assert_eq!(pointer.manifest_size, bytes.len() as u64);
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    assert_eq!(pointer.manifest_sha256.as_bytes(), &digest);
    for upcoming in ["project-09", "project-forever"] {
        let p = feed.product(&ProductId(upcoming.to_owned())).unwrap();
        assert_eq!(p.availability, Availability::Upcoming);
    }

    assert_eq!(m.release_id, "pd2r-0.1.0-alpha");
    assert_eq!(m.mod_folder(), "PD2Resurrected");
    assert_eq!(m.archive.size, 72_790_802);
    assert_eq!(m.inventory.files().len(), 577);
    let total: u64 = m.inventory.files().iter().map(|f| f.size).sum();
    assert_eq!(total, 1_699_496_572);
    assert_eq!(m.compatibility.loader_versions, ["1.3.1-beta"]);
    let s = &m.compatibility.required_settings[0];
    assert_eq!(
        (s.file.as_str(), s.key.as_str(), s.value.as_str()),
        (
            "d2rloader/config/d2rloader.toml",
            "d2rcore.save_format.mode",
            "extended"
        )
    );
    let shipped: Vec<_> = m
        .inventory
        .files()
        .iter()
        .filter(|f| m.ownership_of(&f.path) == Ownership::ShippedDefault)
        .map(|f| f.path.as_str())
        .collect();
    assert_eq!(
        shipped,
        [
            "d2rloader/config/d2rloader.toml",
            "d2rloader/config/filter.json"
        ]
    );

    // kind \t size \t sha256 \t path; `#` lines are comments, the first names the archive.
    let listing = String::from_utf8(fixture("real/pd2r-0.1.0-alpha/archive-listing.tsv")).unwrap();
    let header = format!(
        "# PD2Resurrected-0.1.0-alpha.7z\t{}\t{}",
        m.archive.size, m.archive.sha256
    );
    assert_eq!(listing.lines().next(), Some(header.as_str()));
    assert!(header.ends_with("b6e2bfbd2ff4d451727109d12f89d4fc262dbff89e0c35b54aeff5c602d2c152"));
    let rows: Vec<Vec<&str>> = listing
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.split('\t').collect())
        .collect();
    let entries: Vec<(&str, Option<u64>)> = rows
        .iter()
        .map(|r| (r[3], (r[0] == "F").then(|| r[1].parse().unwrap())))
        .collect();
    assert_eq!(m.archive_mismatches(&entries), Vec::<String>::new());
    for r in rows.iter().filter(|r| r[0] == "F") {
        let rel = path(r[3].strip_prefix("PD2Resurrected/").unwrap());
        let f = m.inventory.files().iter().find(|f| f.path == rel).unwrap();
        assert_eq!(f.sha256.to_string(), r[2], "{}", r[3]);
    }

    // A missing, extra, misplaced or resized entry is reported.
    let mut bad = entries.clone();
    let last = bad.pop().unwrap();
    bad.push(("PD2Resurrected/extra.dll", Some(1)));
    bad.push(("modinfo.json", Some(1)));
    let resized = bad.iter().position(|e| e.1.is_some()).unwrap();
    bad[resized].1 = Some(1);
    let got = m.archive_mismatches(&bad);
    assert_eq!(got.len(), 4, "{got:?}");
    assert!(got.contains(&format!("{} is missing from the archive", last.0)));
}

/// The valid manifest with `release_notes_header_image` set to `url`.
fn with_header_image(url: &str) -> Result<ReleaseManifest, CatalogError> {
    let mut v: serde_json::Value =
        serde_json::from_slice(&fixture("valid/release-manifest.json")).unwrap();
    v["release_notes_header_image"] = serde_json::Value::String(url.to_owned());
    ReleaseManifest::parse(&serde_json::to_vec(&v).unwrap())
}

/// #179: the header image is optional, so older manifests still parse without it.
#[test]
fn release_manifest_without_header_image_still_parses() {
    assert_eq!(valid_manifest().release_notes_header_image, None);
    let m =
        ReleaseManifest::parse(&fixture("real/pd2r-0.1.0-alpha/release-manifest.json")).unwrap();
    assert_eq!(m.release_notes_header_image, None);
}

#[test]
fn release_manifest_accepts_a_header_image_on_github_hosts() {
    for url in [
        "https://github.com/synpoox/pd2r/releases/download/v0.4.0/header.png",
        "https://objects.githubusercontent.com/github-production-release-asset/1/header.png",
        "https://GitHub.com/synpoox/pd2r/releases/download/v0.4.0/header.png",
    ] {
        let m = with_header_image(url).unwrap_or_else(|e| panic!("{url}: {e}"));
        assert_eq!(m.release_notes_header_image.as_deref(), Some(url));
    }
}

#[test]
fn release_manifest_rejects_a_header_image_off_https_or_the_allowed_hosts() {
    let f = "release_notes_header_image";
    let host = invalid(
        f,
        "host must be github.com or objects.githubusercontent.com",
    );
    for (url, want) in [
        ("http://github.com/a.png", not_https(f)),
        ("header.png", not_https(f)),
        ("https://github.com@evil.example/a.png", not_https(f)),
        ("https://github.com:8443/a.png", not_https(f)),
        ("https://evil.example/a.png", host.clone()),
        ("https://raw.githubusercontent.com/a.png", host.clone()),
        ("https://github.com.evil.example/a.png", host.clone()),
        (
            "https://github.com/../a.png",
            invalid(f, "URL path must not contain dot segments"),
        ),
    ] {
        assert_eq!(with_header_image(url).unwrap_err(), want, "{url}");
    }
}
