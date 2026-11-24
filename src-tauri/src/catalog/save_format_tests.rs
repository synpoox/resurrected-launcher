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

/// #290: `to` declares `format` and lists `same` as the unnumbered releases it reads.
fn numbered(id: &str, date: &str, format: u32, same: Option<&[&str]>) -> ReleaseManifest {
    let mut m = rel(id, date, Some(format), false);
    m.saves.unnumbered_compatible_releases =
        same.map(|s| s.iter().map(|&id| id.to_owned()).collect());
    m
}

#[test]
fn unnumbered_install_before_a_break_is_caught_without_history() {
    // 0.2.5 broke saves; 0.2.4, 0.2.5 and 0.2.6 declare no format; 0.2.7 is the first to.
    let to = numbered("p-0.2.7", "2026-03-01", 1, Some(&["p-0.2.5", "p-0.2.6"]));
    let before = rel("p-0.2.4", "2026-01-01", None, false);
    assert!(to.breaks_saves_from(Some(&before), &[]));
    for after in [
        rel("p-0.2.5", "2026-02-01", None, true),
        rel("p-0.2.6", "2026-02-01", None, false),
    ] {
        assert!(
            !to.breaks_saves_from(Some(&after), &[]),
            "{}",
            after.release_id
        );
    }
    assert!(!to.breaks_saves_from(None, &[]));
}

#[test]
fn unnumbered_install_counts_as_compatible_without_the_list() {
    let to = numbered("p-0.2.7", "2026-03-01", 1, None);
    assert!(!to.breaks_saves_from(Some(&rel("p-0.2.4", "2026-01-01", None, false)), &[]));
}

#[test]
fn numbered_install_ignores_the_list() {
    let to = numbered("p-0.2.8", "2026-04-01", 1, Some(&[]));
    assert!(!to.breaks_saves_from(Some(&numbered("p-0.2.7", "2026-03-01", 1, Some(&[]))), &[]));
}

#[test]
fn unnumbered_list_needs_a_format_version_and_release_ids() {
    let mut json: serde_json::Value = serde_json::from_slice(&fixture_bytes()).unwrap();
    json["saves"]["unnumbered_compatible_releases"] = serde_json::json!(["p-0.2.5"]);
    let err = ReleaseManifest::parse(json.to_string().as_bytes()).unwrap_err();
    assert!(matches!(
        err,
        CatalogError::InvalidField { ref field, .. } if field == "saves.unnumbered_compatible_releases"
    ));
    json["saves"]["save_format_version"] = 1.into();
    let m = ReleaseManifest::parse(json.to_string().as_bytes()).unwrap();
    assert_eq!(
        m.saves.unnumbered_compatible_releases,
        Some(vec!["p-0.2.5".to_owned()])
    );
    json["saves"]["unnumbered_compatible_releases"] = serde_json::json!(["Not An Id"]);
    assert!(ReleaseManifest::parse(json.to_string().as_bytes()).is_err());
    assert_eq!(
        parse_with(Some(1.into()))
            .unwrap()
            .saves
            .unnumbered_compatible_releases,
        None
    );
}
