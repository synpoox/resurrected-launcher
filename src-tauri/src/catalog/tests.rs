//! Fixture tests: one valid feed and release manifest, plus one malformed file per
//! rejection class in `fixtures/catalog/invalid/`, each asserting its exact error.

use std::fs;
use std::path::PathBuf;

use crate::contract::{
    ErrorCode, InventoryError, LauncherError, PathError, ProductId, RelativePath,
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

fn manifest_rejections() -> Vec<(&'static str, CatalogError)> {
    use CatalogError::*;
    vec![
        ("zero-hash", placeholder("archive.sha256")),
        ("missing-hash", placeholder("files[1].sha256")),
        ("missing-size", placeholder("archive.size")),
        ("zero-size", placeholder("loader.size")),
        ("empty-file-placeholder-hash", placeholder("files[2].size")),
        ("not-https-archive", not_https("archive.url")),
        ("not-https-loader", not_https("loader.url")),
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
            "unofficial-loader",
            LoaderSourceNotOfficial {
                url: field("https://example.invalid/D2RLoaderInstaller.exe"),
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
    assert!(!m.loader.signed);
    assert_eq!(
        m.compatibility.game_builds[0].platform,
        Platform::WindowsX64
    );
    assert_eq!(m.compatibility.required_settings[0].key, "default_mod");
    assert_eq!(m.saves.folder, path("PD2R"));
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
