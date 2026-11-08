//! Game launch and process tracking against the real machine (#58), with
//! `examples/fake-d2rloader.rs` copied into temp D2R roots as `D2RLoader.exe`.
//! Nothing here touches a real D2R installation.
#![cfg(windows)]

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use resurrected_launcher_lib::catalog::ReleaseManifest;
use resurrected_launcher_lib::contract::{ErrorCode, GameLocation, GameStore, Readiness};
use resurrected_launcher_lib::game::launch::play;
use resurrected_launcher_lib::game::process::{running_under, wait_until_exited};
use resurrected_launcher_lib::game::{GameHost, SystemGameHost};
use serde_json::Value;
use tempfile::TempDir;

const OUT: &str = "FAKE_D2RLOADER_OUT";
const SLEEP: &str = "FAKE_D2RLOADER_SLEEP_MS";
const LONG_MS: &str = "60000";

/// `play` spawns through the host, which inherits this process's environment, so
/// tests that set the fake's variables and call `play` take turns.
static ENV: Mutex<()> = Mutex::new(());

fn fake_exe() -> PathBuf {
    let deps = std::env::current_exe().unwrap();
    let exe = deps
        .parent()
        .unwrap()
        .with_file_name("examples")
        .join("fake-d2rloader.exe");
    if !exe.is_file() {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let ok = Command::new(cargo)
            .args(["build", "--example", "fake-d2rloader"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .status()
            .unwrap()
            .success();
        assert!(ok, "building the fake-d2rloader example failed");
    }
    exe
}

/// A temp folder holding a D2R root (`<tmp>\D2R`) with the fake loader.
struct Rig {
    tmp: TempDir,
    root: PathBuf,
}

impl Rig {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("D2R");
        install_fake(&root);
        Rig { tmp, root }
    }

    fn location(&self) -> GameLocation {
        GameLocation {
            path: self.root.clone(),
            build: "test".into(),
            store: GameStore::Unknown,
        }
    }

    fn out(&self) -> PathBuf {
        self.tmp.path().join("record.json")
    }

    /// Plays with the fake recording to [`Rig::out`] and sleeping `sleep_ms`.
    fn play(
        &self,
        readiness: &Readiness,
        sleep_ms: &str,
    ) -> Result<Killer, resurrected_launcher_lib::contract::LauncherError> {
        let _env = ENV.lock().unwrap_or_else(|p| p.into_inner());
        std::env::set_var(OUT, self.out());
        std::env::set_var(SLEEP, sleep_ms);
        play(&self.location(), &manifest(), readiness, &SystemGameHost).map(|g| {
            assert_eq!(g.image, self.root.join("D2RLoader.exe"));
            Killer(g.pid)
        })
    }
}

fn install_fake(root: &Path) {
    fs::create_dir_all(root).unwrap();
    fs::copy(fake_exe(), root.join("D2RLoader.exe")).unwrap();
}

fn manifest() -> ReleaseManifest {
    support::release(support::PD2R_1).manifest
}

/// Starts `<root>\D2RLoader.exe` directly, as a game started outside the launcher.
/// The `Child` handle is dropped at once; the process keeps running.
fn start_external(root: &Path, sleep_ms: &str) -> Killer {
    let child = Command::new(root.join("D2RLoader.exe"))
        .current_dir(root)
        .env_remove(OUT)
        .env(SLEEP, sleep_ms)
        .spawn()
        .unwrap()
        .id();
    Killer(child)
}

/// Kills the process on drop so temp roots can be removed.
struct Killer(u32);

impl Drop for Killer {
    fn drop(&mut self) {
        let _ = Command::new("taskkill")
            .args(["/F", "/PID", &self.0.to_string()])
            .output();
    }
}

fn pids_under(root: &Path) -> Vec<u32> {
    running_under(root, &SystemGameHost)
        .unwrap()
        .into_iter()
        .map(|g| g.pid)
        .collect()
}

fn read_record(path: &Path) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(text) = fs::read_to_string(path) {
            if let Ok(v) = serde_json::from_str(&text) {
                return v;
            }
        }
        assert!(Instant::now() < deadline, "fake loader wrote no record");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn play_runs_the_loader_with_exactly_mod_folder_args_from_the_root() {
    let rig = Rig::new();
    let game = rig.play(&Readiness::UpdateAvailable, LONG_MS).unwrap();
    let record = read_record(&rig.out());
    assert_eq!(record["args"], serde_json::json!(["-mod", "PD2R"]));
    let cwd = PathBuf::from(record["cwd"].as_str().unwrap());
    assert_eq!(
        fs::canonicalize(cwd).unwrap(),
        fs::canonicalize(&rig.root).unwrap()
    );
    // `play` dropped its spawn handle; the game must still be running.
    assert_eq!(pids_under(&rig.root), vec![game.0]);
}

#[test]
fn play_refuses_every_unplayable_readiness_without_spawning() {
    let rig = Rig::new();
    let refused = [
        Readiness::D2rMissing,
        Readiness::Running { since: 1 },
        Readiness::RecoveryRequired,
        Readiness::BuildUnsupported { found: "x".into() },
        Readiness::LoaderMissing,
        Readiness::LoaderUnsupported {
            found: "1.0".into(),
            allowed: vec!["1.3.1-beta".into()],
        },
        Readiness::NotInstalled,
    ];
    for readiness in refused {
        let err = rig.play(&readiness, "0").err().expect("refused");
        assert_eq!(Some(err.code), readiness.play_refusal(), "{readiness:?}");
    }
    assert!(!rig.out().exists());
    assert!(pids_under(&rig.root).is_empty());
}

#[test]
fn play_refuses_while_a_journal_is_pending() {
    let rig = Rig::new();
    let mods = rig.root.join("mods");
    fs::create_dir_all(&mods).unwrap();
    fs::write(mods.join("PD2R.resurrected-journal.json"), "{}").unwrap();
    let err = rig.play(&Readiness::Ready, "0").err().expect("refused");
    assert_eq!(err.code, ErrorCode::RecoveryRequired);
    assert!(!rig.out().exists());
}

#[test]
fn play_refuses_while_a_game_already_runs_under_the_root() {
    let rig = Rig::new();
    let _external = start_external(&rig.root, LONG_MS);
    let err = rig.play(&Readiness::Ready, "0").err().expect("refused");
    assert_eq!(err.code, ErrorCode::GameRunning);
    assert!(!rig.out().exists());
}

#[test]
fn an_external_game_is_found_under_its_root_only() {
    let rig = Rig::new();
    let sibling = rig.tmp.path().join("D2R2"); // `C:\..\D2R2` is not under `..\D2R`
    install_fake(&sibling);
    let inside = start_external(&rig.root, LONG_MS);
    let outside = start_external(&sibling, LONG_MS);

    let found = running_under(&rig.root, &SystemGameHost).unwrap();
    assert_eq!(found.iter().map(|g| g.pid).collect::<Vec<_>>(), [inside.0]);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(
        found[0].since <= now && now - found[0].since < 60,
        "{found:?}"
    );
    assert_eq!(pids_under(&sibling), vec![outside.0]);
    // Case-insensitive, as Windows compares paths.
    let upper = PathBuf::from(rig.root.to_string_lossy().to_uppercase());
    assert_eq!(pids_under(&upper), vec![inside.0]);
}

#[test]
fn a_running_game_is_rediscovered_by_a_fresh_scan_after_restart() {
    let rig = Rig::new();
    let pid = {
        let game = rig.play(&Readiness::Ready, LONG_MS).unwrap();
        let pid = game.0;
        std::mem::forget(game); // the launcher exits; the game does not
        pid
    };
    let _kill = Killer(pid);
    // A restarted launcher holds nothing but the root and a new host.
    let host: Box<dyn GameHost> = Box::new(SystemGameHost);
    let found = running_under(&rig.root, host.as_ref()).unwrap();
    assert_eq!(found.iter().map(|g| g.pid).collect::<Vec<_>>(), [pid]);
    assert_eq!(
        fs::canonicalize(&found[0].image).unwrap(),
        fs::canonicalize(rig.root.join("D2RLoader.exe")).unwrap()
    );
}

#[test]
fn wait_until_exited_returns_once_the_game_exits() {
    let rig = Rig::new();
    let start = Instant::now();
    let _game = rig.play(&Readiness::Ready, "1500").unwrap();
    assert!(!pids_under(&rig.root).is_empty());
    wait_until_exited(&rig.root, &SystemGameHost, Duration::from_millis(100)).unwrap();
    assert!(start.elapsed() >= Duration::from_millis(1400));
    assert!(pids_under(&rig.root).is_empty());
}

#[test]
fn the_game_outlives_its_dropped_spawn_handle() {
    let rig = Rig::new();
    let game = start_external(&rig.root, LONG_MS); // its `Child` is already dropped
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(pids_under(&rig.root), vec![game.0]);
}
