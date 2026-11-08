//! Clause 7: phase B, writes to a D2R copy, with the owner's written approval
//! (#61, 2026-11-08: only `C:\Users\micah\source\repos\d2r-game`, an exact
//! clone of the owner's copy).
//!
//! The guard now accepts a validated `<D2R>\mods\<folder>` with no links on the
//! way (`contract/guard.rs`); the sentinel still marks test installations.
//! [`phase_b`] is every step of the owner checklist's phase B that needs no one
//! at the keyboard, as commands, on a D2R folder **without** a sentinel:
//!
//! 1. Adopt: only `mods\PD2Resurrected.resurrected-lock` is new.
//! 3. Edit the mod-local `d2rloader.toml`, delete one managed file, Repair: the
//!    file is restored, the edit kept, nothing else changes (by hash).
//! 4. A Repair "killed" during activation (injected crash at
//!    `StagedMovedToLive`), then a new launcher: recovery is required before
//!    Play, and afterwards the installation is complete.
//! 5. Uninstall: only managed files go; the edited config, unknown files, other
//!    mods and everything outside the product folder stay.
//! 6. Install fresh: downloads, verifies and extracts with the bundled 7-Zip.
//!
//! Play (step 2), Set up D2RLoader (7) and other machines (8) need the owner;
//! see `docs/gates/slice3-gate-report.md`. Its game host never starts anything.
//!
//! It runs on a sentinel-less copy of the real-layout fixture in every test run,
//! and on a real copy only when `SLICE3_GATE_D2R_PHASE_B` names one
//! ([`phase_b_on_the_owner_d2r_copy`]); that run downloads the published
//! release's real archive.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use resurrected_launcher_lib::app::Release;
use resurrected_launcher_lib::catalog::ReleaseManifest;
use resurrected_launcher_lib::contract::{Readiness, DISPOSABLE_SENTINEL};
use resurrected_launcher_lib::install::preflight::SystemHost;
use serde_json::json;

use crate::phase_a::meta_tree;
use crate::restart::crash_mid_repair_removing;
use crate::rig::*;
use crate::support;

/// The mod-local loader config the player edits.
const TOML: &str = "d2rloader/config/d2rloader.toml";
/// The managed file phase B deletes; in the fixture and the real release.
const MANAGED: &str = "PD2Resurrected.mpq/data/global/excel/armor.txt";
const LOCK: &str = "mods/PD2Resurrected.resurrected-lock";
const PRODUCT_DIR: &str = "mods/PD2Resurrected/";

/// Every entry of the D2R folder: under `mods`, size and SHA-256 of each file;
/// elsewhere (the game itself, tens of GB), its metadata. Content hashes of
/// the whole real copy are taken outside the test, before and after.
type Snapshot = BTreeMap<String, String>;

fn snapshot(d2r: &Path) -> Snapshot {
    let mut out = Snapshot::new();
    for (k, v) in tree(&d2r.join("mods")) {
        let v = match v {
            Some((size, hash, _)) => format!("{size} {hash}"),
            None => "dir".into(),
        };
        out.insert(format!("mods/{k}"), v);
    }
    for (k, v) in meta_tree(d2r) {
        if k != "mods" && !k.starts_with("mods/") {
            out.insert(k, format!("{v:?}"));
        }
    }
    out
}

/// `+new`, `-removed` and `~changed` entries from `a` to `b`.
fn changes(a: &Snapshot, b: &Snapshot) -> Vec<String> {
    let mut out: Vec<String> = a
        .iter()
        .filter_map(|(k, v)| match b.get(k) {
            None => Some(format!("-{k}")),
            Some(w) if w != v => Some(format!("~{k}")),
            _ => None,
        })
        .chain(
            b.keys()
                .filter(|k| !a.contains_key(*k))
                .map(|k| format!("+{k}")),
        )
        .collect();
    out.sort();
    out
}

fn sha(path: &Path) -> String {
    support::sha256(&fs::read(path).unwrap()).to_string()
}

/// What phase B saw, for the PR's evidence table.
#[derive(Debug)]
#[allow(dead_code)] // Read through Debug: printed as evidence.
pub struct Findings {
    pub adopt: Vec<String>,
    pub repair: Vec<String>,
    pub recovery: Vec<String>,
    pub uninstall_preserved: Vec<String>,
    pub uninstall_removed: usize,
    pub install: Vec<String>,
    pub entries: usize,
}

