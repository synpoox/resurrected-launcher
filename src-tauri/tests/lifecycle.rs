//! Window and tray lifecycle (#75) against the real machine: single instance with
//! two helper processes, Quit while the game runs with `examples/fake-d2rloader.rs`
//! as D2RLoader.exe in a temp D2R root, and the plugins' commands over IPC.
//! Nothing here touches a real D2R installation or the real launcher's data.
#![cfg(windows)]

mod support;

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use resurrected_launcher_lib::app::{self, CatalogState, ProductState};
use resurrected_launcher_lib::contract::{GameLocation, GameStore, ProductId, Readiness};
use resurrected_launcher_lib::game::launch::play;
use resurrected_launcher_lib::game::SystemGameHost;
use resurrected_launcher_lib::lifecycle::{self, quit_plan, Quit};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::WebviewWindowBuilder;

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
    let _app = lifecycle::builder(mock_builder(), move |_| {
        fs::write(&marker, "setup ran")?;
        Ok(())
    })
    .build(mock_context(noop_assets()))
    .unwrap();
    // Only the first instance gets here; a second one exits inside build().
    println!("ready");
    std::thread::spawn(|| {
        std::io::stdin().read_line(&mut String::new()).unwrap();
        std::process::exit(0);
    });
    // Pumps the plugin's window, so a second instance's message is delivered.
    let mut msg = [0usize; 8];
    // SAFETY: plain Win32 message loop on this thread's queue.
    while unsafe { GetMessageW(&mut msg, 0, 0, 0) } > 0 {
        unsafe { DispatchMessageW(&msg) };
    }
}

fn instance(marker: &Path) -> std::process::Child {
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "helper_instance", "--nocapture", "--test-threads=1"])
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
    while line.trim() != "ready" {
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

fn game_running(product: &str) -> ProductState {
    ProductState {
        product_id: ProductId(product.into()),
        installed: None,
        recommended: None,
        running: None,
        location: None,
        readiness: Readiness::Running { since: 1 },
        running_game: None,
        loader_guidance: None,
        catalog: CatalogState::Refreshed,
    }
}

/// Stand-in launcher: plays, takes the Quit decision, and exits as `app.exit` does.
/// A no-op in normal runs.
#[test]
fn helper_plays_then_quits() {
    let Some(root) = std::env::var_os(QUIT_ROOT) else {
        return;
    };
    let location = GameLocation {
        path: root.into(),
        build: "test".into(),
        store: GameStore::Unknown,
    };
    let manifest = support::release(support::PD2R_1).manifest;
    let game = play(&location, &manifest, &Readiness::Ready, &SystemGameHost).unwrap();
    fs::write(std::env::var_os(QUIT_PID).unwrap(), game.pid.to_string()).unwrap();
    assert_eq!(quit_plan(&[game_running("pd2r")]), Quit::Exit);
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
