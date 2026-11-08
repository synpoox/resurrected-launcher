//! Clause 5: restart behavior. A restart is a new [`Launcher`] on the same State
//! and D2R folder; quitting is a real launcher process exiting. The launcher
//! process here is this test binary re-run as [`child_launcher_plays_then_quits`].

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;

use resurrected_launcher_lib::app::Launcher;
use resurrected_launcher_lib::contract::{Boundary, FailAt, FaultPoint, OperationKind, Readiness};
use resurrected_launcher_lib::install::ops::{self, Env};
use resurrected_launcher_lib::install::plan::Request;
use resurrected_launcher_lib::install::preflight::SystemHost;
use serde_json::json;

use crate::fake_loader::{self, FakeLoader};
use crate::rig::*;

const MANAGED: &str = "PD2Resurrected.mpq/data/global/excel/armor.txt";

/// A repair killed mid-activation: the launcher process "dies" at
/// `StagedMovedToLive` (injected crash, no rollback), leaving its journal.
fn crash_mid_repair(rig: &Rig) {
    crash_mid_repair_removing(rig, MANAGED);
}

/// [`crash_mid_repair`] after removing the managed file `managed` (phase B uses
/// one of the real release's files).
pub(crate) fn crash_mid_repair_removing(rig: &Rig, managed: &str) {
    fs::remove_file(rig.root().join(managed)).unwrap();
    let launcher: std::sync::Arc<Launcher> = rig.launcher();
    let installed = rig.releases.0.lock().unwrap().last().unwrap().clone();
    assert!(Boundary::sequence(OperationKind::Repair).contains(&Boundary::StagedMovedToLive));
    let crash = FailAt {
        boundary: Boundary::StagedMovedToLive,
        point: FaultPoint::AfterStep,
    };
    let cancel = AtomicBool::new(false);
    let env = Env {
        downloader: &launcher.downloader,
        seven_zip: &launcher.seven_zip,
        state: &launcher.state,
        host: &SystemHost,
        injector: &crash,
        cancel: &cancel,
    };
    let req = Request {
        kind: OperationKind::Repair,
        d2r: &rig.d2r,
        release: &installed.manifest,
        release_sha256: installed.sha256,
        installed: Some(&installed.manifest),
        acknowledge_save_breaking: false,
        consent_clean_install: false,
    };
    ops::run(&env, &req).expect_err("the injected crash");
}

fn journal(rig: &Rig) -> PathBuf {
    rig.d2r
        .join("mods")
        .join("PD2Resurrected.resurrected-journal.json")
}

/// Every file of `release` is in the product root with its published bytes.
fn complete(rig: &Rig, release: &crate::support::FixtureRelease) {
    for (rel, bytes) in &release.payload {
        let path = rel.split('/').fold(rig.root(), |p, c| p.join(c));
        assert_eq!(&fs::read(&path).unwrap(), bytes, "{rel}");
    }
}

#[test]
fn restart_a_pending_journal_blocks_play_until_recovery() {
    let rig = Rig::new();
    let r1 = rig.adopted();
    crash_mid_repair(&rig);
    assert!(journal(&rig).exists());

    let rig = rig.restart();
    rig.publish(&r1);
    // Before startup recovery ran.
    assert_eq!(rig.state().readiness, Readiness::RecoveryRequired);
    assert_eq!(code(rig.play().unwrap_err()), "recovery_required");

    // Startup while a game runs cannot recover, so the journal stays pending.
    let game = FakeLoader::start(&rig.d2r.join("D2RLoader.exe"));
    rig.start_up();
    assert!(journal(&rig).exists());
    assert!(matches!(rig.state().readiness, Readiness::Running { .. }));
    game.stop();
    rig.state_event();
    assert_eq!(rig.state().readiness, Readiness::RecoveryRequired);
    assert_eq!(code(rig.play().unwrap_err()), "recovery_required");
    assert!(rig.host.spawns().is_empty());

    let recovered = rig
        .invoke("recover_product", json!({ "product_id": PRODUCT }))
        .unwrap();
    let how = recovered["data"].as_str().unwrap().to_owned();
    assert!(how == "rolled_back" || how == "rolled_forward", "{how}");
    assert!(!journal(&rig).exists());
    assert_eq!(rig.state().readiness, Readiness::Ready);
    if how == "rolled_forward" {
        complete(&rig, &r1);
    } else {
        // The previous installation, missing the file the repair was restoring.
        assert!(!rig.root().join(MANAGED).exists());
    }
    let game = rig.play().unwrap();
    FakeLoader::adopt(&game.image).stop();
}