/// The owner checklist's phase B steps 1 and 3-6 against `d2r`. `launcher`
/// starts a launcher (same State every time) with the release published.
fn phase_b(d2r: &Path, launcher: &dyn Fn() -> Rig) -> Findings {
    let rig = launcher();
    let release = rig.releases.0.lock().unwrap().last().unwrap().clone();
    let root = d2r.join(release.manifest.install_root.as_str());
    assert_eq!(root, rig.root());
    assert!(
        !root.join(DISPOSABLE_SENTINEL).exists() && !d2r.join(DISPOSABLE_SENTINEL).exists(),
        "phase B tests the guard's mod-folder rule; a sentinel would bypass it"
    );
    let expected: BTreeMap<String, String> = release
        .manifest
        .inventory
        .files()
        .iter()
        .map(|f| (f.path.as_str().to_owned(), f.sha256.to_string()))
        .collect();
    assert!(expected.contains_key(MANAGED) && expected.contains_key(TOML));
    let file = |rel: &str| rel.split('/').fold(root.clone(), |p, c| p.join(c));
    let is_complete = || {
        for (rel, hash) in &expected {
            if rel != TOML {
                assert_eq!(&sha(&file(rel)), hash, "{rel}");
            }
        }
    };
    let s0 = snapshot(d2r);
    rig.start_up();
    assert_eq!(rig.state().readiness, Readiness::NotInstalled);

    // 1. Adopt.
    let insp = rig.inspect("adopt", json!({})).unwrap();
    assert!(insp.conflicts.is_empty(), "{:?}", insp.conflicts);
    let outcome = rig.run("adopt", json!({}));
    assert_eq!(outcome["status"], "succeeded", "adopt: {outcome}");
    assert_eq!(rig.state().readiness, Readiness::Ready);
    let s1 = snapshot(d2r);
    let adopt = changes(&s0, &s1);
    assert_eq!(
        adopt,
        [format!("+{LOCK}")],
        "adopt changed more than the lock"
    );

    // 3. Edit the mod-local TOML (keeping the required setting), delete a
    // managed file, Repair.
    let mut edited = fs::read(file(TOML)).unwrap();
    edited.extend_from_slice(b"\n# edited by the slice 3 gate (#61)\n");
    fs::write(file(TOML), &edited).unwrap();
    fs::remove_file(file(MANAGED)).unwrap();
    let insp = rig.inspect("repair", json!({})).unwrap();
    assert!(insp.conflicts.is_empty(), "{:?}", insp.conflicts);
    assert!(insp.preserved.iter().any(|p| p.as_str() == TOML));
    let outcome = rig.run("repair", json!({}));
    assert_eq!(outcome["status"], "succeeded", "repair: {outcome}");
    assert_eq!(sha(&file(MANAGED)), expected[MANAGED]);
    assert_eq!(fs::read(file(TOML)).unwrap(), edited);
    let s3 = snapshot(d2r);
    let repair = changes(&s1, &s3);
    assert_eq!(
        repair,
        [format!("~{PRODUCT_DIR}{TOML}")],
        "repair changed more than the edit"
    );

    // 4. A Repair killed during activation; a new launcher recovers before Play.
    crash_mid_repair_removing(&rig, MANAGED);
    let journal = d2r
        .join("mods")
        .join("PD2Resurrected.resurrected-journal.json");
    assert!(journal.exists());
    drop(rig);
    let rig = launcher();
    assert_eq!(rig.state().readiness, Readiness::RecoveryRequired);
    assert_eq!(code(rig.play().unwrap_err()), "recovery_required");
    rig.start_up();
    assert_eq!(rig.state().readiness, Readiness::Ready);
    assert!(!journal.exists());
    // Complete, old or new: the new installation is the repaired one; the old
    // one is the installation before the repair, missing the deleted file.
    let s4 = snapshot(d2r);
    let mut recovery = changes(&s3, &s4);
    if recovery.is_empty() {
        is_complete();
        recovery.push("rolled forward: the repaired installation".into());
    } else {
        assert_eq!(
            recovery,
            [format!("-{PRODUCT_DIR}{MANAGED}")],
            "recovery left neither the old nor the new installation"
        );
        recovery.push("rolled back: the installation before the repair".into());
        let outcome = rig.run("repair", json!({}));
        assert_eq!(outcome["status"], "succeeded", "repair: {outcome}");
        is_complete();
        assert!(changes(&s3, &snapshot(d2r)).is_empty());
    }

    // 5. Uninstall.
    let insp = rig.inspect("uninstall", json!({})).unwrap();
    let uninstall_preserved: Vec<String> = insp
        .preserved
        .iter()
        .map(|p| p.as_str().to_owned())
        .collect();
    assert!(uninstall_preserved.iter().any(|p| p == TOML));
    let outcome = rig.run("uninstall", json!({}));
    assert_eq!(outcome["status"], "succeeded", "uninstall: {outcome}");
    assert_eq!(rig.state().readiness, Readiness::NotInstalled);
    assert_eq!(fs::read(file(TOML)).unwrap(), edited);
    for rel in expected.keys() {
        let kept = uninstall_preserved.contains(rel);
        assert_eq!(file(rel).exists(), kept, "{rel}");
    }
    let s5 = snapshot(d2r);
    let removed = changes(&s3, &s5);
    for c in &removed {
        let rel = c[1..]
            .strip_prefix(PRODUCT_DIR)
            .unwrap_or_else(|| panic!("uninstall changed {c}, outside the product folder"));
        let rel = rel.trim_end_matches('/');
        let managed_dir = expected.keys().any(|e| e.starts_with(&format!("{rel}/")));
        assert!(
            c.starts_with('-') && (expected.contains_key(rel) || managed_dir),
            "uninstall touched {c}, which is not a managed file or folder"
        );
    }

    // 6. Install fresh: download, verify, extract with the bundled 7-Zip.
    let outcome = rig.run("install", json!({}));
    assert_eq!(outcome["status"], "succeeded", "install: {outcome}");
    assert_eq!(rig.state().readiness, Readiness::Ready);
    is_complete();
    assert_eq!(fs::read(file(TOML)).unwrap(), edited);
    let s6 = snapshot(d2r);
    let install = changes(&s3, &s6);
    assert!(
        install.is_empty(),
        "install differs from before: {install:?}"
    );
    assert!(rig.host.spawns().is_empty(), "phase B started something");

    Findings {
        adopt,
        repair,
        recovery,
        uninstall_preserved,
        uninstall_removed: removed.len(),
        install,
        entries: s0.len(),
    }
}

