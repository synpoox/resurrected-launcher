//! Clause 2: save and config preservation on the real-layout fixture
//! (`mods/PD2Resurrected`, shipped default `d2rloader/config/d2rloader.toml`,
//! required TOML setting `d2rcore.save_format.mode = "extended"`).

use std::fs;
use std::path::{Path, PathBuf};

use resurrected_launcher_lib::app::ConflictView;
use serde_json::json;

use crate::rig::*;
use crate::support::{self, add_unknown_plugin};

const TOML: &str = "d2rloader/config/d2rloader.toml";
const MANAGED: &str = "PD2Resurrected.mpq/data/global/excel/armor.txt";

fn installed_record(rig: &Rig) -> String {
    rig.state().installed.unwrap().release_id
}

/// A player edit that keeps the required setting.
const EDITED_TOML: &str = "[d2rcore.save_format]\nmode = \"extended\"\n[d2rcore.items]\nitem_stat_ranges = false # the player's choice\n";

#[test]
fn real_layout_edited_d2rloader_toml_is_kept_by_adopt_repair_and_update() {
    let rig = Rig::new();
    let r1 = real_layout(REAL_1);
    let root = rig.dirs.as_ref().unwrap().game.installed(&r1);
    fs::write(root.join(TOML), EDITED_TOML).unwrap();
    let plugin = add_unknown_plugin(&root);
    rig.publish(&r1);
    rig.start_up();

    let adopt = rig.inspect("adopt", json!({})).unwrap();
    assert_eq!(
        adopt
            .preserved
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>(),
        [TOML]
    );
    assert!(adopt.conflicts.is_empty(), "{:?}", adopt.conflicts);
    assert_eq!(rig.run("adopt", json!({}))["status"], "succeeded");

    fs::remove_file(root.join(MANAGED)).unwrap();
    let repair = rig.inspect("repair", json!({})).unwrap();
    assert_eq!(repair.files_added, 1);
    assert_eq!(rig.run("repair", json!({}))["status"], "succeeded");
    assert_eq!(fs::read(root.join(MANAGED)).unwrap(), r1.payload[MANAGED]);

    rig.publish(&real_layout(REAL_1B));
    let update = rig.inspect("update", json!({})).unwrap();
    assert!(update.preserved.iter().any(|p| p.as_str() == TOML));
    assert!(update.conflicts.is_empty(), "{:?}", update.conflicts);
    assert_eq!(rig.run("update", json!({}))["status"], "succeeded");
    assert_eq!(installed_record(&rig), REAL_1B);

    assert_eq!(fs::read_to_string(root.join(TOML)).unwrap(), EDITED_TOML);
    assert_eq!(
        fs::read(root.join(support::UNKNOWN_PLUGIN)).unwrap(),
        plugin
    );
}

#[test]
fn real_layout_required_setting_conflict_pauses_the_update() {
    let rig = Rig::new();
    rig.adopted();
    let root = rig.root();
    let conflicting = "[d2rcore.save_format]\nmode = \"legacy\"\n";
    fs::write(root.join(TOML), conflicting).unwrap();
    rig.publish(&real_layout(REAL_1B));
    // The operation lock beside the product root is opened (not changed) by
    // every operation; everything else must stay as it is.
    let snapshot = || {
        let mut t = tree(&rig.d2r);
        t.remove("mods/PD2Resurrected.resurrected-lock");
        t
    };
    let before = snapshot();

    let insp = rig.inspect("update", json!({})).unwrap();
    assert_eq!(
        insp.conflicts,
        [ConflictView::Setting {
            file: resurrected_launcher_lib::contract::RelativePath::new(TOML).unwrap(),
            key: "d2rcore.save_format.mode".into(),
            required: "extended".into(),
            found: Some("legacy".into()),
        }]
    );
    rig.start("update", json!({}), &insp.token).unwrap();
    let outcome = rig.outcome();
    assert_eq!(outcome["status"], "failed", "{outcome}");
    assert_eq!(outcome["error"]["code"], "mandatory_setting_conflict");
    // Paused, not overwritten: nothing in the D2R folder changed.
    assert_eq!(before, snapshot());
    assert_eq!(installed_record(&rig), REAL_1);
    assert_eq!(fs::read_to_string(root.join(TOML)).unwrap(), conflicting);

    // Once the player resolves it, the same update goes ahead and keeps the edit.
    fs::write(root.join(TOML), EDITED_TOML).unwrap();
    assert_eq!(rig.run("update", json!({}))["status"], "succeeded");
    assert_eq!(fs::read_to_string(root.join(TOML)).unwrap(), EDITED_TOML);
}

/// D2R's per-user mod save folder, laid out as on the owner's machine, with
/// every save marked read-only: a write would fail its operation, and any new,
/// changed or removed entry shows in the snapshot.
fn saves_folder(base: &Path) -> PathBuf {
    let saves = base
        .join("Saved Games")
        .join("Diablo II Resurrected")
        .join("mods")
        .join("PD2Resurrected");
    fs::create_dir_all(&saves).unwrap();
    for (name, bytes) in [
        ("Syn.d2s", &b"character save"[..]),
        ("Syn.key", b"key bindings"),
        ("SharedStashSoftCoreV2.d2i", b"shared stash"),
        ("Settings.json", b"{}"),
    ] {
        let p = saves.join(name);
        fs::write(&p, bytes).unwrap();
        let mut perm = fs::metadata(&p).unwrap().permissions();
        perm.set_readonly(true);
        fs::set_permissions(&p, perm).unwrap();
    }
    saves
}

#[test]
fn real_layout_saves_folder_is_never_opened_for_write() {
    let profile = tempfile::tempdir().unwrap();
    let saves = saves_folder(profile.path());
    let rig = Rig::new();
    // Saves the game keeps beside the D2R folder's other mods are not the
    // product's either.
    let other_mod = rig.d2r.join("mods").join("Reimagined");
    fs::create_dir_all(&other_mod).unwrap();
    fs::write(other_mod.join("Reimagined.mpq"), b"another mod").unwrap();
    let before = (tree(&saves), tree(&other_mod));

    let r1 = real_layout(REAL_1);
    rig.dirs.as_ref().unwrap().game.product_root(&r1);
    rig.publish(&r1);
    rig.start_up();
    let succeeded = |kind: &str, extra| {
        let outcome = rig.run(kind, extra);
        assert_eq!(outcome["status"], "succeeded", "{kind}: {outcome}");
        assert_eq!(before, (tree(&saves), tree(&other_mod)), "after {kind}");
    };
    succeeded("install", json!({}));
    fs::remove_file(rig.root().join(MANAGED)).unwrap();
    succeeded("repair", json!({}));
    rig.publish(&real_layout(REAL_1B));
    succeeded("update", json!({}));
    succeeded("clean_install", json!({ "consent_clean_install": true }));
    succeeded("uninstall", json!({}));

    for entry in fs::read_dir(&saves).unwrap() {
        let path = entry.unwrap().path();
        let mut perm = fs::metadata(&path).unwrap().permissions();
        assert!(
            perm.readonly(),
            "{} lost its read-only mark",
            path.display()
        );
        // Let the temp dir be removed.
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        fs::set_permissions(&path, perm).unwrap();
    }
}
