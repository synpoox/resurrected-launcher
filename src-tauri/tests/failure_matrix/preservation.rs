//! Player files through every operation: shipped defaults the player never
//! touched follow the release, edited loot filters and configs and unknown
//! (optional) plugins stay byte for byte, and labelled clean-install backups are
//! never deleted. Includes the two #50 follow-ups.

use super::*;
use resurrected_launcher_lib::contract::FailAt;

#[test]
fn update_replaces_untouched_shipped_defaults_with_the_new_release_defaults() {
    let rig = Rig::new();
    let m1 = rig.install_1();
    let root = rig.root();
    let plugin = add_unknown_plugin(&root);

    // 1 -> 2: the untouched loot filter default changes.
    let to_2 = Op::new(&rig, Update, PD2R_2, Some(m1));
    to_2.run(&rig, &NoFaults).unwrap();
    let want = with_file(release_tree(&to_2.release), UNKNOWN_PLUGIN, &plugin);
    assert_eq!(tree(&root), want);

    // 2 -> 3: the untouched config.ini default changes.
    let mut to_3 = Op::new(&rig, Update, PD2R_3, Some(to_2.manifest.clone()));
    to_3.acknowledge_save_breaking = true;
    to_3.run(&rig, &NoFaults).unwrap();
    let want = with_file(release_tree(&to_3.release), UNKNOWN_PLUGIN, &plugin);
    assert_eq!(tree(&root), want);
    assert_eq!(rig.installed_release().as_deref(), Some(PD2R_3));
}

#[test]
fn edited_loot_filter_edited_config_and_unknown_plugin_survive_update_repair_and_uninstall() {
    let rig = Rig::new();
    let m1 = rig.install_1();
    let root = rig.root();
    let filter = edit_loot_filter(&root);
    let mut config = release(PD2R_1).payload[CONFIG].clone();
    config.extend_from_slice(b"; my note\nwindowed=1\n");
    fs::write(root.join(CONFIG), &config).unwrap();
    let plugin = add_unknown_plugin(&root);
    let kept = |t: Tree| {
        let t = with_file(t, LOOT_FILTER, &filter);
        let t = with_file(t, CONFIG, &config);
        with_file(t, UNKNOWN_PLUGIN, &plugin)
    };

    let update = Op::new(&rig, Update, PD2R_2, Some(m1));
    let report = update.run(&rig, &NoFaults).unwrap();
    assert_eq!(tree(&root), kept(release_tree(&update.release)), "update");
    let preserved: Vec<_> = report.preserved.iter().map(|p| p.as_str()).collect();
    assert_eq!(preserved, [CONFIG, LOOT_FILTER]);

    let m2 = update.manifest;
    fs::write(root.join(ARMOR), b"damaged").unwrap();
    let repair = Op::new(&rig, Repair, PD2R_2, Some(m2.clone()));
    repair.run(&rig, &NoFaults).unwrap();
    assert_eq!(tree(&root), kept(release_tree(&repair.release)), "repair");

    let uninstall = Op::new(&rig, Uninstall, PD2R_2, Some(m2));
    uninstall.run(&rig, &NoFaults).unwrap();
    assert_eq!(tree(&root), kept(empty_root()), "uninstall");
    assert_eq!(rig.installed_release(), None);
}

#[test]
fn repair_restores_missing_and_damaged_managed_files_and_shipped_defaults_only() {
    let s = Scenario::new(Repair);
    s.run(&NoFaults).unwrap();
    // The scenario removed config.ini (a shipped default) and modinfo.json and
    // damaged armor.txt; the edited loot filter, plugin and save stay.
    s.assert_settled(true, "repair");
}