/// Phase B rehearsed on a copy of the real layout with no sentinel, like a real
/// installation, with an unknown plugin and another mod beside it.
#[test]
fn phase_b_writes_only_the_mod_folder_of_a_real_layout_copy() {
    let dirs = Dirs::new();
    let release = real_layout(REAL_1);
    let root = dirs.game.installed(&release);
    fs::remove_file(root.join(DISPOSABLE_SENTINEL)).unwrap();
    let plugin = support::add_unknown_plugin(&root);
    let other = dirs.game.d2r().join("mods").join("Reimagined");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("Reimagined.mpq"), b"another mod").unwrap();

    let launcher = || {
        let rig = Rig::attach(dirs.game.d2r(), dirs.state.path(), Box::new(SystemHost));
        rig.publish(&release);
        rig
    };
    let found = phase_b(dirs.game.d2r(), &launcher);
    assert_eq!(
        fs::read(root.join(support::UNKNOWN_PLUGIN)).unwrap(),
        plugin
    );
    assert_eq!(found.adopt, [format!("+{LOCK}")]);
}

/// Phase B on a real D2R copy: `SLICE3_GATE_D2R_PHASE_B=<D2R folder>`. Only
/// with the owner's written approval for that folder, and on a backed-up
/// clone: it writes `mods\PD2Resurrected` and leaves the TOML edit behind.
/// Uses the published PD2R 0.1.0-alpha manifest (`fixtures/catalog/real`) and
/// downloads its real archive. Prints its findings.
#[test]
fn phase_b_on_the_owner_d2r_copy() {
    let Some(d2r) = std::env::var_os("SLICE3_GATE_D2R_PHASE_B").map(PathBuf::from) else {
        skip("set SLICE3_GATE_D2R_PHASE_B to run phase B on an approved D2R copy");
        return;
    };
    let bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/catalog/real/pd2r-0.1.0-alpha/release-manifest.json"),
    )
    .unwrap();
    let release = Release {
        manifest: ReleaseManifest::parse(&bytes).unwrap(),
        sha256: support::sha256(&bytes),
    };
    let state = tempfile::tempdir().unwrap();
    let launcher = || {
        let rig = Rig::attach_online(&d2r, state.path(), GateHost::read_only());
        rig.releases.0.lock().unwrap().push(release.clone());
        rig
    };
    let found = phase_b(&d2r, &launcher);
    eprintln!("PHASE B on {}:\n{found:#?}", d2r.display());
}
