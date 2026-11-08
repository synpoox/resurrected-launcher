//! Clauses "Hide, reopen, Quit and autostart" and "No duplicate application
//! writer", on a **real** window: a launcher process built by the real
//! [`lifecycle::builder`] on the Wry runtime (WebView2) under a private
//! identifier, with a `main` window on `about:blank`. Closing the window hides it
//! and keeps it; [`lifecycle::show_main`] (the tray's left-click and Open, and the
//! single-instance callback) shows it again; a second instance started while
//! it is hidden exits and shows it. Focus is reported, not asserted: Windows
//! may refuse a background process the foreground (manual check).

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use resurrected_launcher_lib::contract::DISPOSABLE_SENTINEL;
use resurrected_launcher_lib::lifecycle::{self, notice};
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::helper;
use crate::single_writer;

const ID: &str = "SLICE4_GATE_WINDOW_ID";
/// The temp dir the parent owns: the window process's WebView2 data and notice
/// marker go here, never under `%LOCALAPPDATA%` (#129).
const DATA: &str = "SLICE4_GATE_WINDOW_DATA";

fn until(what: &str, mut ok: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !ok() {
        if Instant::now() > deadline {
            println!("R timeout={what}");
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    true
}

fn report(key: &str, value: impl std::fmt::Display) {
    println!("R {key}={value}");
}

fn script<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    let visible = || window.is_visible().unwrap_or(false);
    report("shown", until("shown", visible));
    // The title bar's Close (`close_window`) is `window.close()` on `main`.
    window.close().unwrap();
    until("hidden", || !visible());
    report("closed_kept", app.get_webview_window("main").is_some());
    report("closed_visible", visible());
    lifecycle::show_main(app);
    report("reopened_visible", until("reopened", visible));
    window.close().unwrap();
    report("hidden_again", until("hidden again", || !visible()));
    println!("READY");
    // The parent starts a second instance, then writes a line.
    std::io::stdin().read_line(&mut String::new()).unwrap();
    report("second_instance_visible", until("second instance", visible));
    report(
        "second_instance_focused",
        window.is_focused().unwrap_or(false),
    );
    std::process::exit(0);
}

/// Not a gate test: the real-window launcher process. Does nothing on its own.
#[test]
#[ignore = "run only as a child process by the real-window gate test"]
fn helper_real_window() {
    let (Some(id), Some(data)) = (std::env::var_os(ID), std::env::var_os(DATA)) else {
        return;
    };
    let data = PathBuf::from(data);
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = id.into_string().unwrap();
    lifecycle::builder(tauri::Builder::default().any_thread(), move |app| {
        // The first-close notice counts as shown: no real toast, and nothing is
        // registered for this private identifier.
        let dir = data.join("notice");
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(notice::MARKER), b"")?;
        app.manage(notice::Dir(dir));
        let blank = WebviewUrl::External("about:blank".parse().unwrap());
        let window = WebviewWindowBuilder::new(app, "main", blank)
            .inner_size(1280.0, 720.0)
            .data_directory(data.join("webview"))
            .build()?;
        let handle = app.handle().clone();
        std::thread::spawn(move || script(&handle, &window));
        Ok(())
    })
    .build(context)
    .unwrap()
    .run(|_, _| {});
}

#[test]
fn close_hides_the_window_and_reopen_and_a_second_instance_show_it() {
    let id = helper::private_identifier("window");
    let data = tempfile::Builder::new()
        .prefix("slice4-window")
        .tempdir()
        .unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "real_window::helper_real_window",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(ID, &id)
        .env(DATA, data.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut results = HashMap::new();
    let mut line = String::new();
    let mut read_until = |stop: &str, results: &mut HashMap<String, String>| loop {
        line.clear();
        if out.read_line(&mut line).unwrap() == 0 {
            return false;
        }
        if let Some(at) = line.find("R ") {
            if let Some((k, v)) = line[at + 2..].trim().split_once('=') {
                results.insert(k.to_owned(), v.to_owned());
            }
        }
        if line.trim_end().ends_with(stop) {
            return true;
        }
    };
    let ready = read_until("READY", &mut results);
    assert!(ready, "the window process ended early: {results:?}");
    for key in ["shown", "closed_kept", "reopened_visible", "hidden_again"] {
        assert_eq!(
            results.get(key).map(String::as_str),
            Some("true"),
            "{key}: {results:?}"
        );
    }
    assert_eq!(results["closed_visible"], "false", "Close hides the window");

    // A second launch while the window is hidden in the tray.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("PD2Resurrected");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join(DISPOSABLE_SENTINEL), b"").unwrap();
    let marker = tmp.path().join("second");
    let mut second = Command::new(std::env::current_exe().unwrap())
        .args([
            "single_writer::helper_instance",
            "--exact",
            "--ignored",
            "--test-threads=1",
        ])
        .env(single_writer::ID, &id)
        .env(single_writer::ROOT, &root)
        .env(single_writer::MARKER, &marker)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(single_writer::exits_in(&mut second, 15).success());
    assert!(!marker.exists(), "the second instance ran setup");
    child.stdin.take().unwrap().write_all(b"\n").unwrap();
    read_until("\u{0}", &mut results);
    assert!(single_writer::exits_in(&mut child, 15).success());
    assert_eq!(
        results.get("second_instance_visible").map(String::as_str),
        Some("true"),
        "the second instance shows the first's window: {results:?}"
    );
    // Reported for the record; Windows decides whether a background process
    // may take the foreground.
    eprintln!(
        "second instance focused the window: {}",
        results["second_instance_focused"]
    );
    helper::assert_no_app_data(&id);
    helper::remove_when_released(data);
}