#[test]
fn clean_install_backup_is_retained_through_later_operations_and_recovery() {
    let s = Scenario::new(CleanInstall);
    let report = s.run(&NoFaults).unwrap();
    s.assert_settled(true, "clean install");
    let [backup] = &report.clean_install_backups[..] else {
        panic!("one labelled backup: {:?}", report.clean_install_backups);
    };
    let rig = &s.rig;
    let m2 = s.op.manifest.clone();
    let kept = |after: &str, r: &Report| {
        assert_eq!(
            &r.clean_install_backups,
            std::slice::from_ref(backup),
            "{after}"
        );
        assert!(tree(backup) == s.old, "{after}: the backup changed");
    };

    fs::write(s.root.join(ARMOR), b"damaged").unwrap();
    let repair = Op::new(rig, Repair, PD2R_2, Some(m2.clone()));
    kept("repair", &repair.run(rig, &NoFaults).unwrap());

    // An update crashes mid-activation and is rolled back by restart recovery.
    let mut to_3 = Op::new(rig, Update, PD2R_3, Some(m2.clone()));
    to_3.acknowledge_save_breaking = true;
    let crash = FailAt {
        boundary: Boundary::StagedMovedToLive,
        point: FaultPoint::AfterEntry(0),
    };
    to_3.run(rig, &crash).unwrap_err();
    assert_eq!(rig.recover(&NoFaults).unwrap(), Recovery::RolledBack);
    let names = |v: &[PathBuf]| {
        v.iter()
            .map(|p| p.file_name().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(&clean_install_backups(&s.root)),
        names(std::slice::from_ref(backup))
    );
    assert!(tree(backup) == s.old, "recovery: the backup changed");
    kept("update", &to_3.run(rig, &NoFaults).unwrap());

    let uninstall = Op::new(rig, Uninstall, PD2R_3, Some(to_3.manifest.clone()));
    kept("uninstall", &uninstall.run(rig, &NoFaults).unwrap());

    // A second clean install adds a second backup and keeps the first.
    let again = Op::new(rig, CleanInstall, PD2R_1, None);
    let report = again.run(rig, &NoFaults).unwrap();
    assert_eq!(report.clean_install_backups.len(), 2);
    assert!(report.clean_install_backups.contains(backup));
    assert!(
        tree(backup) == s.old,
        "second clean install: the backup changed"
    );
}

/// #50 item 1: the player edits an untouched shipped default after the staging
/// recheck but before it is moved to backup `B`; `CleanedUp` must not delete the
/// edit with `B`.
#[test]
fn update_keeps_a_loot_filter_edited_between_the_staging_recheck_and_activation() {
    let rig = Rig::new();
    let m1 = rig.install_1();
    let root = rig.root();
    let edit = b"// edited while the update was staging\n".to_vec();
    let player = Hook(|b, p| {
        if (b, p) == (Boundary::StagedInventoryVerified, FaultPoint::AfterStep) {
            fs::write(root.join(LOOT_FILTER), &edit).unwrap();
        }
    });

    let _ = Op::new(&rig, Update, PD2R_2, Some(m1)).run(&rig, &player);
    let _ = rig.recover(&NoFaults);

    let now = fs::read(root.join(LOOT_FILTER)).ok();
    assert_eq!(
        now.as_deref(),
        Some(&edit[..]),
        "the player's edit was lost"
    );
}

/// #50 item 2: a kept settings file that is not UTF-8 (an ANSI INI with an
/// accented comment) must not block clean install, the way out of a conflict.
#[test]
fn clean_install_proceeds_when_the_kept_config_is_not_utf8() {
    let rig = Rig::new();
    let m1 = rig.install_1();
    let root = rig.root();
    let mut ansi = release(PD2R_1).payload[CONFIG].clone();
    ansi.extend_from_slice(b"; caf\xe9\n");
    fs::write(root.join(CONFIG), &ansi).unwrap();

    let clean = Op::new(&rig, CleanInstall, PD2R_2, Some(m1));
    let result = clean.run(&rig, &NoFaults);

    assert!(
        result
            .as_ref()
            .is_ok_and(|r| r.clean_install_backups.len() == 1),
        "{:?}",
        result.map_err(|e| e.code)
    );
    assert_eq!(rig.installed_release().as_deref(), Some(PD2R_2));
}
