//! Clause "Hide, reopen, Quit and autostart", on the mock runtime: the
//! first-close notice is shown once, Quit decides per state as the spec says,
//! and `--autostart` is wired to the hidden start. Close hiding a real window
//! and reopening it is [`crate::real_window`]. Seeing the toast, the tray icon
//! and the autostart start in the tray are manual checks on Windows 11.

use std::cell::Cell;
use std::fs;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use resurrected_launcher_lib::app::{self, TRAY_EVENT};
use resurrected_launcher_lib::contract::{Phase, ProductId, Readiness, CONTRACT_VERSION};
use resurrected_launcher_lib::lifecycle::{self, notice, Quit, AUTOSTART_FLAG};
use serde_json::{json, Value};
use tauri::{Listener, Manager};

use crate::rig::*;
use crate::support::{self, Reply};

#[test]
fn the_first_close_notice_is_shown_once() {
    let dir = tempfile::tempdir().unwrap();
    let shown = Cell::new(0);
    let show = || {
        shown.set(shown.get() + 1);
        Ok(())
    };
    assert_eq!(notice::attempt(dir.path(), show), notice::Outcome::Shown);
    assert!(dir.path().join(notice::MARKER).is_file());
    for _ in 0..3 {
        assert_eq!(
            notice::attempt(dir.path(), show),
            notice::Outcome::AlreadyShown
        );
    }
    assert_eq!(shown.get(), 1, "shown once, on the first close only");
    assert_eq!(
        notice::TEXT,
        "Still running in the tray. Downloads and update checks carry on here. \
         To exit, right-click the icon and choose Quit."
    );

    // A failed toast leaves no marker, so the next close tries again.
    let dir = tempfile::tempdir().unwrap();
    let failed = notice::attempt(dir.path(), || Err("toast refused".into()));
    assert_eq!(failed, notice::Outcome::Failed("toast refused".into()));
    assert!(!dir.path().join(notice::MARKER).exists());
    assert_eq!(
        notice::attempt(dir.path(), || Ok(())),
        notice::Outcome::Shown
    );
}

fn tray_requests(rig: &Rig) -> Receiver<Value> {
    let (tx, rx) = channel();
    rig.webview.app_handle().listen_any(TRAY_EVENT, move |e| {
        let _ = tx.send(serde_json::from_str(e.payload()).unwrap());
    });
    rx
}

fn quit(rig: &Rig) -> Quit {
    lifecycle::quit(rig.webview.app_handle())
}

fn asked_quit_while_busy(requests: &Receiver<Value>) {
    let asked = requests.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(asked["contract_version"], CONTRACT_VERSION);
    assert_eq!(
        asked["data"],
        json!({ "action": "quit_while_busy", "product_id": PRODUCT, "error": null })
    );
}

#[test]
fn quit_when_idle_exits_at_once() {
    let rig = Rig::new();
    rig.adopted();
    assert_eq!(rig.state().readiness, Readiness::Ready);
    let requests = tray_requests(&rig);
    assert_eq!(quit(&rig), Quit::Exit);
    assert!(requests.try_recv().is_err(), "no dialog");
    // Quit accepted: nothing starts before the process ends.
    assert_eq!(code(rig.play().unwrap_err()), "operation_in_progress");
}

#[test]
fn quit_with_an_update_available_exits_at_once() {
    let rig = Rig::new();
    rig.adopted();
    rig.publish(&real_layout(REAL_1B));
    assert_eq!(rig.state().readiness, Readiness::UpdateAvailable);
    assert_eq!(quit(&rig), Quit::Exit);
}

#[test]
fn quit_in_an_error_state_exits_at_once() {
    let rig = Rig::new();
    rig.adopted();
    fs::remove_file(rig.d2r.join("D2R.exe")).unwrap();
    assert_eq!(rig.state().readiness, Readiness::D2rMissing);
    assert_eq!(quit(&rig), Quit::Exit);
}

