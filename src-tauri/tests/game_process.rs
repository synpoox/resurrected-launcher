//! Game launch and process tracking against the real machine (#58), with
//! `examples/fake-d2rloader.rs` copied into temp D2R roots as `D2RLoader.exe`.
//! Nothing here touches a real D2R installation.
#![cfg(windows)]

mod support;

use std::ffi::c_void;
use std::fs;
use std::io::Write;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
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

/// `examples/fake-d2rloader.exe` of this test's profile, built once if missing.
fn fake_exe() -> &'static Path {
    static EXE: OnceLock<PathBuf> = OnceLock::new();
    EXE.get_or_init(|| {
        let deps = std::env::current_exe().unwrap();
        let exe = deps
            .parent()
            .unwrap()
            .with_file_name("examples")
            .join("fake-d2rloader.exe");
        if !exe.is_file() {
            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
            let mut build = Command::new(cargo);
            build
                .args(["build", "--example", "fake-d2rloader"])
                .current_dir(env!("CARGO_MANIFEST_DIR"));
            if !cfg!(debug_assertions) {
                build.arg("--release");
            }
            assert!(
                build.status().unwrap().success(),
                "building the fake-d2rloader example failed"
            );
        }
        exe
    })
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
            Killer::open(g.pid)
        })
    }

    /// Runs [`helper_plays_and_exits`] in a child test process, assigned to `job`
    /// before it plays, and waits for it to exit. Returns the game's PID.
    fn play_in_helper(&self, job: Option<&Job>) -> u32 {
        let mut helper = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "helper_plays_and_exits", "--nocapture"])
            .env(HELPER_ROOT, &self.root)
            .env(OUT, self.out())
            .env(SLEEP, LONG_MS)
            .env(HELPER_PID, self.tmp.path().join("pid"))
            // Not piped: the game inherits the helper's std handles, so a stdout
            // pipe would stay open until the game exits.
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        if let Some(job) = job {
            job.assign(&helper);
        }
        // The helper plays only after this line, so it is in the job by then.
        helper.stdin.take().unwrap().write_all(b"go\n").unwrap();
        assert!(helper.wait().unwrap().success(), "helper failed");
        fs::read_to_string(self.tmp.path().join("pid"))
            .unwrap()
            .parse()
            .unwrap()
    }
}

/// Set only in the helper process that [`Rig::play_in_helper`] starts.
const HELPER_ROOT: &str = "GAME_PROCESS_HELPER_ROOT";
/// Where the helper writes the game's PID.
const HELPER_PID: &str = "GAME_PROCESS_HELPER_PID";

/// The helper: a stand-in launcher that plays and exits. A no-op in normal runs.
#[test]
fn helper_plays_and_exits() {
    let Some(root) = std::env::var_os(HELPER_ROOT) else {
        return;
    };
    std::io::stdin().read_line(&mut String::new()).unwrap();
    let location = GameLocation {
        path: root.into(),
        build: "test".into(),
        store: GameStore::Unknown,
    };
    let game = play(&location, &manifest(), &Readiness::Ready, &SystemGameHost).unwrap();
    fs::write(std::env::var_os(HELPER_PID).unwrap(), game.pid.to_string()).unwrap();
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
#[allow(clippy::zombie_processes)] // Windows has no zombies; the Killer ends it.
fn start_external(root: &Path, sleep_ms: &str) -> Killer {
    let child = Command::new(root.join("D2RLoader.exe"))
        .current_dir(root)
        .env_remove(OUT)
        .env(SLEEP, sleep_ms)
        .spawn()
        .unwrap();
    // Opened while `child` still holds the process, so the PID cannot be reused yet.
    Killer::open(child.id())
}

type Handle = isize;

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
    fn TerminateProcess(process: Handle, code: u32) -> i32;
    fn WaitForSingleObject(handle: Handle, ms: u32) -> u32;
    fn CloseHandle(handle: Handle) -> i32;
    fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> Handle;
    fn SetInformationJobObject(job: Handle, class: i32, info: *const c_void, len: u32) -> i32;
    fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
}

/// Kills the process on drop so temp roots can be removed. It holds a handle
/// opened while the process was known to be ours, so a reused PID is never hit.
struct Killer {
    pid: u32,
    handle: Handle,
}

impl Killer {
    fn open(pid: u32) -> Self {
        const PROCESS_TERMINATE: u32 = 0x1;
        const SYNCHRONIZE: u32 = 0x0010_0000;
        // SAFETY: plain Win32 call; the handle is closed in Drop.
        let handle = unsafe { OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, 0, pid) };
        assert_ne!(handle, 0, "opening pid {pid}");
        Killer { pid, handle }
    }

    /// Whether the process is still running.
    fn alive(&self) -> bool {
        const WAIT_TIMEOUT: u32 = 0x102;
        // SAFETY: `handle` is open until Drop.
        unsafe { WaitForSingleObject(self.handle, 0) == WAIT_TIMEOUT }
    }
}

