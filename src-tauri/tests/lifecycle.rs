//! Window and tray lifecycle (#75) against the real machine: single instance with
//! two helper processes, Quit while the game runs with `examples/fake-d2rloader.rs`
//! as D2RLoader.exe in a temp D2R root, Quit while an install downloads from the
//! fixture server, and the plugins' commands over IPC.
//! Nothing here touches a real D2R installation or the real launcher's data.
#![cfg(windows)]

mod support;

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use resurrected_launcher_lib::app::{
    self, CachedReleases, Desktop, Launcher, Release, Releases, OUTCOME_EVENT,
};
use resurrected_launcher_lib::catalog::{GameBuild, Platform};
use resurrected_launcher_lib::contract::{
    GameLocation, GameStore, LauncherError, ProductId, Readiness, RelativePath, CONTRACT_VERSION,
};
use resurrected_launcher_lib::game::launch::play;
use resurrected_launcher_lib::game::SystemGameHost;
use resurrected_launcher_lib::install::download::Downloader;
use resurrected_launcher_lib::install::preflight::SystemHost;
use resurrected_launcher_lib::lifecycle::{self, Quit};
use resurrected_launcher_lib::state::{InstalledRelease, Preferences, StateStore};
use serde_json::{json, Value};
use support::{DisposableGame, FixtureServer, Reply};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{
    get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY,
};
use tauri::webview::InvokeRequest;
use tauri::{App, Listener, WebviewWindow, WebviewWindowBuilder};

/// Set only in helper processes: where the instance writes its setup marker.
const INSTANCE_MARKER: &str = "LIFECYCLE_INSTANCE_MARKER";
/// Set only in the Quit helper: the temp D2R root, and where to write the game's PID.
const QUIT_ROOT: &str = "LIFECYCLE_QUIT_ROOT";
const QUIT_PID: &str = "LIFECYCLE_QUIT_PID";

type Handle = isize;

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
    fn TerminateProcess(process: Handle, code: u32) -> i32;
    fn WaitForSingleObject(handle: Handle, ms: u32) -> u32;
    fn CloseHandle(handle: Handle) -> i32;
}

#[link(name = "user32")]
extern "system" {
    fn GetMessageW(msg: *mut [usize; 8], hwnd: isize, min: u32, max: u32) -> i32;
    fn DispatchMessageW(msg: *const [usize; 8]) -> isize;
}

/// A launcher instance built by the real [`lifecycle::builder`] on the mock
/// runtime. Its setup stands in for the launcher's: it writes the marker where the
/// real one constructs `Launcher` and starts recovery under the operation lock.
/// A no-op in normal runs.
#[test]
fn helper_instance() {
    let Some(marker) = std::env::var_os(INSTANCE_MARKER) else {
        return;
    };
    let marker = PathBuf::from(marker);
    // mock_context's identifier is not the real launcher's, so a running launcher
    // on this machine is never involved.
    lifecycle::builder(mock_builder(), move |_| {
        fs::write(&marker, "setup ran")?;
        // Only the first instance gets here; a second one exits inside build().
        println!("ready");
        std::thread::spawn(|| {
            std::io::stdin().read_line(&mut String::new()).unwrap();
            std::process::exit(0);
        });
        // Setup runs on the thread owning the plugin's window: pump it so a second
        // instance's message is delivered. Never returns.
        let mut msg = [0usize; 8];
        // SAFETY: plain Win32 message loop on this thread's queue.
        while unsafe { GetMessageW(&mut msg, 0, 0, 0) } > 0 {
            unsafe { DispatchMessageW(&msg) };
        }
        Ok(())
    })
    .build(mock_context(noop_assets()))
    .unwrap()
    .run(|_, _| {});
}

fn instance(marker: &Path) -> std::process::Child {
    Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "helper_instance",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(INSTANCE_MARKER, marker)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap()
}

#[test]
fn second_instance_exits_before_constructing_the_launcher() {
    let tmp = tempfile::tempdir().unwrap();
    let (first_marker, second_marker) = (tmp.path().join("first"), tmp.path().join("second"));
    let mut first = instance(&first_marker);
    let mut out = BufReader::new(first.stdout.take().unwrap());
    let mut line = String::new();
    // libtest may print "test helper_instance ... " on the same line.
    while !line.trim_end().ends_with("ready") {
        line.clear();
        assert_ne!(out.read_line(&mut line).unwrap(), 0, "first instance ended");
    }
    assert!(first_marker.is_file(), "the first instance ran setup");

    let mut second = instance(&second_marker);
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = second.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "second instance did not exit");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success());
    assert!(
        !second_marker.exists(),
        "the second instance ran setup (would construct Launcher)"
    );
    assert!(
        first.try_wait().unwrap().is_none(),
        "the first instance keeps running"
    );
    drop(first.stdin.take());
    assert!(first.wait().unwrap().success());
}

