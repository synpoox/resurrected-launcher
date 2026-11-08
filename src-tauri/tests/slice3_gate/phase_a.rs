//! Clause 6: phase A, read-only.
//!
//! [`phase_a`] is the owner checklist's phase A as commands: detect, browse to a
//! folder without `D2R.exe` and then to the copy, start up, assess readiness and
//! inspect adoption. The D2R folder must not change at all. It never calls Play
//! or starts an operation, and its game host cannot start anything.
//!
//! Before phase B the [`WriteGuard`] refused every write to a real copy, and
//! phase A also tried every write operation and saw each one refused with
//! `product_root_not_disposable`. That run on the owner's copy is recorded in
//! `docs/gates/slice3-gate-report.md` (at the commit before the guard change).
//! Since phase B the guard accepts a real copy's `mods\<folder>`, so those
//! attempts would write; they are gone, and [`super::phase_b`] covers writes.
//!
//! It runs on a fixture copy of the real layout without the sentinel in every
//! test run, and against a real D2R copy only when `SLICE3_GATE_D2R` names one
//! ([`phase_a_on_the_owner_d2r_copy_is_read_only`]).
//!
//! [`WriteGuard`]: resurrected_launcher_lib::contract::WriteGuard

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use resurrected_launcher_lib::app::Release;
use resurrected_launcher_lib::catalog::{GameBuild, Platform, ReleaseManifest};
use resurrected_launcher_lib::contract::{
    GameLocation, GameStore, ProductId, Readiness, RelativePath, DISPOSABLE_SENTINEL,
};
use resurrected_launcher_lib::install::preflight::SystemHost;
use resurrected_launcher_lib::state::InstalledRelease;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::rig::*;
use crate::support;

/// SHA-256 of `src/contract/guard.rs` (LF line endings) as changed by phase B
/// of the slice 3 gate (#61), with the owner's written approval on #61. Any
/// later change to which folders the launcher may write must move it, in a PR
/// that says why.
const GUARD_SHA256: &str = "120af0e38ba6432d16fb2f63b9f7bb23d515c545b71c8fd7b2e49e692479679d";

#[test]
fn write_guard_is_the_phase_b_guard() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/contract/guard.rs");
    let text = fs::read_to_string(path).unwrap().replace("\r\n", "\n");
    let hash: String = Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(
        hash, GUARD_SHA256,
        "WriteGuard changed. Changing which folders the launcher writes needs the owner's approval (#61)."
    );
    // Real copies only in `<D2R>\mods\<folder>`; the sentinel for tests.
    assert!(text.contains("fn is_mod_folder"));
    assert!(text.contains("fn has_sentinel"));
    assert_eq!(DISPOSABLE_SENTINEL, ".resurrected-disposable");
}

/// Metadata of every entry under `dir`: (is_dir, size, modified, read-only).
/// Content hashes of the real copy are taken outside the test, before and after.
pub(crate) type Meta = BTreeMap<String, (bool, u64, SystemTime, bool)>;

pub(crate) fn meta_tree(dir: &Path) -> Meta {
    fn go(dir: &Path, prefix: &str, out: &mut Meta) {
        for e in fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            let rel = format!("{prefix}{}", e.file_name().to_string_lossy());
            let m = fs::symlink_metadata(e.path()).unwrap();
            let real_dir = m.is_dir() && !m.file_type().is_symlink();
            out.insert(
                rel.clone(),
                (
                    real_dir,
                    m.len(),
                    m.modified().unwrap(),
                    m.permissions().readonly(),
                ),
            );
            if real_dir {
                go(&e.path(), &format!("{rel}/"), out);
            }
        }
    }
    let mut out = Meta::new();
    if dir.exists() {
        go(dir, "", &mut out);
    }
    out
}

/// What phase A saw, for the PR's evidence table.
#[derive(Debug)]
#[allow(dead_code)] // Read through Debug: printed as evidence.
pub struct Findings {
    pub detected: Vec<GameLocation>,
    pub location: GameLocation,
    pub not_installed: Readiness,
    pub adoption: String,
    pub with_record: Readiness,
    pub with_update: Readiness,
    pub entries: usize,
}

