//! Clause "Game left running on Quit": a launcher process plays with
//! `examples/fake-d2rloader.exe` as D2RLoader.exe, takes the tray's Quit
//! decision and exits. The game outlives it; the next launch (a new launcher on
//! the same State and D2R folder) rediscovers it by the real process listing
//! and refuses every write with `game_running`.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use resurrected_launcher_lib::contract::Readiness;
use resurrected_launcher_lib::install::preflight::SystemHost;
use resurrected_launcher_lib::lifecycle::{self, Quit};
use serde_json::{json, Value};
use tauri::Manager;

use crate::helper;
use crate::rig::*;
use crate::support::DisposableGame;

const CHILD: &str = "SLICE4_GATE_QUIT_CHILD";

/// A D2R folder with fake game files and `fake-d2rloader.exe` as the loader.
fn dirs() -> Arc<Dirs> {
    let game = DisposableGame::new();
    let d2r = game.d2r();
    fs::create_dir_all(d2r).unwrap();
    fs::write(d2r.join("D2R.exe"), D2R_BYTES).unwrap();
    fs::copy(helper::fake_d2rloader(), d2r.join("D2RLoader.exe")).unwrap();
    fs::write(d2r.join("D2RCore.dll"), b"fake D2RCore.dll").unwrap();
    Arc::new(Dirs {
        game,
        state: tempfile::tempdir().unwrap(),
    })
}

/// Not a gate test: the launcher process [`quit_leaves_the_game_running_and_the_next_launch_refuses_writes`]
/// starts. Plays in the D2R folder and State its job names, writes the game's
/// pid, takes the tray Quit decision and exits as `app.exit(0)` would.
#[test]
#[ignore = "run only as a child process by the Quit gate test"]
fn child_launcher_plays_then_quits() {
    let Some(job) = std::env::var_os(CHILD) else {
        return;
    };
    let job = fs::read_to_string(job).unwrap();
    let [d2r, state, out] = [0, 1, 2].map(|i| PathBuf::from(job.lines().nth(i).unwrap()));
    let rig = Rig::attach(&d2r, &state, Box::new(SystemHost));
    rig.publish(&real_layout(REAL_1));
    rig.start_up();
    let game = rig.play().unwrap();
    fs::write(&out, game.pid.to_string()).unwrap();
    assert_eq!(lifecycle::quit(rig.webview.app_handle()), Quit::Exit);
    std::process::exit(0);
}

fn refused(rig: &Rig, cmd: &str, body: Value) {
    let e = rig
        .invoke(cmd, body)
        .err()
        .unwrap_or_else(|| panic!("{cmd} was accepted while the game runs"));
    assert_eq!(code(e), "game_running", "{cmd}");
}

#[test]
fn quit_leaves_the_game_running_and_the_next_launch_refuses_writes() {
    let dirs = dirs();
    let rig = Rig::on(Arc::clone(&dirs), Box::new(SystemHost));
    let release = rig.adopted();
    drop(rig);

    let work = tempfile::tempdir().unwrap();
    let (job, pid_file, record) = (
        work.path().join("job"),
        work.path().join("pid"),
        work.path().join("started.json"),
    );
    let lines = [dirs.game.d2r(), dirs.state.path(), &pid_file]
        .map(|p| p.display().to_string())
        .join("\n");
    fs::write(&job, lines).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "quit_game_running::child_launcher_plays_then_quits",
            "--exact",
            "--ignored",
            "--test-threads=1",
        ])
        .env(CHILD, &job)
        // The game inherits these: it stays up a minute and records its start.
        .env("FAKE_D2RLOADER_SLEEP_MS", "60000")
        .env("FAKE_D2RLOADER_OUT", &record)
        // Not piped: the game inherits the handles and would hold a pipe open.
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "the launcher process failed");
    let pid: u32 = fs::read_to_string(&pid_file).unwrap().parse().unwrap();
    struct Kill(u32);
    impl Drop for Kill {
        fn drop(&mut self) {
            helper::kill(self.0);
        }
    }
    let _game = Kill(pid);
    std::thread::sleep(Duration::from_millis(500));
    assert!(helper::alive(pid), "the game ended with the launcher");
    let started: Value = serde_json::from_str(&fs::read_to_string(&record).unwrap()).unwrap();
    assert_eq!(started["args"], json!(["-mod", "PD2Resurrected"]));

    // The next launch.
    let before = tree(dirs.game.d2r());
    let rig = Rig::on(Arc::clone(&dirs), Box::new(SystemHost));
    rig.publish(&release);
    rig.start_up();
    let state = rig.state();
    assert!(
        matches!(state.readiness, Readiness::Running { .. }),
        "{:?}",
        state.readiness
    );
    assert_eq!(state.running_game.as_ref().unwrap().pid, pid);

    assert_eq!(code(rig.play().unwrap_err()), "game_running");
    for kind in ["repair", "uninstall", "update", "adopt", "install"] {
        let start = rig.start(kind, json!({}), "any");
        assert_eq!(code(start.unwrap_err()), "game_running", "start {kind}");
    }
    refused(&rig, "recover_product", json!({ "product_id": PRODUCT }));
    refused(&rig, "choose_game_location", json!({}));
    rig.invoke("detect_game_locations", json!({})).unwrap();
    refused(&rig, "select_game_location", json!({ "index": 0 }));
    // The tray's Update goes through the same check.
    let mut tray = lifecycle::Model::default();
    tray.click(rig.webview.app_handle(), "update:pd2r");
    assert!(rig.host.spawns().is_empty(), "nothing was started");
    assert_eq!(tree(dirs.game.d2r()), before, "nothing was written");
}
