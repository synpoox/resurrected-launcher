//! What the slice 4 gate adds to the slice 3 rig: real helper processes, the
//! `examples/fake-d2rloader.exe` stand-in, a program that exits with a known
//! code, progress listeners and a private single-instance identifier.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{channel, Receiver};
use std::sync::OnceLock;

use resurrected_launcher_lib::app::PROGRESS_EVENT;
use resurrected_launcher_lib::contract::ProgressEvent;
use serde_json::Value;
use tauri::{Listener, Manager};

use crate::rig::Rig;

type Handle = isize;

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
    fn TerminateProcess(process: Handle, code: u32) -> i32;
    fn WaitForSingleObject(handle: Handle, ms: u32) -> u32;
    fn GetExitCodeProcess(process: Handle, code: *mut u32) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
}

const PROCESS_TERMINATE: u32 = 0x1;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const SYNCHRONIZE: u32 = 0x0010_0000;
const STILL_ACTIVE: u32 = 259;

/// Whether process `pid` is still running (opened by pid, exit code read).
pub fn alive(pid: u32) -> bool {
    // SAFETY: plain Win32 calls; the handle is closed before returning.
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h == 0 {
            return false;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code) != 0;
        CloseHandle(h);
        ok && code == STILL_ACTIVE
    }
}

/// Ends process `pid` (a fake game the test started) and waits for it.
pub fn kill(pid: u32) {
    // SAFETY: plain Win32 calls; the handle is closed before returning.
    unsafe {
        let h = OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, 0, pid);
        if h != 0 {
            TerminateProcess(h, 1);
            WaitForSingleObject(h, 10_000);
            CloseHandle(h);
        }
    }
}

/// `examples/fake-d2rloader.exe` of this test's profile. `cargo test` builds
/// examples; built here once if missing (as in `tests/lifecycle.rs`).
pub fn fake_d2rloader() -> &'static Path {
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

/// The exit code of [`quick_exit`].
pub const QUICK_EXIT_CODE: i32 = 7;

/// A GUI program, compiled once with the pinned `rustc`, that waits a moment
/// and exits with [`QUICK_EXIT_CODE`]: a game that fails at startup.
pub fn quick_exit() -> &'static Path {
    static EXE: OnceLock<PathBuf> = OnceLock::new();
    EXE.get_or_init(|| {
        let dir = tempfile::Builder::new()
            .prefix("slice4-quick-exit")
            .tempdir()
            .unwrap()
            .keep();
        let src = dir.join("quick_exit.rs");
        fs::write(
            &src,
            format!(
                "#![windows_subsystem = \"windows\"]\nfn main() {{ \
                 std::thread::sleep(std::time::Duration::from_millis(700)); \
                 std::process::exit({QUICK_EXIT_CODE}); }}\n"
            ),
        )
        .unwrap();
        let out = dir.join("quick_exit.exe");
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let built = Command::new(rustc)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(["--edition", "2021", "-C", "debuginfo=0", "-o"])
            .arg(&out)
            .arg(&src)
            .output()
            .expect("rustc runs");
        assert!(
            built.status.success(),
            "building the quick-exit program: {}",
            String::from_utf8_lossy(&built.stderr)
        );
        out
    })
}

/// Every `operation-progress` payload of `rig`'s app, in order.
pub fn progress(rig: &Rig) -> Receiver<ProgressEvent> {
    let (tx, rx) = channel();
    rig.webview
        .app_handle()
        .listen_any(PROGRESS_EVENT, move |e| {
            let v: Value = serde_json::from_str(e.payload()).unwrap();
            let _ = tx.send(serde_json::from_value(v["data"].clone()).unwrap());
        });
    rx
}

/// A single-instance identifier no launcher uses: the real one is
/// `com.projectd2r.launcher`, and the other suites use `mock_context`'s.
pub fn private_identifier(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(
        "com.projectd2r.launcher.slice4-gate.{tag}.{}.{nanos}",
        std::process::id()
    )
}

/// Fails if anything exists under `%LOCALAPPDATA%\<id>`: a test launcher's data
/// belongs in a temp dir, never the user's app data (#129).
pub fn assert_no_app_data(id: &str) {
    let local = std::env::var_os("LOCALAPPDATA").expect("LOCALAPPDATA is set");
    let dir = Path::new(&local).join(id);
    assert!(!dir.exists(), "test launcher wrote to {}", dir.display());
}

/// Removes `dir`, retrying while WebView2's helper processes, which outlive the
/// window process by a moment, still hold files in it.
pub fn remove_when_released(dir: tempfile::TempDir) {
    let path = dir.keep();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while fs::remove_dir_all(&path).is_err() && path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "{} still in use",
            path.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