/// The owner checklist's phase A against `d2r`, with `release` as the published
/// release. `watch` lists other folders that must not change (the saves).
fn phase_a(d2r: &Path, release: Release, game: GateHost, watch: &[PathBuf]) -> Findings {
    let root = d2r.join(release.manifest.install_root.as_str());
    assert!(
        !root.join(DISPOSABLE_SENTINEL).exists() && !d2r.join(DISPOSABLE_SENTINEL).exists(),
        "a sentinel would let the guard pass; refusing to run phase A"
    );
    let snapshot = || {
        let mut all = vec![meta_tree(d2r)];
        all.extend(watch.iter().map(|w| meta_tree(w)));
        all
    };
    let before = snapshot();
    let state = tempfile::tempdir().unwrap();
    let not_d2r = tempfile::tempdir().unwrap();
    let rig = Rig::attach_with(d2r, state.path(), Box::new(SystemHost), game, false);
    rig.releases.0.lock().unwrap().push(release.clone());

    // Checklist 4: detection, then browsing.
    let detected: Vec<GameLocation> = data(rig.invoke("detect_game_locations", json!({})).unwrap());
    let canonical = fs::canonicalize(d2r).unwrap();
    assert!(
        detected
            .iter()
            .all(|l| fs::canonicalize(&l.path).ok() != Some(canonical.clone())),
        "detection listed the copy: {detected:?}"
    );
    rig.desktop.will_pick(not_d2r.path());
    let refused = rig.invoke("choose_game_location", json!({})).unwrap_err();
    assert_eq!(code(refused), "d2r_not_found");
    rig.desktop.will_pick(d2r);
    let location: Option<GameLocation> =
        data(rig.invoke("choose_game_location", json!({})).unwrap());
    let location = location.unwrap();
    assert_eq!(location.store, GameStore::Unknown);

    // Checklist 5: readiness and the adoption inspection.
    rig.start_up();
    let not_installed = rig.state().readiness;
    let adopt = rig.inspect("adopt", json!({})).unwrap();
    let adoption = format!(
        "adopt: {} added, {} replaced, preserved {:?}, conflicts {:?}",
        adopt.files_added,
        adopt.files_replaced_or_removed,
        adopt
            .preserved
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>(),
        adopt.conflicts
    );

    // Checklist 6 (writes refused) was the pre-phase-B guard; see the module docs.
    // Inspecting the other operations still writes nothing.
    for (kind, extra) in [
        ("install", json!({})),
        ("clean_install", json!({ "consent_clean_install": true })),
    ] {
        rig.inspect(kind, extra).unwrap();
    }

    // As if adopted: State (a temp dir) records the release; the copy is untouched.
    rig.launcher()
        .state
        .lock()
        .unwrap()
        .mark_installed(InstalledRelease {
            product_id: ProductId(PRODUCT.into()),
            release_id: release.manifest.release_id.clone(),
            display_version: release.manifest.display_version.clone(),
            manifest_sha256: release.sha256,
            installed_at: 0,
            verified: true,
        })
        .unwrap();
    let with_record = rig.state().readiness;
    let recovered = rig
        .invoke("recover_product", json!({ "product_id": PRODUCT }))
        .unwrap();
    assert_eq!(recovered["data"], "nothing");
    rig.inspect("repair", json!({})).unwrap();
    rig.inspect("uninstall", json!({})).unwrap();
    let mut next = release.clone();
    next.manifest.release_id = format!("{}-gate-update", release.manifest.release_id);
    rig.releases.0.lock().unwrap().push(next);
    rig.inspect("update", json!({})).unwrap();
    let with_update = rig.state().readiness;

    assert!(rig.host.spawns().is_empty(), "phase A started something");
    let after = snapshot();
    assert_eq!(before.len(), after.len());
    for (b, a) in before.iter().zip(&after) {
        let changed: Vec<_> = b
            .iter()
            .filter(|(k, v)| a.get(*k) != Some(v))
            .map(|(k, _)| k)
            .chain(a.keys().filter(|k| !b.contains_key(*k)))
            .collect();
        assert!(changed.is_empty(), "phase A changed {changed:?}");
    }
    Findings {
        detected,
        location,
        not_installed,
        adoption,
        with_record,
        with_update,
        entries: before.iter().map(BTreeMap::len).sum(),
    }
}

/// Phase A rehearsed on a copy of the real layout that has no sentinel, like a
/// real installation, with every PD2R file in place and the player's edits.
#[test]
fn phase_a_reads_a_real_layout_copy_and_changes_nothing() {
    let dirs = Dirs::new();
    let release = real_layout(REAL_1);
    let root = dirs.game.installed(&release);
    fs::remove_file(root.join(DISPOSABLE_SENTINEL)).unwrap();
    support::add_unknown_plugin(&root);
    let saves = tempfile::tempdir().unwrap();
    fs::write(saves.path().join("Syn.d2s"), b"character").unwrap();

    let mut manifest = release.manifest.clone();
    manifest.compatibility.game_builds = vec![GameBuild {
        build: D2R_BUILD.into(),
        platform: Platform::WindowsX64,
        executable: RelativePath::new("D2R.exe").unwrap(),
        size: D2R_BYTES.len() as u64,
        sha256: support::sha256(D2R_BYTES),
    }];
    let published = Release {
        manifest,
        sha256: support::sha256(&release.manifest_json),
    };
    // Fixture versions; phase_a itself asserts that nothing was started.
    let found = phase_a(
        dirs.game.d2r(),
        published,
        GateHost::default(),
        &[saves.path().to_owned()],
    );
    assert_eq!(found.location.build, D2R_BUILD);
    assert_eq!(found.not_installed, Readiness::NotInstalled);
    assert_eq!(found.with_record, Readiness::Ready);
    assert_eq!(found.with_update, Readiness::UpdateAvailable);
}

/// Phase A on a real D2R copy: `SLICE3_GATE_D2R=<D2R folder>`, optionally
/// `SLICE3_GATE_SAVES=<save folder>` (default: the PD2R folder under
/// `%USERPROFILE%\Saved Games\Diablo II Resurrected\mods`). Uses the published
/// PD2R 0.1.0-alpha manifest (`fixtures/catalog/real`) and the real machine,
/// except that nothing can be started. Prints its findings.
#[test]
fn phase_a_on_the_owner_d2r_copy_is_read_only() {
    let Some(d2r) = std::env::var_os("SLICE3_GATE_D2R").map(PathBuf::from) else {
        skip("set SLICE3_GATE_D2R to run phase A on a real D2R copy");
        return;
    };
    let saves = std::env::var_os("SLICE3_GATE_SAVES")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE").map(|p| {
                Path::new(&p).join(r"Saved Games\Diablo II Resurrected\mods\PD2Resurrected")
            })
        })
        .into_iter()
        .filter(|p| p.is_dir())
        .collect::<Vec<_>>();
    let bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/catalog/real/pd2r-0.1.0-alpha/release-manifest.json"),
    )
    .unwrap();
    let release = Release {
        manifest: ReleaseManifest::parse(&bytes).unwrap(),
        sha256: support::sha256(&bytes),
    };
    let found = phase_a(&d2r, release, GateHost::read_only(), &saves);
    eprintln!(
        "PHASE A on {} (watching {saves:?}):\n{found:#?}",
        d2r.display()
    );
}
