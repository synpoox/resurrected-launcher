//! Clause "No duplicate application writer": two launcher processes built by the
//! real [`lifecycle::builder`] under one private identifier. The first holds the
//! product root's [`OperationLock`], as it does through every operation and
//! recovery. The second exits during plugin setup, before `setup` (where the
//! real launcher constructs `Launcher`, the only owner of operations) runs, so it
//! never reaches the lock; the first keeps running and keeps its lock.
//! Focusing the first window is not observable on the mock runtime (manual check).

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use resurrected_launcher_lib::contract::{ErrorCode, WriteGuard, DISPOSABLE_SENTINEL};
use resurrected_launcher_lib::install::lock::OperationLock;
use resurrected_launcher_lib::lifecycle;
use tauri::test::{mock_builder, mock_context, noop_assets};

use crate::helper;

pub(crate) const ID: &str = "SLICE4_GATE_INSTANCE_ID";
pub(crate) const ROOT: &str = "SLICE4_GATE_INSTANCE_ROOT";
pub(crate) const MARKER: &str = "SLICE4_GATE_INSTANCE_MARKER";

#[link(name = "user32")]
extern "system" {
    fn GetMessageW(msg: *mut [usize; 8], hwnd: isize, min: u32, max: u32) -> i32;
    fn DispatchMessageW(msg: *const [usize; 8]) -> isize;
}

/// Not a gate test: one launcher instance. Its setup writes the marker, then
/// takes the root's operation lock and holds it until stdin closes, pumping the
/// single-instance window's messages meanwhile. Does nothing when run on its own.
#[test]
#[ignore = "run only as a child process by the single-writer gate test"]
fn helper_instance() {
    let (Some(id), Some(root), Some(marker)) = (
        std::env::var_os(ID),
        std::env::var_os(ROOT),
        std::env::var_os(MARKER),
    ) else {
        return;
    };
    let (root, marker) = (PathBuf::from(root), PathBuf::from(marker));
    let mut context = mock_context(noop_assets());
    context.config_mut().identifier = id.into_string().unwrap();
    lifecycle::builder(mock_builder(), move |_| {
        fs::write(&marker, "setup ran")?;
        let lock = OperationLock::acquire(&WriteGuard::check(&root)?)?;
        println!("ready");
        std::thread::spawn(move || {
            std::io::stdin().read_line(&mut String::new()).unwrap();
            drop(lock);
            std::process::exit(0);
        });
        let mut msg = [0usize; 8];
        // SAFETY: plain Win32 message loop on this thread's queue.
        while unsafe { GetMessageW(&mut msg, 0, 0, 0) } > 0 {
            unsafe { DispatchMessageW(&msg) };
        }
        Ok(())
    })
    .build(context)
    .unwrap()
    .run(|_, _| {});
}

pub(crate) fn instance(id: &str, root: &std::path::Path, marker: &std::path::Path) -> Child {
    Command::new(std::env::current_exe().unwrap())
        .args([
            "single_writer::helper_instance",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(ID, id)
        .env(ROOT, root)
        .env(MARKER, marker)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

pub(crate) fn exits_in(child: &mut Child, secs: u64) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("instance did not exit");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn held(guard: &WriteGuard) -> bool {
    match OperationLock::acquire(guard) {
        Ok(_) => false,
        Err(e) => {
            assert_eq!(e.code, ErrorCode::OperationInProgress, "{e:?}");
            true
        }
    }
}

#[test]
fn a_second_instance_never_takes_the_operation_lock() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("PD2Resurrected");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join(DISPOSABLE_SENTINEL), b"").unwrap();
    let guard = WriteGuard::check(&root).unwrap();
    let id = helper::private_identifier("instance");

    let first_marker = tmp.path().join("first");
    let mut first = instance(&id, &root, &first_marker);
    let mut out = BufReader::new(first.stdout.take().unwrap());
    let mut line = String::new();
    // libtest may print "test ... " on the same line.
    while !line.trim_end().ends_with("ready") {
        line.clear();
        assert_ne!(out.read_line(&mut line).unwrap(), 0, "first instance ended");
    }
    assert!(first_marker.is_file());
    assert!(held(&guard), "the first instance holds the lock");

    // Two more launches while the first runs.
    for n in [2, 3] {
        let marker = tmp.path().join(format!("instance-{n}"));
        let mut second = instance(&id, &root, &marker);
        let status = exits_in(&mut second, 10);
        assert!(status.success(), "instance {n}: {status:?}");
        assert!(
            !marker.exists(),
            "instance {n} ran setup, where the launcher would construct Launcher"
        );
        assert!(
            first.try_wait().unwrap().is_none(),
            "the first instance keeps running"
        );
        assert!(held(&guard), "the first instance still holds the lock");
    }

    first.stdin.take().unwrap().write_all(b"\n").unwrap();
    assert!(exits_in(&mut first, 10).success());
    assert!(
        !held(&guard),
        "the lock is free once the first instance exits"
    );
    helper::assert_no_app_data(&id);
}