const CHILD_ENV: &str = "SLICE3_GATE_CHILD";

/// Not a gate test: the launcher process [`play_in_a_child_launcher`] starts.
/// Plays the adopted product in the D2R folder and State named by
/// `SLICE3_GATE_CHILD`, writes the started game, then returns, so the process
/// exits with the game running. Does nothing when run on its own.
#[test]
#[ignore = "run only as a child process by the restart tests"]
fn child_launcher_plays_then_quits() {
    let Some(job) = std::env::var_os(CHILD_ENV) else {
        return;
    };
    let job = fs::read_to_string(job).unwrap();
    let [d2r, state, out] = [0, 1, 2].map(|i| PathBuf::from(job.lines().nth(i).unwrap()));
    let rig = Rig::attach(&d2r, &state, Box::new(SystemHost));
    rig.publish(&real_layout(REAL_1));
    rig.start_up();
    let game = rig.play().unwrap();
    fs::write(out, format!("{}\n{}\n", game.pid, game.image.display())).unwrap();
}

/// Runs a separate launcher process that plays, then quits. Returns the game.
fn play_in_a_child_launcher(rig: &Rig) -> FakeLoader {
    let work = tempfile::tempdir().unwrap();
    let (job, out) = (work.path().join("job"), work.path().join("out"));
    let text = [&rig.d2r, &rig.state_dir, &out.to_path_buf()]
        .map(|p: &PathBuf| p.display().to_string())
        .join("\n");
    fs::write(&job, text).unwrap();
    let exe = rig.d2r.join("D2RLoader.exe");
    fake_loader::forget(&exe);
    // Output to a file, not a pipe: the game inherits the child's std handles
    // (std spawns with handle inheritance), so a pipe would stay open as long
    // as the game runs and `output()` would wait for the game, not the launcher.
    let log = work.path().join("log");
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "restart::child_launcher_plays_then_quits",
            "--exact",
            "--ignored",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, &job)
        .stdin(Stdio::null())
        .stdout(fs::File::create(&log).unwrap())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(
        status.success(),
        "child launcher failed:\n{}",
        fs::read_to_string(&log).unwrap_or_default()
    );
    let reported = fs::read_to_string(&out).expect("the child launcher reported its game");
    let mut lines = reported.lines();
    let pid: u32 = lines.next().unwrap().parse().unwrap();
    assert_eq!(Path::new(lines.next().unwrap()), exe);
    let game = FakeLoader::adopt(&exe);
    assert_eq!(game.pid(), pid);
    assert_eq!(game.started.argv[1..], ["-mod", "PD2Resurrected"]);
    game
}

#[test]
fn restart_quitting_the_launcher_leaves_the_game_running() {
    let rig = Rig::new();
    rig.adopted();
    // `output()` returned: the launcher process has exited.
    let game = play_in_a_child_launcher(&rig);
    std::thread::sleep(std::time::Duration::from_secs(1));
    let exited = fs::read_to_string(rig.d2r.join("D2RLoader.exe.exited"));
    assert!(
        game.is_running(),
        "the game ended with the launcher (its own exit record: {exited:?})"
    );
    game.stop();
}

#[test]
fn restart_rediscovers_the_running_game() {
    let rig = Rig::new();
    let r1 = rig.adopted();
    let game = play_in_a_child_launcher(&rig);

    let rig = rig.restart();
    rig.publish(&r1);
    rig.start_up();
    let state = rig.state();
    assert!(
        matches!(state.readiness, Readiness::Running { .. }),
        "{:?}",
        state.readiness
    );
    assert_eq!(state.running_game.unwrap().pid, game.pid());
    assert_eq!(code(rig.play().unwrap_err()), "game_running");
    let repair = rig.start("repair", json!({}), "any").unwrap_err();
    assert_eq!(code(repair), "game_running");
    assert!(rig.host.spawns().is_empty());

    // start_up's exit watcher reports the exit.
    game.stop();
    rig.state_event();
    assert_eq!(rig.state().readiness, Readiness::Ready);
}