/// `examples/fake-d2rloader.exe` of this test's profile, built once if missing
/// (as in `tests/game_process.rs`).
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
            assert!(build.status().unwrap().success(), "building fake-d2rloader");
        }
        exe
    })
}

struct NoDesktop;

impl Desktop for NoDesktop {
    fn pick_folder(&self) -> Option<PathBuf> {
        None
    }
    fn open_loader_download_page(&self) -> std::io::Result<()> {
        Ok(())
    }
    fn open_folder(&self, _: &Path) -> std::io::Result<()> {
        Ok(())
    }
    fn show_in_folder(&self, _: &Path) -> std::io::Result<()> {
        Ok(())
    }
    fn pick_save_file(&self, _: &str) -> Option<PathBuf> {
        None
    }
    fn saved_games(&self) -> std::io::Result<PathBuf> {
        Err(std::io::Error::other("no Saved Games in tests"))
    }
    fn run_value(&self) -> std::io::Result<Option<String>> {
        Ok(None)
    }
    fn set_run_value(&self, _: Option<&str>) -> std::io::Result<()> {
        Ok(())
    }
}

/// The one recommended (and installed) release.
struct Published(Release);

impl Releases for Published {
    fn recommended(&self, _: &mut StateStore, _: &ProductId) -> Result<Release, LauncherError> {
        Ok(self.0.clone())
    }
    fn installed(
        &self,
        _: &mut StateStore,
        _: &InstalledRelease,
    ) -> Result<Release, LauncherError> {
        Ok(self.0.clone())
    }
}

/// A mock app managing a `Launcher` on temp state with `d2r` selected, and its
/// `main` webview. Catalog refreshes fail like offline.
fn mount(
    state_dir: &Path,
    d2r: &Path,
    releases: Box<dyn Releases>,
    downloader: Downloader,
) -> (App<MockRuntime>, WebviewWindow<MockRuntime>) {
    let mut state = StateStore::open(state_dir);
    state
        .set_preferences(Preferences {
            d2r_location: Some(d2r.to_owned()),
            ..Preferences::default()
        })
        .unwrap();
    let mut launcher = Launcher::new(
        state,
        releases,
        downloader,
        support::seven_zip(),
        Box::new(SystemHost),
        Box::new(SystemGameHost),
        Box::new(NoDesktop),
    );
    launcher.feed_url = "https://feed.fixtures.invalid/feed.json".into();
    let app = mock_builder()
        .invoke_handler(app::invoke_handler())
        .manage(Arc::new(launcher))
        .build(tauri::generate_context!())
        .unwrap();
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    (app, webview)
}

fn events(app: &App<MockRuntime>, event: &str) -> Receiver<Value> {
    let (tx, rx) = channel();
    app.listen_any(event, move |e| {
        let _ = tx.send(serde_json::from_str(e.payload()).unwrap());
    });
    rx
}

fn invoke(webview: &WebviewWindow<MockRuntime>, cmd: &str, body: Value) -> Value {
    get_ipc_response(
        webview,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "http://tauri.localhost".parse().unwrap(),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .unwrap_or_else(|e| panic!("{cmd}: {e}"))
    .deserialize()
    .unwrap()
}

/// Stand-in launcher: plays, takes the tray's Quit decision on a mock app with an
/// idle `Launcher`, and exits as `app.exit` does. A no-op in normal runs.
#[test]
fn helper_plays_then_quits() {
    let Some(root) = std::env::var_os(QUIT_ROOT) else {
        return;
    };
    let root = PathBuf::from(root);
    let location = GameLocation {
        path: root.clone(),
        build: "test".into(),
        store: GameStore::Unknown,
    };
    let manifest = support::release(support::PD2R_1).manifest;
    let game = play(&location, &manifest, &Readiness::Ready, &SystemGameHost).unwrap();
    fs::write(std::env::var_os(QUIT_PID).unwrap(), game.pid.to_string()).unwrap();
    let state = tempfile::tempdir().unwrap();
    let releases = Box::new(CachedReleases);
    let (app, _webview) = mount(state.path(), &root, releases, Downloader::new());
    assert_eq!(lifecycle::quit(app.handle()), Quit::Exit);
    std::process::exit(0);
}

#[test]
fn quit_while_the_game_runs_leaves_the_game_running() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("D2R");
    fs::create_dir_all(&root).unwrap();
    fs::copy(fake_exe(), root.join("D2RLoader.exe")).unwrap();
    let pid_file = tmp.path().join("pid");
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "helper_plays_then_quits", "--nocapture"])
        .env(QUIT_ROOT, &root)
        .env(QUIT_PID, &pid_file)
        .env("FAKE_D2RLOADER_SLEEP_MS", "60000")
        .env_remove("FAKE_D2RLOADER_OUT")
        // Not piped: the game would hold the pipe open.
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "helper failed");
    let pid: u32 = fs::read_to_string(&pid_file).unwrap().parse().unwrap();
    const PROCESS_TERMINATE: u32 = 0x1;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const WAIT_TIMEOUT: u32 = 0x102;
    // SAFETY: plain Win32 calls; the handle is closed below.
    unsafe {
        let game = OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, 0, pid);
        assert_ne!(game, 0, "the game exited with the launcher");
        let alive = WaitForSingleObject(game, 0) == WAIT_TIMEOUT;
        TerminateProcess(game, 1);
        WaitForSingleObject(game, 5_000);
        CloseHandle(game);
        assert!(alive, "the game exited with the launcher");
    }
}

