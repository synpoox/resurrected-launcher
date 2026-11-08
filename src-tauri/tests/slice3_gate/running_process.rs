//! Clause 3: running-process protection, with real processes found by a real
//! process listing. A fake loader started under the D2R root, outside the
//! launcher, blocks every operation; the same program outside the root does not.

use std::fs;

use resurrected_launcher_lib::contract::Readiness;
use serde_json::json;

use crate::fake_loader::{self, FakeLoader};
use crate::rig::*;

const EVERY_OPERATION: [(&str, bool); 6] = [
    ("install", false),
    ("adopt", false),
    ("update", false),
    ("repair", false),
    ("uninstall", false),
    ("clean_install", true),
];

#[test]
fn a_fake_loader_under_the_root_blocks_every_operation() {
    let rig = Rig::new();
    rig.adopted();
    rig.publish(&real_layout(REAL_1B));
    let tokens: Vec<String> = ["update", "repair", "uninstall"]
        .iter()
        .map(|k| rig.inspect(k, json!({})).unwrap().token)
        .collect();
    let loader = FakeLoader::start(&rig.d2r.join("D2RLoader.exe"));
    let before = tree(&rig.d2r);

    let state = rig.state();
    assert!(
        matches!(state.readiness, Readiness::Running { since } if since > 0),
        "{:?}",
        state.readiness
    );
    assert_eq!(state.running_game.unwrap().pid, loader.pid());

    for (kind, consent) in EVERY_OPERATION {
        let extra = json!({ "consent_clean_install": consent });
        // A fresh token where the plan allows one, else a stale one: either way
        // the running game is the refusal.
        let token = rig
            .inspect(kind, extra.clone())
            .map(|i| i.token)
            .unwrap_or_else(|_| tokens[0].clone());
        let refused = rig.start(kind, extra, &token).unwrap_err();
        assert_eq!(code(refused), "game_running", "{kind}");
    }
    let recover = rig.invoke("recover_product", json!({ "product_id": PRODUCT }));
    assert_eq!(code(recover.unwrap_err()), "game_running");
    assert_eq!(code(rig.play().unwrap_err()), "game_running");
    rig.desktop.will_pick(&rig.d2r);
    let choose = rig.invoke("choose_game_location", json!({}));
    assert_eq!(code(choose.unwrap_err()), "game_running");

    assert!(rig.host.spawns().is_empty());
    assert!(rig.outcomes.try_recv().is_err(), "an operation started");
    assert_eq!(
        before,
        tree(&rig.d2r),
        "something changed while the game ran"
    );
    assert!(loader.is_running(), "the launcher touched the game");

    loader.stop();
    assert_eq!(rig.state().readiness, Readiness::UpdateAvailable);
    assert_eq!(rig.run("repair", json!({}))["status"], "succeeded");
}

#[test]
fn a_process_outside_the_root_does_not_count() {
    let rig = Rig::new();
    rig.adopted();
    let parent = rig.d2r.parent().unwrap().to_owned();
    // `...\Diablo II Resurrected2` shares the root's name as a prefix.
    let sibling = fake_loader::install(&parent.join("Diablo II Resurrected2"), "D2RLoader.exe");
    let elsewhere = fake_loader::install(&parent.join("elsewhere"), "D2R.exe");
    let outside = [FakeLoader::start(&sibling), FakeLoader::start(&elsewhere)];

    let state = rig.state();
    assert_eq!(state.readiness, Readiness::Ready);
    assert_eq!(state.running_game, None);
    fs::remove_file(rig.root().join("modinfo.json")).unwrap();
    assert_eq!(rig.run("repair", json!({}))["status"], "succeeded");
    let game = rig.play().unwrap();
    let ours = FakeLoader::adopt(&game.image);
    assert!(outside.iter().all(FakeLoader::is_running));
    assert_eq!(rig.state().running_game.unwrap().pid, ours.pid());
    ours.stop();
    assert_eq!(rig.state().readiness, Readiness::Ready);
}
