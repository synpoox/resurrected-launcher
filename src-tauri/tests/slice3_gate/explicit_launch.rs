//! Clause 4: Play starts exactly `<D2R>\D2RLoader.exe -mod PD2Resurrected` from
//! the D2R folder, through the production spawn path. The fake loader records
//! the argv it actually received, so this checks the real command line, not
//! only what the launcher asked for.

use resurrected_launcher_lib::contract::Readiness;

use crate::fake_loader::FakeLoader;
use crate::rig::*;

#[test]
fn explicit_launch_starts_d2rloader_with_exactly_mod_and_the_product_folder() {
    let rig = Rig::new();
    let release = rig.adopted();
    let exe = rig.d2r.join("D2RLoader.exe");
    assert_eq!(release.manifest.mod_folder(), "PD2Resurrected");

    let game = rig.play().unwrap();
    let loader = FakeLoader::adopt(&exe);
    assert_eq!(game.image, exe);
    assert_eq!(game.pid, loader.pid());
    assert_eq!(
        loader.started.argv,
        [exe.to_string_lossy().as_ref(), "-mod", "PD2Resurrected"],
        "the loader's own argv"
    );
    assert_eq!(loader.started.cwd, rig.d2r);
    assert_eq!(
        rig.host.spawns(),
        [(
            exe.clone(),
            vec!["-mod".to_owned(), "PD2Resurrected".to_owned()],
            rig.d2r.clone()
        )]
    );

    // One game at a time: a second Play is refused and spawns nothing.
    assert!(matches!(rig.state().readiness, Readiness::Running { .. }));
    assert_eq!(code(rig.play().unwrap_err()), "game_running");
    assert_eq!(rig.host.spawns().len(), 1);

    // The exit watcher reports the exit; the product is Ready again.
    loader.stop();
    rig.state_event();
    assert_eq!(rig.state().readiness, Readiness::Ready);
}