/// Quit while an install is downloading asks the frontend and does not exit. The
/// decision reads the launcher's running operation, not the tray's view of it.
#[test]
fn quit_while_an_install_downloads_asks_and_does_not_exit() {
    const D2R_BYTES: &[u8] = b"fake D2R.exe for #75";
    let server = FixtureServer::start();
    let game = DisposableGame::new();
    fs::create_dir_all(game.d2r()).unwrap();
    fs::write(game.d2r().join("D2R.exe"), D2R_BYTES).unwrap();
    let r1 = support::release(support::PD2R_1);
    let mut manifest = r1.manifest.clone();
    manifest.archive = server.serve(&r1);
    manifest.compatibility.game_builds = vec![GameBuild {
        build: "fixture".into(),
        platform: Platform::WindowsX64,
        executable: RelativePath::new("D2R.exe").unwrap(),
        size: D2R_BYTES.len() as u64,
        sha256: support::sha256(D2R_BYTES),
    }];
    // Holds the operation in its download while Quit is chosen.
    server.route(
        &r1.archive_path(),
        Reply::Delayed(
            Duration::from_secs(2),
            Box::new(Reply::Body(r1.archive.clone())),
        ),
    );
    game.product_root(&r1);
    let state = tempfile::tempdir().unwrap();
    let release = Release {
        manifest,
        sha256: support::sha256(&r1.manifest_json),
    };
    let downloader = Downloader::with_tls(server.tls_config());
    let (app, webview) = mount(
        state.path(),
        game.d2r(),
        Box::new(Published(release)),
        downloader,
    );
    let (requests, outcomes) = (events(&app, app::TRAY_EVENT), events(&app, OUTCOME_EVENT));
    let request = json!({
        "contract_version": CONTRACT_VERSION,
        "data": { "product_id": "pd2r", "kind": "install" },
    });
    let plan = invoke(&webview, "inspect_operation", json!({ "request": request }));
    invoke(
        &webview,
        "start_operation",
        json!({ "request": request, "inspection_token": plan["data"]["token"] }),
    );

    assert_eq!(
        lifecycle::quit(app.handle()),
        Quit::Busy(ProductId("pd2r".into()))
    );
    let asked = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(asked["contract_version"], CONTRACT_VERSION);
    assert_eq!(
        asked["data"],
        json!({ "action": "quit_while_busy", "product_id": "pd2r" })
    );
    // The install carries on; the temp dirs outlive it.
    let outcome = outcomes.recv_timeout(Duration::from_secs(60)).unwrap();
    assert_eq!(outcome["data"]["status"], "succeeded", "{outcome}");
    assert_eq!(lifecycle::quit(app.handle()), Quit::Exit);
}

/// The notification and single-instance plugins are registered Rust-side, but no
/// capability grants their commands to the webview.
#[test]
fn lifecycle_plugin_commands_are_refused_over_ipc() {
    // Single instance stays out of this process (its mutex would take the
    // identifier); it registers no commands, so its namespace is checked anyway.
    let app = lifecycle::plugins(mock_builder())
        .invoke_handler(app::invoke_handler())
        .build(tauri::generate_context!())
        .unwrap();
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    for cmd in [
        "plugin:notification|notify",
        "plugin:notification|show",
        "plugin:notification|request_permission",
        "plugin:notification|is_permission_granted",
        "plugin:notification|permission_state",
        "plugin:notification|register_listener",
        "plugin:single-instance|anything",
    ] {
        let err = get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: "http://tauri.localhost".parse().unwrap(),
                body: InvokeBody::Json(serde_json::json!({ "options": { "title": "x" } })),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("not allowed"), "{cmd}: {err}");
    }
}
