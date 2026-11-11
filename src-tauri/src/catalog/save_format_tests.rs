//! #213: the save-format rule of `ReleaseManifest::breaks_saves_from` and the
//! `saves.save_format_version` validation.

use std::fs;
use std::path::PathBuf;

use super::*;

fn fixture_bytes() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/catalog/valid/release-manifest.json");
    fs::read(path).unwrap()
}

/// A release of the fixture product: `id` released on `date`, with the given format
/// version and break flag.
fn rel(id: &str, date: &str, format: Option<u32>, breaking: bool) -> ReleaseManifest {
    let mut m = ReleaseManifest::parse(&fixture_bytes()).unwrap();
    m.release_id = id.into();
    m.release_date = date.into();
    m.saves.save_format_version = format;
    m.saves.incompatibility_notice = breaking.then(|| "Saves break.".to_owned());
    m
}

fn parse_with(format: Option<serde_json::Value>) -> Result<ReleaseManifest, CatalogError> {
    let mut json: serde_json::Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    if let Some(v) = format {
        json["saves"]["save_format_version"] = v;
    }
    ReleaseManifest::parse(json.to_string().as_bytes())
}

#[test]
fn same_format_is_compatible() {
    let from = rel("a", "2026-01-01", Some(2), false);
    let to = rel("b", "2026-02-01", Some(2), false);
    assert!(!to.breaks_saves_from(Some(&from), &[]));
}

#[test]
fn higher_format_breaks() {
    let from = rel("a", "2026-01-01", Some(1), false);
    let to = rel("b", "2026-02-01", Some(2), false);
    assert!(to.breaks_saves_from(Some(&from), &[]));
}

#[test]
fn lower_format_breaks_as_a_downgrade() {
    let from = rel("a", "2026-01-01", Some(2), false);
    let to = rel("b", "2026-02-01", Some(1), false);
    assert!(to.breaks_saves_from(Some(&from), &[]));
}

#[test]
fn breaking_target_breaks_without_any_versions() {
    let from = rel("a", "2026-01-01", None, false);
    let to = rel("b", "2026-02-01", None, true);
    assert!(to.breaks_saves_from(Some(&from), &[]));
    assert!(to.breaks_saves_from(None, &[]));
}

#[test]
fn missing_version_on_either_side_is_compatible() {
    let none = rel("a", "2026-01-01", None, false);
    let two = rel("b", "2026-02-01", Some(2), false);
    assert!(!two.breaks_saves_from(Some(&none), &[]));
    assert!(!none.breaks_saves_from(Some(&two), &[]));
    assert!(!two.breaks_saves_from(None, &[]));
}

#[test]
fn skipped_breaking_release_is_caught_by_flag_or_format() {
    let from = rel("a", "2026-01-01", None, false);
    let to = rel("c", "2026-03-01", None, false);
    let skipped_flagged = rel("b", "2026-02-01", None, true);
    assert!(to.breaks_saves_from(Some(&from), &[skipped_flagged]));

    let from = rel("a", "2026-01-01", Some(1), false);
    let to = rel("c", "2026-03-01", Some(1), false);
    let skipped_format = rel("b", "2026-02-01", Some(2), false);
    assert!(to.breaks_saves_from(Some(&from), &[skipped_format]));
}

#[test]
fn releases_outside_the_update_range_are_ignored() {
    let from = rel("b", "2026-02-01", Some(1), false);
    let to = rel("c", "2026-03-01", Some(1), false);
    let older_breaking = rel("a", "2026-01-01", Some(2), true);
    let newer_breaking = rel("d", "2026-04-01", Some(2), true);
    assert!(!to.breaks_saves_from(Some(&from), &[older_breaking, newer_breaking]));
}

#[test]
fn zero_format_version_is_rejected() {
    let err = parse_with(Some(0.into())).unwrap_err();
    assert!(matches!(
        err,
        CatalogError::InvalidField { ref field, .. } if field == "saves.save_format_version"
    ));
}

#[test]
fn format_version_is_optional_and_parsed() {
    assert_eq!(parse_with(None).unwrap().saves.save_format_version, None);
    assert_eq!(
        parse_with(Some(3.into()))
            .unwrap()
            .saves
            .save_format_version,
        Some(3)
    );
}