#[test]
fn quit_while_downloading_asks_and_keeps_downloading() {
    let rig = Rig::new();
    let r1 = support::release(support::PD2R_1);
    rig.publish(&r1);
    rig.dirs.as_ref().unwrap().game.product_root(&r1);
    rig.start_up();
    rig.server.route(
        &r1.archive_path(),
        Reply::Delayed(
            Duration::from_secs(2),
            Box::new(Reply::Body(r1.archive.clone())),
        ),
    );
    let requests = tray_requests(&rig);
    let insp = rig.inspect("install", json!({})).unwrap();
    rig.start("install", json!({}), &insp.token).unwrap();
    assert_eq!(quit(&rig), Quit::Busy(ProductId(PRODUCT.into())));
    asked_quit_while_busy(&requests);
    // Keep downloading: the install carries on to its end.
    assert_eq!(rig.outcome()["status"], "succeeded");
    assert_eq!(quit(&rig), Quit::Exit);
}

/// Holds the update inside its first `Activating` event until `release` sends.
/// Events emitted meanwhile (Quit's requests) are delivered once it returns.
#[test]
fn quit_while_applying_or_cancelling_asks_and_waits_for_the_safe_stop() {
    let rig = Rig::new();
    let r1 = support::release(support::PD2R_1);
    rig.publish(&r1);
    rig.dirs.as_ref().unwrap().game.product_root(&r1);
    rig.start_up();
    assert_eq!(rig.run("install", json!({}))["status"], "succeeded");
    rig.publish(&support::release(support::PD2R_2));

    let (reached_tx, reached) = channel();
    let (release_tx, release) = channel::<()>();
    let release = std::sync::Mutex::new(Some(release));
    rig.webview
        .app_handle()
        .listen_any(app::PROGRESS_EVENT, move |e| {
            let v: Value = serde_json::from_str(e.payload()).unwrap();
            if v["data"]["phase"] == serde_json::to_value(Phase::Activating).unwrap() {
                if let Some(wait) = release.lock().unwrap().take() {
                    let _ = reached_tx.send(());
                    let _ = wait.recv_timeout(Duration::from_secs(60));
                }
            }
        });
    let requests = tray_requests(&rig);
    let insp = rig.inspect("update", json!({})).unwrap();
    rig.start("update", json!({}), &insp.token).unwrap();
    reached.recv_timeout(Duration::from_secs(60)).unwrap();

    // `applying`: Quit opens the window on the same dialog and does not exit.
    assert_eq!(quit(&rig), Quit::Busy(ProductId(PRODUCT.into())));
    // Cancel download and quit: `cancelling` is still busy until the safe stop.
    let cancel = json!({ "request": { "contract_version": CONTRACT_VERSION, "data": { "product_id": PRODUCT } } });
    rig.invoke("cancel_operation", cancel).unwrap();
    assert!(rig.state().cancelling);
    assert_eq!(quit(&rig), Quit::Busy(ProductId(PRODUCT.into())));

    release_tx.send(()).unwrap();
    // Both Quits asked the window for the dialog.
    asked_quit_while_busy(&requests);
    asked_quit_while_busy(&requests);
    let outcome = rig.outcome();
    assert!(
        outcome["status"] == "cancelled" || outcome["status"] == "succeeded",
        "{outcome}"
    );
    assert_eq!(rig.state().running, None);
    // After the safe stop, the dialog's quit goes through.
    assert_eq!(quit(&rig), Quit::Exit);
}

/// Recorded source check: the hidden start and the tray-only second instance
/// are not observable on the mock runtime (manual check on Windows 11).
#[test]
fn autostart_starts_hidden_and_a_second_autostart_stays_in_the_tray() {
    assert_eq!(AUTOSTART_FLAG, "--autostart");
    let src =
        fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lifecycle/mod.rs")).unwrap();
    let flat: String = src.split_whitespace().collect();
    assert!(
        flat.contains("letautostart=std::env::args().skip(1).any(|a|a==AUTOSTART_FLAG);"),
        "run() reads --autostart"
    );
    assert!(
        flat.contains(".visible(false)") && flat.contains("if!autostart{window.show()?;}"),
        "the window is built hidden and shown after the move unless autostart"
    );
    assert!(
        flat.contains("if!args.iter().any(|a|a==AUTOSTART_FLAG){show_main(app)}"),
        "a second instance started with --autostart does not show the window"
    );
    // Start with Windows writes exactly `"<exe>" --autostart` (tests/app_commands.rs,
    // start_with_windows_is_only_the_run_value).
    let settings =
        fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/app/settings.rs")).unwrap();
    assert!(settings.contains(r#"format!("\"{}\" --autostart", exe.display())"#));
}
