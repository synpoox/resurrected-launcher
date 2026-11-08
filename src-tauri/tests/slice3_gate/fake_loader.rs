//! A fake D2RLoader: a real Windows GUI program, compiled once per test run with
//! the pinned `rustc`, that records what it was started with and stays running.
//!
//! Started as `<dir>\<name>`, it writes `<dir>\<name>.started` (its pid, working
//! directory and every `argv` entry, one per line, written atomically), then runs
//! until `<dir>\<name>.stop` appears or [`MAX_LIFETIME_SECS`] pass, so a failed
//! test never leaves it behind for long. It is never a retail file and never
//! loads a game.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use resurrected_launcher_lib::game::{GameHost, SystemGameHost};

pub const MAX_LIFETIME_SECS: u64 = 180;

const SOURCE: &str = r#"
#![windows_subsystem = "windows"]
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn sibling(exe: &PathBuf, suffix: &str) -> PathBuf {
    let mut s = exe.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

fn main() {
    let exe = std::env::current_exe().unwrap();
    let stop = sibling(&exe, ".stop");
    let _ = std::fs::remove_file(&stop);
    let mut text = format!("{}\n{}\n", std::process::id(), std::env::current_dir().unwrap().display());
    for arg in std::env::args() {
        text.push_str(&arg);
        text.push('\n');
    }
    let tmp = sibling(&exe, ".started.tmp");
    std::fs::write(&tmp, text).unwrap();
    std::fs::rename(&tmp, sibling(&exe, ".started")).unwrap();
    let until = Instant::now() + Duration::from_secs(MAX);
    while !stop.exists() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
    let why = if stop.exists() { "stopped" } else { "timed out" };
    let _ = std::fs::write(sibling(&exe, ".exited"), why);
}
"#;

/// The compiled fake loader.
pub fn exe() -> &'static Path {
    static EXE: OnceLock<PathBuf> = OnceLock::new();
    EXE.get_or_init(|| {
        let dir = tempfile::Builder::new()
            .prefix("slice3-fake-loader")
            .tempdir()
            .unwrap()
            .keep();
        let src = dir.join("fake_loader.rs");
        fs::write(
            &src,
            SOURCE.replace("MAX", &format!("{MAX_LIFETIME_SECS}u64")),
        )
        .unwrap();
        let out = dir.join("fake_loader.exe");
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        // From the package dir, so rustup picks the pinned rust-toolchain.toml.
        let built = Command::new(rustc)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(["--edition", "2021", "-C", "debuginfo=0", "-o"])
            .arg(&out)
            .arg(&src)
            .output()
            .expect("rustc runs");
        assert!(
            built.status.success(),
            "building the fake loader: {}",
            String::from_utf8_lossy(&built.stderr)
        );
        out
    })
}

/// Copies the fake loader to `dir\name`; returns that path.
pub fn install(dir: &Path, name: &str) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    let to = dir.join(name);
    fs::copy(exe(), &to).unwrap();
    to
}

fn sibling(exe: &Path, suffix: &str) -> PathBuf {
    let mut s = exe.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// What one fake loader process recorded about its own start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Started {
    pub pid: u32,
    pub cwd: PathBuf,
    /// Every argv entry, `argv[0]` included.
    pub argv: Vec<String>,
}

/// Forgets an earlier start record of `exe`, so the next one is fresh.
pub fn forget(exe: &Path) {
    let _ = fs::remove_file(sibling(exe, ".started"));
}

/// Waits for `exe`'s start record.
pub fn wait_started(exe: &Path) -> Started {
    let path = sibling(exe, ".started");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(text) = fs::read_to_string(&path) {
            let mut lines = text.lines();
            return Started {
                pid: lines.next().unwrap().parse().unwrap(),
                cwd: PathBuf::from(lines.next().unwrap()),
                argv: lines.map(str::to_owned).collect(),
            };
        }
        assert!(Instant::now() < deadline, "{} never started", exe.display());
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Whether process `pid` with image `exe` is running, by a real process listing.
pub fn is_running(pid: u32, exe: &Path) -> bool {
    let want = exe.to_string_lossy().to_lowercase();
    SystemGameHost
        .processes()
        .unwrap()
        .iter()
        .any(|p| p.pid == pid && p.image.to_string_lossy().to_lowercase() == want)
}

/// One running fake loader. Dropping it stops the process and waits for its exit.
pub struct FakeLoader {
    pub exe: PathBuf,
    pub started: Started,
    child: Option<Child>,
}

impl FakeLoader {
    /// Starts `exe` directly, as a player starting the game outside the launcher.
    pub fn start(exe: &Path) -> Self {
        forget(exe);
        let child = Command::new(exe)
            .current_dir(exe.parent().unwrap())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let started = wait_started(exe);
        assert_eq!(started.pid, child.id());
        Self {
            exe: exe.to_owned(),
            started,
            child: Some(child),
        }
    }

    /// Takes over a fake loader someone else started (the launcher's Play).
    pub fn adopt(exe: &Path) -> Self {
        Self {
            exe: exe.to_owned(),
            started: wait_started(exe),
            child: None,
        }
    }

    pub fn pid(&self) -> u32 {
        self.started.pid
    }

    pub fn is_running(&self) -> bool {
        is_running(self.pid(), &self.exe)
    }

    /// Stops the process and waits until it is gone.
    pub fn stop(mut self) {
        self.stop_now();
    }

    fn stop_now(&mut self) {
        let _ = fs::write(sibling(&self.exe, ".stop"), b"");
        if let Some(mut child) = self.child.take() {
            let _ = child.wait();
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        while self.is_running() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for FakeLoader {
    fn drop(&mut self) {
        self.stop_now();
    }
}