impl Drop for Killer {
    fn drop(&mut self) {
        // SAFETY: `handle` is open and owned by this Killer.
        unsafe {
            TerminateProcess(self.handle, 1);
            WaitForSingleObject(self.handle, 5_000);
            CloseHandle(self.handle);
        }
    }
}

/// A job object that kills its processes when closed (on drop), as a terminal or
/// IDE may run the launcher in.
struct Job(Handle);

impl Job {
    fn kill_on_close(breakaway_ok: bool) -> Self {
        #[repr(C)]
        #[derive(Default)]
        struct BasicLimits {
            per_process_user_time: i64,
            per_job_user_time: i64,
            flags: u32,
            min_working_set: usize,
            max_working_set: usize,
            active_processes: u32,
            affinity: usize,
            priority_class: u32,
            scheduling_class: u32,
        }
        #[repr(C)]
        #[derive(Default)]
        struct ExtendedLimits {
            basic: BasicLimits,
            io_counters: [u64; 6],
            process_memory: usize,
            job_memory: usize,
            peak_process_memory: usize,
            peak_job_memory: usize,
        }
        const EXTENDED_LIMIT_INFORMATION: i32 = 9;
        const KILL_ON_JOB_CLOSE: u32 = 0x2000;
        const BREAKAWAY_OK: u32 = 0x800;
        let mut limits = ExtendedLimits::default();
        limits.basic.flags = KILL_ON_JOB_CLOSE | if breakaway_ok { BREAKAWAY_OK } else { 0 };
        // SAFETY: plain Win32 calls; `limits` outlives the call, the job is closed in Drop.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            assert_ne!(job, 0);
            let set = SetInformationJobObject(
                job,
                EXTENDED_LIMIT_INFORMATION,
                (&limits as *const ExtendedLimits).cast(),
                std::mem::size_of::<ExtendedLimits>() as u32,
            );
            assert_ne!(set, 0, "{}", std::io::Error::last_os_error());
            Job(job)
        }
    }

    fn assign(&self, child: &Child) {
        // SAFETY: both handles are open for the call.
        let ok = unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle() as Handle) };
        assert_ne!(ok, 0, "{}", std::io::Error::last_os_error());
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: the job handle is owned by this Job.
        unsafe { CloseHandle(self.0) };
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
    assert_eq!(pids_under(&rig.root), vec![game.pid]);
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
    assert_eq!(
        found.iter().map(|g| g.pid).collect::<Vec<_>>(),
        [inside.pid]
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(
        found[0].since <= now && now - found[0].since < 60,
        "{found:?}"
    );
    assert_eq!(pids_under(&sibling), vec![outside.pid]);
    // Case-insensitive, as Windows compares paths.
    let upper = PathBuf::from(rig.root.to_string_lossy().to_uppercase());
    assert_eq!(pids_under(&upper), vec![inside.pid]);
}

#[test]
fn a_running_game_is_rediscovered_by_a_fresh_scan_after_restart() {
    let rig = Rig::new();
    // The launcher that played exits; only this test's kill handle remains.
    let game = Killer::open(rig.play_in_helper(None));
    let pid = game.pid;
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
    assert_eq!(pids_under(&rig.root), vec![game.pid]);
}

#[test]
fn the_game_outlives_the_launcher_process_that_played_it() {
    let rig = Rig::new();
    let game = Killer::open(rig.play_in_helper(None));
    assert_eq!(read_record(&rig.out())["args"][0], "-mod");
    std::thread::sleep(Duration::from_millis(300));
    assert!(game.alive());
    assert_eq!(pids_under(&rig.root), vec![game.pid]);
}

#[test]
fn the_game_breaks_away_from_a_kill_on_close_job_that_allows_it() {
    let rig = Rig::new();
    let job = Job::kill_on_close(true);
    let game = Killer::open(rig.play_in_helper(Some(&job)));
    drop(job); // kills whatever is still in the job
    std::thread::sleep(Duration::from_millis(300));
    assert!(game.alive(), "the game was killed with the launcher's job");
}

#[test]
fn play_falls_back_inside_a_job_that_forbids_breakaway() {
    let rig = Rig::new();
    let job = Job::kill_on_close(false);
    // Breakaway is denied there, so play retries without it and still starts the game.
    let game = Killer::open(rig.play_in_helper(Some(&job)));
    assert!(game.alive());
    drop(job);
    let deadline = Instant::now() + Duration::from_secs(5);
    while game.alive() {
        assert!(Instant::now() < deadline, "the job did not hold the game");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn play_refuses_when_the_journal_cannot_be_checked() {
    let rig = Rig::new();
    let location = GameLocation {
        path: rig.tmp.path().join("bad\0name"), // any error but NotFound
        ..rig.location()
    };
    let err = play(&location, &manifest(), &Readiness::Ready, &SystemGameHost).unwrap_err();
    assert_eq!(err.code, ErrorCode::RecoveryRequired, "{err}");
}
